use std::future;

use anyhow::Context;
use futures::{FutureExt, StreamExt, stream::BoxStream};
use log::debug;
use modemmanager::dbus::{modem::ModemProxy, modem_manager::ModemManager1Proxy};
use rusty_network_manager::DeviceProxy;
use tokio::{select, sync::mpsc};
use tokio_stream::wrappers::ReceiverStream;
use zbus::{
    Connection,
    fdo::ObjectManagerProxy,
    proxy::{Builder, Defaults},
    zvariant::{ObjectPath, OwnedObjectPath as NetworkObject},
};

use crate::{
    StaticStream, StreamErrorLog,
    networkmanager::{NetworkManager, describe_path},
};

impl NetworkManager {
    /// listen to the cellular signal strength on a given device. note that the
    /// device passed here must be a cellular device, otherwise the stream won't
    /// produce anything. this method uses ModemManager under the hood and will
    /// only work if it is running (won't produce anything if not)
    pub fn listen_cellular_strength(self, device: NetworkObject) -> StaticStream<f64> {
        let (tx, rx) = mpsc::channel(1);

        fn convert_strength(strength: u32) -> f64 {
            strength as f64 / 100f64
        }

        tokio::spawn(async move {
            let Some(proxy) = DeviceProxy::new_from_path(device, &self.connection)
                .await
                .context("failed to bind to modem device")
                .stream_log("mm cellular strength")
            else {
                return;
            };

            async fn track_modem<'a>(
                modem: String,
                connection: &'a Connection,
            ) -> Option<(ModemProxy<'a>, BoxStream<'a, f64>)> {
                // we don't want to try bind empty objects
                if modem.is_empty() || modem == "/" {
                    return None;
                }

                debug!("tracking modem {} for signal strength", describe_path(&modem));

                fn build_proxy(
                    modem: String,
                    connection: &Connection,
                ) -> Result<Builder<ModemProxy>, zbus::Error> {
                    ModemProxy::builder(connection)
                        .path(modem)?
                        .interface("org.freedesktop.ModemManager1.Modem")?
                        .destination("org.freedesktop.ModemManager1")
                }

                let proxy = build_proxy(modem, connection)
                    .context("failed to bind to modem")
                    .stream_log("mm cellular strength")?
                    .build()
                    .await
                    .context("failed to bind to modem")
                    .stream_log("mm cellular strength")?;

                let stream = proxy
                    .receive_signal_quality_changed()
                    .await
                    .filter_map(async |a| {
                        a.get()
                            .await
                            .context("failed to read new access point strength")
                            .stream_log("nm wifi strength")
                            .map(|(strength, _recent)| convert_strength(strength))
                    })
                    .boxed();

                Some((proxy, stream))
            }

            let mut modem = if let Some(string) = proxy
                .udi()
                .await
                .context("failed to read active modem")
                .stream_log("mm cellular strength")
            {
                track_modem(string, &self.connection).await
            } else {
                None
            };

            let mut changed_stream = proxy
                .receive_udi_changed()
                .await
                .filter_map(async |change| {
                    change
                        .get()
                        .await
                        .context("failed to get new modem")
                        .stream_log("mm cellular strength")
                })
                .boxed();

            let mut read = true;

            loop {
                if read {
                    read = false;

                    if let Some((proxy, _)) = modem.as_ref() {
                        if let Some((strength, _recent)) = proxy
                            .signal_quality()
                            .await
                            .context("failed to read strength for new modem")
                            .stream_log("mm cellular strength")
                        {
                            if let Err(_) = tx.send(convert_strength(strength)).await {
                                debug!("cellular strength stream was dropped");
                                return;
                            }
                        }
                    }
                }

                let signal = modem
                    .as_mut()
                    .map(|(_, stream)| stream.next().boxed())
                    .unwrap_or_else(|| future::pending().boxed());

                select! {
                    biased;
                    next_ap = changed_stream.next() => {
                        let Some(next_ap) = next_ap else { continue };

                        modem = track_modem(next_ap, &self.connection).await;
                        read = true; // update the stream with the new value
                    }
                    strength = signal => {
                        let Some(strength) = strength else { continue };

                        if let Err(_) = tx.send(strength).await {
                            debug!("cellular strength stream was dropped");
                            return;
                        }
                    }
                }
            }
        });

        ReceiverStream::new(rx).boxed()
    }
}
