use std::future;

use futures::{FutureExt, StreamExt, stream::BoxStream};
use log::debug;
use modemmanager::dbus::modem::ModemProxy;
use rusty_network_manager::DeviceProxy;
use tokio::{select, sync::mpsc};
use tokio_stream::wrappers::ReceiverStream;
use zbus::{Connection, proxy::Builder, zvariant::OwnedObjectPath};

use crate::{
    StaticStream, StreamContext,
    networkmanager::{NetworkManager, describe_path},
};

impl NetworkManager {
    /// listen to the cellular signal strength on a given device. note that the
    /// device passed here must be a cellular device, otherwise the stream won't
    /// produce anything. this method uses ModemManager under the hood and will
    /// only work if it is running (won't produce anything if not)
    pub fn listen_cellular_strength(self, device: OwnedObjectPath) -> StaticStream<f64> {
        const STREAM: &str = "mm cellular strength";

        let (tx, rx) = mpsc::channel(1);

        fn convert_strength(strength: u32) -> f64 {
            strength as f64 / 100f64
        }

        tokio::spawn(async move {
            let Some(proxy) = DeviceProxy::new_from_path(device, &self.connection)
                .await
                .stream_context(STREAM, "failed to bind to device")
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

                fn build_proxy<'a>(
                    modem: String,
                    connection: &'a Connection,
                ) -> Result<Builder<'a, ModemProxy<'a>>, zbus::Error> {
                    // for some reason, the ModemProxy doesn't have `new_from_path`, which means we
                    // have to bind to the interface in a more manual fashion
                    ModemProxy::builder(connection)
                        .path(modem)?
                        .interface("org.freedesktop.ModemManager1.Modem")?
                        .destination("org.freedesktop.ModemManager1")
                }

                let proxy = build_proxy(modem, connection)
                    .stream_context(STREAM, "failed to create modem proxy")?
                    .build()
                    .await
                    .stream_context(STREAM, "failed to bind to modem")?;

                let stream = proxy
                    .receive_signal_quality_changed()
                    .await
                    .filter_map(async |a| {
                        a.get()
                            .await
                            .stream_context(STREAM, "failed to read signal strength")
                            .map(|(strength, _recent)| convert_strength(strength))
                    })
                    .boxed();

                Some((proxy, stream))
            }

            let mut modem = if let Some(string) =
                proxy.udi().await.stream_context(STREAM, "failed to read active modem")
            {
                track_modem(string, &self.connection).await
            } else {
                None
            };

            let mut changed_stream = proxy
                .receive_udi_changed()
                .await
                .filter_map(async |change| {
                    change.get().await.stream_context(STREAM, "failed to read changed active modem")
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
                            .stream_context(STREAM, "failed to read first signal strength")
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
