use std::{collections::HashMap, future};

use anyhow::{Context, Result};
use futures::{
    FutureExt, StreamExt,
    stream::{self, BoxStream},
};
use log::{debug, trace};
use rusty_network_manager::{AccessPointProxy, ActiveProxy, NetworkManagerProxy, WirelessProxy};
use tokio::{select, sync::mpsc};
use tokio_stream::wrappers::ReceiverStream;
use zbus::Connection;

use crate::{StaticStream, StreamContext, util::StreamCustomExt};

pub use zbus::zvariant::OwnedObjectPath;

#[derive(Clone)] // everything in here's reference counted anyways
pub struct NetworkManager {
    pub(crate) connection: Connection,
    pub(crate) proxy: NetworkManagerProxy<'static>,
}

impl NetworkManager {
    /// connects to the network manager dbus interface
    pub async fn connnect() -> Result<Self> {
        let connection =
            Connection::system().await.context("failed to connect to dbus system bus")?;
        let proxy = NetworkManagerProxy::new(&connection)
            .await
            .context("could not connect to network manager dbus interface")?;

        Ok(Self { connection, proxy })
    }

    /// listen to changes of the primarily used connection
    pub async fn listen_primary_connection(&self) -> StaticStream<Option<OwnedObjectPath>> {
        const STREAM: &str = "nm primary connection";

        self.proxy
            .receive_primary_connection_changed()
            .await
            .filter_map(async |change| {
                change
                    .get()
                    .await
                    .stream_context(STREAM, "failed to get new primary connection path")
                    .map(
                        |path| {
                            if path.is_empty() || path.as_str() == "/" { None } else { Some(path) }
                        },
                    )
            })
            .boxed()
    }

    /// listen to all active connections
    pub fn listen_active_connections(self) -> StaticStream<Vec<ActiveConnection>> {
        const STREAM: &str = "nm active connections";

        let (tx, rx) = mpsc::channel(1);

        tokio::spawn(async move {
            let mut trackers = HashMap::new();
            let mut states = HashMap::new();

            let paths = self
                .proxy
                .active_connections()
                .await
                .stream_context(STREAM, "failed to read active connections")
                .unwrap_or_default();

            for path in paths {
                if let Some((tracker, state)) =
                    TrackedActiveConnection::track(path, &self.connection)
                        .await
                        .stream_context(STREAM, "failed to track initial active connection")
                {
                    trackers.insert(tracker.path.clone(), tracker);
                    states.insert(state.path.clone(), state);
                }
            }

            let mut change_stream = self
                .proxy
                .receive_active_connections_changed()
                .await
                .filter_map(async |change| {
                    change
                        .get()
                        .await
                        .stream_context(STREAM, "failed to get new active connections")
                })
                .boxed();

            loop {
                if let Err(_) = tx.send(states.values().cloned().collect()).await {
                    debug!("network manager active connections stream was dropped");
                    return;
                }

                let mut streams =
                    stream::select_all(trackers.values_mut().map(|a| &mut a.stream)).boxed();

                select! {
                    biased;
                    paths = change_stream.next() => {
                        let Some(paths) = paths else { continue };
                        drop(streams); // we want to modify trackers

                        // clean unneeded ones
                        trackers.retain(|a, _| paths.contains(a));
                        states.retain(|a, _| paths.contains(a));

                        // add new ones
                        for path in paths {
                            if trackers.contains_key(&path) { continue; }

                            if let Some((tracker, state)) = TrackedActiveConnection::track(path, &self.connection)
                                .await
                                .stream_context(STREAM, "failed to track new active connection")
                            {
                                trackers.insert(tracker.path.clone(), tracker);
                                states.insert(state.path.clone(), state);
                            }
                        }
                    }
                    state = streams.next() => {
                        let Some(state) = state else { continue };

                        // update state
                        states.insert(state.path.clone(), state);
                    }
                }
            }
        });

        ReceiverStream::new(rx).boxed()
    }

