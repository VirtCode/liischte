use std::{
    hash::Hasher,
    time::Duration,
};

use async_trait::async_trait;
use futures::{FutureExt, StreamExt, stream};
use iced::{
    Background, Color, Element, Length, Renderer, Subscription, Theme,
    advanced::subscription::{EventStream, Recipe, from_recipe},
    widget::{Space, container, stack},
};
use iced_winit::futures::BoxStream;
use log::{debug, error, info};
use lucide_icons::Icon;

use crate::{
    info::power::{
        PowerDevice, listen_ac_online, listen_battery_charge, read_ac_online,
        read_battery_capacity, read_battery_charge, read_devices,
    },
    ui::icon,
};

use super::{Status, StatusMessage};

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
    /// tracked ac adapter
    ac: Option<Ac>,
    /// batteries which are considered
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

        for device in read_devices().await.unwrap() {
            debug!("checking power device with name '{}'", &device.name);

            if use_ac.as_ref().map(|ac| *ac == device.name).unwrap_or_default()
                || (use_ac.is_none() && self.ac.is_none() && device.name.starts_with("AC"))
            {
                self.ac = Some(Ac { online: read_ac_online(&device).await.unwrap(), device })
            } else if use_batteries.iter().any(|bat| *bat == device.name)
                || (use_batteries.is_empty() && device.name.starts_with("BAT"))
            {
                self.batteries.push(Battery {
                    capacity: read_battery_capacity(&device).await.unwrap(),
                    charge: read_battery_charge(&device).await.unwrap(),
                    device,
                });
            }
        }

        info!(
            "using ac {} and batteries [{}]",
            self.ac.as_ref().map(|ac| ac.device.name.as_str()).unwrap_or("<none>"),
            self.batteries
                .iter()
                .map(|bat| bat.device.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    fn subscribe(&self) -> Subscription<Self::Message> {
        let polling_rate = Duration::from_secs(30);

        Subscription::batch(vec![
            Subscription::batch(self.batteries.iter().enumerate().map(|(i, bat)| {
                from_recipe(ChargeMonitor(bat.device.clone(), polling_rate))
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

    fn update(&mut self, message: &Self::Message) {
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
    }

    fn render(&self) -> Element<'_, Self::Message, Theme, Renderer> {
        let warning = 0.1;

        if self.ac.as_ref().map(|ac| ac.online).unwrap_or_default() {
            icon(Icon::BatteryCharging).into()
        } else {
            let total = self.batteries.iter().map(|bat| bat.capacity).sum::<f64>();
            let charge =
                self.batteries.iter().map(|bat| (bat.capacity / total) * bat.charge).sum::<f64>();

            if charge < warning {
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
