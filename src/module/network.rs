use std::hash::Hasher as _;

use anyhow::{Context, Result};
use futures::{StreamExt, stream};
use iced::{
    Element, Renderer, Subscription, Task, Theme,
    advanced::subscription::{EventStream, Hasher, Recipe, from_recipe},
    color,
    widget::stack,
};
use iced_winit::futures::BoxStream;
use liischte_lib::networkmanager::{
    ActiveConnection, ActiveConnectionKind, NetworkManager, OwnedObjectPath, describe_path,
};
use log::{debug, trace};
use lucide_icons::Icon;
use serde::Deserialize;

use super::{Module, ModuleMessage};
use crate::{config::CONFIG, osd::OsdId, ui::icon};

pub const NETWORK_MODULE_IDENTIFIER: &str = "network";

#[derive(Deserialize, Default)]
#[serde(default)]
struct NetworkModuleConfig {
    /// enable modem manager support
    modem: bool,
}

impl ModuleMessage for NetworkMessage {}
#[derive(Clone, Debug)]
pub enum NetworkMessage {
    PrimaryConnection(Option<OwnedObjectPath>),
    ActiveConnections(Vec<ActiveConnection>),

    WirelessStrength(f64),
    CellularStrength(f64),
}

pub struct NewtorkModule {
    config: NetworkModuleConfig,
    nm: NetworkManager,

    active: Vec<ActiveConnection>,

    primary: Option<ActiveConnection>,
    primary_path: Option<OwnedObjectPath>, /* we need this if the primary is communicated before
                                            * the active */
    wireless_strength: f64,
    cellular_strength: f64,
}

impl NewtorkModule {
    pub async fn new() -> Result<Self> {
        Ok(Self {
            config: CONFIG.module(NETWORK_MODULE_IDENTIFIER),
            nm: NetworkManager::connnect().await.context("could not connect to system bus")?,

            active: vec![],
            primary: None,
            primary_path: None,

            wireless_strength: 0f64,
            cellular_strength: 0f64,
        })
    }
}

impl Module for NewtorkModule {
    type Message = NetworkMessage;

    fn subscribe(&self) -> Subscription<Self::Message> {
        let mut subs = vec![
            from_recipe(PrimaryMonitor(self.nm.clone())).map(NetworkMessage::PrimaryConnection),
            from_recipe(ActiveMonitor(self.nm.clone())).map(NetworkMessage::ActiveConnections),
        ];

        if let Some(ref primary) = self.primary
            && let Some(ref device) = primary.device
        {
            match (&primary.kind, self.config.modem) {
                (ActiveConnectionKind::Wireless, _) => {
                    subs.push(
                        from_recipe(WirelessStrengthMonitor(device.clone(), self.nm.clone()))
                            .map(NetworkMessage::WirelessStrength),
                    );
                }
                (ActiveConnectionKind::Cellular, true) => {
                    subs.push(
                        from_recipe(CellularStrengthMonitor(device.clone(), self.nm.clone()))
                            .map(NetworkMessage::CellularStrength),
                    );
                }
                _ => {}
            }
        }

        Subscription::batch(subs)
    }

    fn update(&mut self, message: &Self::Message) -> (Task<Self::Message>, Option<OsdId>) {
        match message {
            NetworkMessage::PrimaryConnection(primary) => {
                self.primary_path = primary.clone();

                if let Some(primary) = primary {
                    self.primary = self.active.iter().find(|con| con.path == *primary).cloned();
                } else {
                    self.primary = None;
                }
            }
            NetworkMessage::ActiveConnections(active) => self.active = active.clone(),
            NetworkMessage::WirelessStrength(strength) => {
                trace!("reported wireless strength: {strength}");
                self.wireless_strength = *strength
            }
            NetworkMessage::CellularStrength(strength) => {
                trace!("reported cellular strength: {strength}");
                self.cellular_strength = *strength
            }
        };

        // if we first receive the primary before the active connection
        if self.primary.is_none()
            && let Some(ref primary) = self.primary_path
        {
            self.primary = self.active.iter().find(|con| con.path == *primary).cloned();
        }

        (Task::none(), None)
    }