    /// listen to the wifi signal strength on a given device. note that the
    /// device passed here must be a wireless device, otherwise the stream won't
    /// produce anything
    pub fn listen_wireless_strength(self, device: OwnedObjectPath) -> StaticStream<f64> {
        const STREAM: &str = "nm wireless strength";

        let (tx, rx) = mpsc::channel(1);

        fn convert_strength(strength: u8) -> f64 {
            strength as f64 / 100f64
        }

        tokio::spawn(async move {
            let Some(proxy) = WirelessProxy::new_from_path(device, &self.connection)
                .await
                .stream_context(STREAM, "failed to bind to wireless device")
            else {
                return;
            };

            async fn track_ap<'a>(
                ap: OwnedObjectPath,
                connection: &'a Connection,
            ) -> Option<(AccessPointProxy<'a>, BoxStream<'a, f64>)> {
                // we don't want to try bind non-aps
                if ap.is_empty() || ap.as_str() == "/" {
                    return None;
                }

                debug!("tracking access point {} for signal strength", describe_path(&ap));

                let proxy = AccessPointProxy::new_from_path(ap, connection)
                    .await
                    .stream_context(STREAM, "failed to bind to access point")?;

                let stream = proxy
                    .receive_strength_changed()
                    .await
                    .filter_map(async |a| {
                        a.get()
                            .await
                            .stream_context(STREAM, "failed to read new strength")
                            .map(convert_strength)
                    })
                    .boxed();

                Some((proxy, stream))
            }

            let mut ap = if let Some(path) = proxy
                .active_access_point()
                .await
                .stream_context(STREAM, "failed to read initial active access point")
            {
                track_ap(path, &self.connection).await
            } else {
                None
            };

            let mut changed_stream = proxy
                .receive_active_access_point_changed()
                .await
                .filter_map(async |change| {
                    change
                        .get()
                        .await
                        .stream_context(STREAM, "failed to read new active access point")
                })
                .boxed();

            let mut read = true;

            loop {
                if read {
                    read = false;

                    if let Some((proxy, _)) = ap.as_ref() {
                        if let Some(strength) = proxy
                            .strength()
                            .await
                            .stream_context(STREAM, "failed to read new strength")
                        {
                            if let Err(_) = tx.send(convert_strength(strength)).await {
                                debug!("wireless strength stream was dropped");
                                return;
                            }
                        }
                    }
                }

                let signal = ap
                    .as_mut()
                    .map(|(_, stream)| stream.next().boxed())
                    .unwrap_or_else(|| future::pending().boxed());

                select! {
                    biased;
                    next_ap = changed_stream.next() => {
                        let Some(next_ap) = next_ap else { continue };

                        ap = track_ap(next_ap, &self.connection).await;
                        read = true; // update the stream with the new value
                    }
                    strength = signal => {
                        let Some(strength) = strength else { continue };

                        if let Err(_) = tx.send(strength).await {
                            debug!("wireless strength stream was dropped");
                            return;
                        }
                    }
                }
            }
        });

        ReceiverStream::new(rx).boxed()
    }
}

pub struct TrackedActiveConnection<'a> {
    path: OwnedObjectPath,
    _proxy: ActiveProxy<'a>,
    stream: BoxStream<'a, ActiveConnection>,
}

impl<'a> TrackedActiveConnection<'a> {
    pub async fn track(
        path: OwnedObjectPath,
        connection: &'a Connection,
    ) -> Result<(Self, ActiveConnection)> {
        let proxy = ActiveProxy::new_from_path(path.clone(), &connection)
            .await
            .context("failed to bind to active connection")?;

        let initial = ActiveConnection {
            path: path.clone(),
            name: proxy.id().await?,
            kind: ActiveConnectionKind::parse(&proxy.type_().await?),
            state: ActiveConnectionState::parse(proxy.state().await?),
            device: proxy.devices().await?.first().cloned(),
        };

        debug!("tracking connection {} (`{}`)", describe_path(&path), initial.name);

        enum Event {
            Name(String),
            Kind(ActiveConnectionKind),
            State(ActiveConnectionState),
            Device(Option<OwnedObjectPath>),
        }

        fn describe_event(event: &Event) -> &'static str {
            match event {
                Event::Name(_) => "name",
                Event::Kind(_) => "kind",
                Event::State(_) => "state",
                Event::Device(_) => "device",
            }
        }

