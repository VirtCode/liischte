use std::{hash::Hasher, time::Duration};

use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::{FutureExt, StreamExt, stream};
use iced::{
    Background, Color, Element, Length, Renderer, Subscription, Task, Theme,
    advanced::subscription::{EventStream, Recipe, from_recipe},
    widget::{Space, container, stack},
};
use iced_winit::futures::BoxStream;
use log::{debug, error, info};
use lucide_icons::Icon;
use serde::Deserialize;

use crate::{
    config::CONFIG,
    info::power::{
        PowerDevice, listen_ac_online, listen_battery_charge, read_ac_online,
        read_battery_capacity, read_battery_charge, read_devices,
    },
    ui::icon,
};

use super::{Status, StatusMessage};

pub const POWER_STATUS_IDENTIFIER: &str = "power";

#[derive(Deserialize)]
#[serde(default)]
struct PowerStatusConfig {
    /// force the use of a specific ac adapter
    ac: Option<String>,
    /// force the use of a specific set of batteries
    batteries: Vec<String>,

    /// polling rate to poll battery status in seconds
    polling_rate: u64,

    /// battery percentage below which it is considered critical
    critical: f64,
}

impl Default for PowerStatusConfig {
    fn default() -> Self {
        Self { ac: None, batteries: vec![], polling_rate: 30, critical: 0.1 }
    }
}

impl StatusMessage for PowerStatusMessage {}
#[derive(Clone, Debug)]
pub enum PowerStatusMessage {
    AcOnlineMessage(bool),
    BatteryChargeMessage(usize, f64),
}

struct Ac {
    device: PowerDevice,

    /// is the ac adapter providing power
    online: bool,
}

struct Battery {
    device: PowerDevice,

    /// capacity the batter has in Wh
    capacity: f64,
    /// current charge of the battery from 0 to 1
    charge: f64,
}

pub struct PowerStatus {
    config: PowerStatusConfig,

    /// tracked ac adapter
    ac: Option<Ac>,
    /// batteries which are considered
    batteries: Vec<Battery>,
}

impl PowerStatus {
    pub async fn new() -> Result<Self> {
        let config: PowerStatusConfig = CONFIG.status(POWER_STATUS_IDENTIFIER);

        info!("reading available power devices from sysfs");
        let mut ac = None;
        let mut batteries = vec![];

        for device in read_devices().await.context("failed to read power devices")? {
            debug!("checking power device with name '{}'", &device.name);

            if config.ac.as_ref() == Some(&device.name)
                || config.ac.is_none() && ac.is_none() && device.name.starts_with("AC")
            {
                ac = Some(Ac { online: read_ac_online(&device).await?, device })
            } else if config.batteries.contains(&device.name)
                || (config.batteries.is_empty() && device.name.starts_with("BAT"))
            {
                batteries.push(Battery {
                    capacity: read_battery_capacity(&device).await?,
                    charge: read_battery_charge(&device).await?,
                    device,
                });
            }
        }

        info!(
            "using ac {} and batteries [{}]",
            ac.as_ref().map(|ac| ac.device.name.as_str()).unwrap_or("<none>"),
            batteries.iter().map(|bat| bat.device.name.as_str()).collect::<Vec<_>>().join(", ")
        );

        Ok(Self { ac, batteries, config })
    }
}

#[async_trait]
impl Status for PowerStatus {
    type Message = PowerStatusMessage;

    fn subscribe(&self) -> Subscription<Self::Message> {
        Subscription::batch(vec![
            Subscription::batch(self.batteries.iter().enumerate().map(|(i, bat)| {
                from_recipe(ChargeMonitor(
                    bat.device.clone(),
                    Duration::from_secs(self.config.polling_rate),
                ))
                .with(i)
                .map(|(i, c)| PowerStatusMessage::BatteryChargeMessage(i, c))
            })),
            self.ac
                .as_ref()
                .map(|ac| {
                    from_recipe(OnlineMonitor(ac.device.clone()))
                        .map(PowerStatusMessage::AcOnlineMessage)
                })
                .unwrap_or_else(Subscription::none),
        ])
    }

    fn update(&mut self, message: &Self::Message) -> Task<Self::Message> {
        match message {
            PowerStatusMessage::AcOnlineMessage(online) => {
                if let Some(ac) = &mut self.ac {
                    ac.online = *online;
                }
            }
            PowerStatusMessage::BatteryChargeMessage(i, charge) => {
                if let Some(bat) = self.batteries.get_mut(*i) {
                    bat.charge = *charge
                }
            }
        }

        Task::none()
    }

    fn render(&self) -> Element<'_, Self::Message, Theme, Renderer> {
        if self.ac.as_ref().map(|ac| ac.online).unwrap_or_default() {
            icon(Icon::BatteryCharging).into()
        } else {
            let total = self.batteries.iter().map(|bat| bat.capacity).sum::<f64>();
            let charge =
                self.batteries.iter().map(|bat| (bat.capacity / total) * bat.charge).sum::<f64>();

            if charge < self.config.critical {
                icon(Icon::BatteryWarning).into()
            } else {
                stack![
                    icon(Icon::Battery),
                    container(container(Space::new(Length::Fill, Length::Fill)).style(|_| {
                        container::Style {
                            background: Some(Background::Color(Color::WHITE)),
                            ..Default::default()
                        }
                    }))
                    .padding([15, 19 - (10f64 * charge) as u16, 15, 5]),
                ]
                .into()
            }
        }
    }
}

struct OnlineMonitor(PowerDevice);

impl Recipe for OnlineMonitor {
    type Output = bool;

    fn hash(&self, state: &mut iced::advanced::subscription::Hasher) {
        state.write_str(&format!("ac online events for {}", self.0.name));
    }

    fn stream(self: Box<Self>, _input: EventStream) -> BoxStream<Self::Output> {
        debug!("staring ac online listener for {}", self.0.name);

        match listen_ac_online(self.0) {
            Ok(s) => s,
            Err(e) => {
                error!("failed to start ac listening: {e:#}");
                stream::empty().boxed()
            }
        }
    }
}

struct ChargeMonitor(PowerDevice, Duration);

impl Recipe for ChargeMonitor {
    type Output = f64;

    fn hash(&self, state: &mut iced::advanced::subscription::Hasher) {
        state.write_str(&format!("battery charge events for {}", self.0.name));
    }

    fn stream(self: Box<Self>, _input: EventStream) -> BoxStream<Self::Output> {
        debug!("starting battery charge listener for {}", self.0.name);
        listen_battery_charge(self.0, self.1)
    }
}