    fn has_status(&self) -> bool {
        true
    }

    fn render_status(&self) -> Element<'_, Self::Message, Theme, Renderer> {
        let Some(ref primary) = self.primary else { return icon(Icon::Ban).into() };

        let (symbol, background) = match primary.kind {
            ActiveConnectionKind::Wired => (Icon::ChevronsLeftRightEllipsis, None),
            ActiveConnectionKind::Wireless => (
                match () {
                    _ if self.wireless_strength > 0.75 => Icon::Wifi,
                    _ if self.wireless_strength > 0.50 => Icon::WifiHigh,
                    _ if self.wireless_strength > 0.25 => Icon::WifiLow,
                    _ => Icon::WifiZero,
                },
                Some(Icon::Wifi),
            ),
            ActiveConnectionKind::Cellular => (
                match () {
                    _ if self.cellular_strength > 0.8 => Icon::Signal,
                    _ if self.cellular_strength > 0.6 => Icon::SignalHigh,
                    _ if self.cellular_strength > 0.4 => Icon::SignalMedium,
                    _ if self.cellular_strength > 0.2 => Icon::SignalLow,
                    _ => Icon::SignalZero,
                },
                Some(Icon::Signal),
            ),
            ActiveConnectionKind::Unknown(_) => (Icon::Waypoints, None),
        };

        if CONFIG.looks.tone_opacity != 0.0
            && let Some(background) = background
        {
            stack![
                icon(background)
                    .color(CONFIG.looks.foreground.scale_alpha(CONFIG.looks.tone_opacity)),
                icon(symbol)
            ]
            .into()
        } else {
            icon(symbol).into()
        }
    }
}

struct PrimaryMonitor(NetworkManager);

impl Recipe for PrimaryMonitor {
    type Output = Option<OwnedObjectPath>;

    fn hash(&self, state: &mut Hasher) {
        state.write_str("network primary connection events");
    }

    fn stream(self: Box<Self>, _input: EventStream) -> BoxStream<Self::Output> {
        debug!("staring primary connection listener");

        stream::once(async move { self.0.listen_primary_connection().await }).flatten().boxed()
    }
}

struct ActiveMonitor(NetworkManager);

impl Recipe for ActiveMonitor {
    type Output = Vec<ActiveConnection>;

    fn hash(&self, state: &mut Hasher) {
        state.write_str("network active connections events");
    }

    fn stream(self: Box<Self>, _input: EventStream) -> BoxStream<Self::Output> {
        debug!("staring active connections listener");

        self.0.listen_active_connections()
    }
}

struct WirelessStrengthMonitor(OwnedObjectPath, NetworkManager);

impl Recipe for WirelessStrengthMonitor {
    type Output = f64;

    fn hash(&self, state: &mut Hasher) {
        state.write_str("network wireless strength events");
        state.write_str(self.0.as_str());
    }

    fn stream(self: Box<Self>, _input: EventStream) -> BoxStream<Self::Output> {
        debug!("staring wireless strength monitor for {}", describe_path(&self.0));

        self.1.listen_wireless_strength(self.0)
    }
}

struct CellularStrengthMonitor(OwnedObjectPath, NetworkManager);

impl Recipe for CellularStrengthMonitor {
    type Output = f64;

    fn hash(&self, state: &mut Hasher) {
        state.write_str("network cellular strength events");
        state.write_str(self.0.as_str());
    }

    fn stream(self: Box<Self>, _input: EventStream) -> BoxStream<Self::Output> {
        debug!("staring cellular strength monitor for {}", describe_path(&self.0));

        self.1.listen_cellular_strength(self.0)
    }
}
