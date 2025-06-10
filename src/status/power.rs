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
use liischte_lib::power::{BatteryPowerDevice, MainsPowerDevice, PowerDevice, PowerDeviceKind};
use log::{debug, error, info};
use lucide_icons::Icon;
use serde::Deserialize;

use crate::{config::CONFIG, ui::icon};

use super::{Status, StatusMessage};

pub const POWER_STATUS_IDENTIFIER: &str = "power";

#[derive(Deserialize)]
#[serde(default)]
struct PowerStatusConfig {
    /// force the use of a specific mains supply
    mains: Option<String>,
    /// force the use of a specific set of batteries
    batteries: Vec<String>,

    /// polling rate to poll battery status in seconds
    polling_rate: u64,

    /// battery percentage below which it is considered critical
    critical: f64,
}

impl Default for PowerStatusConfig {
    fn default() -> Self {
        Self { mains: None, batteries: vec![], polling_rate: 30, critical: 0.1 }
    }
}

impl StatusMessage for PowerStatusMessage {}
#[derive(Clone, Debug)]
pub enum PowerStatusMessage {
    MainsOnlineMessage(bool),
    BatteryChargeMessage(usize, f64),
}

struct Mains {
    device: MainsPowerDevice,
    online: bool,
}

struct Battery {
    device: BatteryPowerDevice,
    capacity: f64,
    charge: f64,
}

pub struct PowerStatus {
    config: PowerStatusConfig,

    mains: Option<Mains>,
    batteries: Vec<Battery>,
}

impl PowerStatus {
    pub async fn new() -> Result<Self> {
        let config: PowerStatusConfig = CONFIG.status(POWER_STATUS_IDENTIFIER);

        info!("reading available power devices from sysfs");
        let mut mains = None;
        let mut batteries = vec![];

        for device in PowerDevice::read_all().await.context("failed to read power devices")? {
            debug!("checking power device with name `{}` ({:?})", device.name, device.kind);

            match device.kind {
                PowerDeviceKind::Mains => {
                    let device = MainsPowerDevice(device);

                    if mains.is_none()
                        && (config.mains.as_ref() == Some(&device.0.name) || config.mains.is_none())
                    {
                        mains = Some(Mains { online: device.read_online().await?, device })
                    }
                }
                PowerDeviceKind::Battery => {
                    let device = BatteryPowerDevice(device);

                    if config.batteries.is_empty() || config.batteries.contains(&device.0.name) {
                        batteries.push(Battery {
                            capacity: device.read_capacity().await?,
                            charge: device.read_charge().await?,
                            device,
                        });
                    }
                }
                _ => {}
            }
        }

        info!(
            "using ac {} and batteries [{}]",
            mains.as_ref().map(|ac| ac.device.0.name.as_str()).unwrap_or("<none>"),
            batteries.iter().map(|bat| bat.device.0.name.as_str()).collect::<Vec<_>>().join(", ")
        );

        Ok(Self { mains, batteries, config })
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
            self.mains
                .as_ref()
                .map(|ac| {
                    from_recipe(OnlineMonitor(ac.device.clone()))
                        .map(PowerStatusMessage::MainsOnlineMessage)
                })
                .unwrap_or_else(Subscription::none),
        ])
    }

    fn update(&mut self, message: &Self::Message) -> (Task<Self::Message>, bool) {
        match message {
            PowerStatusMessage::MainsOnlineMessage(online) => {
                if let Some(ac) = &mut self.mains {
                    ac.online = *online;
                }
            }
            PowerStatusMessage::BatteryChargeMessage(i, charge) => {
                if let Some(bat) = self.batteries.get_mut(*i) {
                    bat.charge = *charge
                }
            }
        }

        (Task::none(), false)
    }

    fn render(&self) -> Element<'_, Self::Message, Theme, Renderer> {
        if self.mains.as_ref().map(|ac| ac.online).unwrap_or_default() {
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

struct OnlineMonitor(MainsPowerDevice);

impl Recipe for OnlineMonitor {
    type Output = bool;

    fn hash(&self, state: &mut iced::advanced::subscription::Hasher) {
        state.write_str(&format!("ac online events for {}", self.0.0.name));
    }

    fn stream(self: Box<Self>, _input: EventStream) -> BoxStream<Self::Output> {
        debug!("staring mains online listener for {}", self.0.0.name);

        match self.0.listen_online() {
            Ok(s) => s,
            Err(e) => {
                error!("failed to start ac listening: {e:#}");
                stream::empty().boxed()
            }
        }
    }
}

struct ChargeMonitor(BatteryPowerDevice, Duration);

impl Recipe for ChargeMonitor {
    type Output = f64;

    fn hash(&self, state: &mut iced::advanced::subscription::Hasher) {
        state.write_str(&format!("battery charge events for {}", self.0.0.name));
    }

    fn stream(self: Box<Self>, _input: EventStream) -> BoxStream<Self::Output> {
        debug!("starting battery charge listener for {}", self.0.0.name);
        self.0.listen_charge(self.1)
    }
}
