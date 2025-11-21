use anyhow::{Context, Result};
use futures::StreamExt;
use log::debug;
use zbus::{Connection, proxy};

use crate::{StaticStream, StreamContext};

#[proxy(
    interface = "fr.emersion.Mako",
    default_service = "org.freedesktop.Notifications",
    default_path = "/fr/emersion/Mako"
)]
pub trait MakoInterface {
    /// set all modes that are active
    fn set_modes(&self, modes: &Vec<String>) -> zbus::Result<()>;

    /// property holding all currently activated modes
    #[zbus(property)]
    fn modes(&self) -> zbus::Result<Vec<String>>;
}

#[derive(Clone)] // everything in here's reference counted anyways
pub struct Mako {
    proxy: MakoInterfaceProxy<'static>,
}

impl Mako {
    /// connects to the mako dbus interface
    pub async fn connnect() -> Result<Self> {
        debug!("trying to connect to mako's dbus interface");

        let connection =
            Connection::session().await.context("failed to connect to dbus session bus")?;
        let proxy = MakoInterfaceProxy::new(&connection)
            .await
            .context("could not connect to mako dbus interface")?;

        Ok(Self { proxy })
    }

    /// receive which modes are currently active
    pub async fn listen_modes(self) -> StaticStream<Vec<String>> {
        const STREAM: &str = "mako modes";
        debug!("starting a listener for mako modes");

        self.proxy
            .receive_modes_changed()
            .await
            .filter_map(async |change| {
                change
                    .get()
                    .await
                    .stream_context(STREAM, "failed to get new primary connection path")
            })
            .boxed()
    }

    /// set all active modes
    pub async fn set_modes(&self, modes: &Vec<String>) -> anyhow::Result<()> {
        self.proxy.set_modes(modes).await.context("failed to set mode on dbus interface")
    }
}