        let stream = stream::select_all(vec![
            proxy
                .receive_id_changed()
                .await
                .filter_map(async |val| val.get().await.ok().map(Event::Name))
                .boxed(),
            proxy
                .receive_type__changed()
                .await
                .filter_map(async |val| {
                    val.get().await.ok().map(|v| Event::Kind(ActiveConnectionKind::parse(&v)))
                })
                .boxed(),
            proxy
                .receive_state_changed()
                .await
                .filter_map(async |val| {
                    val.get().await.ok().map(|v| Event::State(ActiveConnectionState::parse(v)))
                })
                .boxed(),
            proxy
                .receive_devices_changed()
                .await
                .filter_map(async |val| {
                    val.get().await.ok().map(|v| Event::Device(v.first().cloned()))
                })
                .boxed(),
        ])
        .scan_owning(initial.clone(), async |mut state, event| {
            trace!(
                "updating `{}` property for connection {}",
                describe_event(&event),
                describe_path(&state.path)
            );

            match event {
                Event::Name(name) => state.name = name,
                Event::Kind(kind) => state.kind = kind,
                Event::State(s) => state.state = s,
                Event::Device(device) => state.device = device,
            }

            Some((state.clone(), state))
        })
        .boxed();

        Ok((Self { path, _proxy: proxy, stream }, initial))
    }
}

#[derive(Clone, Debug)]
pub struct ActiveConnection {
    /// dbus path of the connection (see primary connection)
    pub path: OwnedObjectPath,
    /// name of the connection displayed to the user
    pub name: String,
    /// type of the connection
    pub kind: ActiveConnectionKind,
    /// state of the connection
    pub state: ActiveConnectionState,
    /// underlying device if there is any
    pub device: Option<OwnedObjectPath>,
}

/// current state of a connection
/// see https://people.freedesktop.org/~lkundrak/nm-docs/nm-dbus-types.html#NMActiveConnectionState
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ActiveConnectionState {
    /// the state of the connection is unknown
    Unknown = 0,
    /// a newtork connection is being prepared
    Activating = 1,
    /// there is a connection to the network
    Activated = 2,
    /// the network connection is being torn down and cleaned up
    Deactivating = 3,
    /// the network connection is disconnected and will be removed
    Deactivated = 4,
}

impl ActiveConnectionState {
    fn parse(num: u32) -> Self {
        match num {
            1 => Self::Activating,
            2 => Self::Activated,
            3 => Self::Deactivating,
            4 => Self::Deactivated,
            _ => Self::Unknown,
        }
    }
}

/// type of a connection
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActiveConnectionKind {
    /// this is an ethernet connection
    Wired,
    /// this is a wifi connection
    Wireless,
    /// this is a cellular connection
    Cellular,
    /// type not known
    Unknown(String),
}

impl ActiveConnectionKind {
    fn parse(string: &str) -> Self {
        match string {
            "802-3-ethernet" => Self::Wired,
            "802-11-wireless" => Self::Wireless,
            "gsm" => Self::Cellular,
            a => Self::Unknown(a.to_owned()),
        }
    }
}

pub fn describe_path(path: &str) -> &str {
    let mut count = 0;

    for (i, c) in path.chars().rev().enumerate() {
        if c == '/' {
            count += 1;
        }
        if count == 2 {
            return &path[(path.len() - i)..];
        }
    }

    return path;
}
