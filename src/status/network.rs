use std::hash::Hasher as _;

use futures::{StreamExt, stream};
use iced::{
    Element, Renderer, Subscription, Task, Theme,
    advanced::subscription::{EventStream, Hasher, Recipe, from_recipe},
};
use iced_winit::futures::BoxStream;
use liischte_lib::networkmanager::{
    ActiveConnection, ActiveConnectionKind, NetworkManager, NetworkObject,
};
use log::debug;
use lucide_icons::Icon;

use crate::{
    status::{Status, StatusMessage},
    ui::icon,
};

pub const NETWORK_STATUS_IDENTIFIER: &str = "network";

impl StatusMessage for NetworkMessage {}
#[derive(Clone, Debug)]
pub enum NetworkMessage {
    PrimaryConnection(Option<NetworkObject>),
    ActiveConnections(Vec<ActiveConnection>),
    WirelessStrength(f64),
}

pub struct NetworkStatus {
    nm: NetworkManager,

    active: Vec<ActiveConnection>,

    primary: Option<ActiveConnection>,
    primary_path: Option<NetworkObject>, /* we need this if the primary is communicated before
                                          * the active */
    wireless_strength: f64,
}

impl NetworkStatus {
    pub async fn new() -> Self {
        Self {
            nm: NetworkManager::connnect().await.unwrap(),
            active: vec![],
            primary: None,
            primary_path: None,
            wireless_strength: 1f64,
        }
    }
}

impl Status for NetworkStatus {
    type Message = NetworkMessage;

    fn subscribe(&self) -> Subscription<Self::Message> {
        let mut subs = vec![
            from_recipe(PrimaryMonitor(self.nm.clone())).map(NetworkMessage::PrimaryConnection),
            from_recipe(ActiveMonitor(self.nm.clone())).map(NetworkMessage::ActiveConnections),
        ];

        if let Some(ref primary) = self.primary
            && primary.kind == ActiveConnectionKind::Wireless
            && let Some(ref device) = primary.device
        {
            subs.push(
                from_recipe(WirelessStrengthMonitor(device.clone(), self.nm.clone()))
                    .map(NetworkMessage::WirelessStrength),
            );
        }

        Subscription::batch(subs)
    }

    fn update(&mut self, message: &Self::Message) -> Task<Self::Message> {
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
            NetworkMessage::WirelessStrength(strength) => self.wireless_strength = *strength,
        };

        // if we first receive the primary before the active connection
        if self.primary.is_none()
            && let Some(ref primary) = self.primary_path
        {
            self.primary = self.active.iter().find(|con| con.path == *primary).cloned();
        }

        Task::none()
    }

    fn render(&self) -> Element<'_, Self::Message, Theme, Renderer> {
        let Some(ref primary) = self.primary else { return icon(Icon::Ban).into() };

        let symbol = match primary.kind {
            ActiveConnectionKind::Wired => Icon::ChevronsLeftRightEllipsis,
            ActiveConnectionKind::Wireless => match () {
                _ if self.wireless_strength > 0.75 => Icon::Wifi,
                _ if self.wireless_strength > 0.50 => Icon::WifiHigh,
                _ if self.wireless_strength > 0.25 => Icon::WifiLow,
                _ => Icon::WifiZero,
            },
            ActiveConnectionKind::Cellular => Icon::Signal,
            ActiveConnectionKind::Unknown(_) => Icon::Waypoints,
        };

        icon(symbol).into()
    }
}

struct PrimaryMonitor(NetworkManager);

impl Recipe for PrimaryMonitor {
    type Output = Option<NetworkObject>;

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

struct WirelessStrengthMonitor(NetworkObject, NetworkManager);

impl Recipe for WirelessStrengthMonitor {
    type Output = f64;

    fn hash(&self, state: &mut Hasher) {
        state.write_str("network wireless strength events");
        state.write_str(self.0.as_str());
    }

    fn stream(self: Box<Self>, _input: EventStream) -> BoxStream<Self::Output> {
        debug!("staring wireless strength listener for {}", self.0);

        self.1.listen_wireless_strength(self.0)
    }
}
