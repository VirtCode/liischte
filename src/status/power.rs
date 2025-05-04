use std::{
    hash::Hasher,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::{FutureExt, StreamExt, stream};
use iced::{
    Element, Renderer, Subscription, Theme,
    advanced::subscription::{EventStream, Recipe, from_recipe},
    event::listen,
};
use iced_winit::futures::BoxStream;
use log::{debug, error, info, warn};
use lucide_icons::Icon;
use tokio::fs;
use tokio_stream::wrappers::ReadDirStream;
use udev::MonitorBuilder;

use crate::{system::udev::AsyncMonitorSocket, ui::icon};

use super::{Status, StatusMessage};

impl StatusMessage for PowerStatusMessage {}
#[derive(Clone, Debug)]
pub enum PowerStatusMessage {
    AcOnlineMessage(bool),
}

pub struct PowerStatus {
    ac: Option<Ac>,
    batteries: Vec<Battery>,
}

impl PowerStatus {
    pub fn new() -> Self {
        Self { ac: None, batteries: vec![] }
    }
}

#[async_trait]
impl Status for PowerStatus {
    type Message = PowerStatusMessage;

    async fn initialize(&mut self) {
        info!("reading available power devices from sysfs");

        let use_ac = &None::<String>;
        let use_batteries: Vec<String> = vec![];

        let devices = fs::read_dir("/sys/class/power_supply")
            .await
            .context("`power_supply` sysfs is required for battery information")
            .unwrap();

        let devices = ReadDirStream::new(devices)
            .filter_map(async |result| result.ok().map(|a| a.path()))
            .collect::<Vec<_>>()
            .await;

        for path in devices {
            let Some(name) = path.file_name() else { continue };
            let name = name.to_string_lossy().to_string();

            debug!("checking power device with name '{name}'");

            if use_ac.as_ref().map(|ac| *ac == name).unwrap_or_default()
                || (use_ac.is_none() && self.ac.is_none() && name.starts_with("AC"))
            {
                self.ac = Some(Ac { path, name, status: false })
            } else if use_batteries.iter().any(|bat| *bat == name)
                || (use_batteries.is_empty() && name.starts_with("BAT"))
            {
                self.batteries.push(Battery {
                    path,
                    name,
                    capacity: 0f32,
                    charge: 0f32,
                    state: BatteryState::Unknown,
                });
            }
        }

        info!(
            "using ac {} and batteries [{}]",
            self.ac.as_ref().map(|ac| ac.name.as_str()).unwrap_or("<none>"),
            self.batteries.iter().map(|bat| bat.name.as_str()).collect::<Vec<_>>().join(", ")
        );

        if let Some(ac) = &self.ac {
            debug!("listening to ac adapter over udev");
        }
    }

    fn subscribe(&self) -> Subscription<Self::Message> {
        if let Some(ac) = &self.ac {
            from_recipe(AcEvents::new(&ac.name, &ac.path)).map(PowerStatusMessage::AcOnlineMessage)
        } else {
            Subscription::none()
        }
    }

    fn update(&mut self, message: &Self::Message) {
        match message {
            PowerStatusMessage::AcOnlineMessage(online) => {
                if let Some(ac) = &mut self.ac {
                    ac.status = *online;
                }
            }
        }
    }

    fn render(&self) -> Element<'_, Self::Message, Theme, Renderer> {
        icon(if self.ac.as_ref().map(|ac| ac.status).unwrap_or_default() {
            Icon::BatteryCharging
        } else {
            Icon::Battery
        })
        .into()
    }
}

struct AcEvents {
    path: PathBuf,
    name: String,
}

impl AcEvents {
    pub fn new(name: &str, path: &Path) -> Self {
        Self { name: name.to_string(), path: path.to_owned() }
    }

    pub fn start_listener(&self) -> Result<BoxStream<bool>> {
        let socket = MonitorBuilder::new()?
            .match_subsystem_devtype("power_supply", "power_supply")?
            .listen()?;

        // yeah, cooked
        let name = Box::new(self.name.clone()).leak();
        let path = Box::leak(Box::new(self.path.clone()));

        let stream = AsyncMonitorSocket::new(socket)?
            .filter_map(async |r| {
                r.ok().and_then(|e| {
                    if e.sysname().to_string_lossy() == *name { Some(()) } else { None }
                })
            })
            .then(async |_| fs::read_to_string(path.join("online")).await.map(|s| s.trim() == "1"))
            .filter_map(async |r| match r {
                Ok(b) => Some(b),
                Err(e) => {
                    warn!("failed to read sysfs `online` file for ac: {e:#}");
                    None
                }
            })
            .boxed();

        Ok(stream)
    }
}

impl Recipe for AcEvents {
    type Output = bool;

    fn hash(&self, state: &mut iced::advanced::subscription::Hasher) {
        state.write_str(&format!("udev ac events for {}", self.name));
    }

    fn stream(self: Box<Self>, _input: EventStream) -> BoxStream<Self::Output> {
        debug!("staring udev ac listener for {}", self.name);

        match self.start_listener() {
            Ok(s) => s,
            Err(e) => {
                error!("failed to start ac listening: {e:#}");
                stream::empty().boxed()
            }
        }
    }
}

///! Implementation of power information using events from udev and the
///! power_supply sysfs
///! https://www.kernel.org/doc/Documentation/ABI/testing/sysfs-class-power
///! https://www.kernel.org/doc/Documentation/power/power_supply_class.rst

/// status of an ac adapter
struct Ac {
    /// path in the sysfs
    path: PathBuf,
    /// name of the adapter
    name: String,

    /// is the ac adapter providing power
    status: bool,
}

/// status of a battery
struct Battery {
    /// path in the sysfs
    path: PathBuf,
    /// name of the battery
    name: String,

    /// capacity the batter has in Wh
    capacity: f32,
    /// current charge of the battery from 0 to 1
    charge: f32,
    /// status of the battery
    state: BatteryState,
}

/// different statuses a battery can have
enum BatteryState {
    Unknown,
    Charging,
    Discharging,
    NotCharging,
    Full,
}
