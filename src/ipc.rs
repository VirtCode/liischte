use std::{env, hash::Hasher as _, path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use futures::StreamExt;
use iced::{
    Subscription,
    advanced::subscription::{EventStream, Hasher, Recipe, from_recipe},
};
use iced_winit::futures::BoxStream;
use liischte_lib::StreamContext;
use log::{debug, info, trace, warn};
use serde::{Deserialize, Serialize};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
    sync::broadcast::{self, Receiver},
};
use tokio_stream::wrappers::BroadcastStream;

use crate::ui::window::WindowLayer;

/// path where the unix socket is located
fn socket_path() -> PathBuf {
    if let Ok(path) = env::var("LIISCHTE_SOCKET") {
        PathBuf::from(path)
    } else if let Ok(runtime) = env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime).join("liischte.sock")
    } else {
        PathBuf::from("/tmp/liischte.sock")
    }
}

/// message passed over ipc
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum IpcMessage {
    ModuleUpdate(String, String),
    LayerChange(Option<WindowLayer>),
}

/// this implements an ipc server which can receive messages
pub struct IpcServer {
    broadcast: Arc<Receiver<IpcMessage>>,
}

impl IpcServer {
    /// runs the ipc server
    pub async fn run() -> Result<Self> {
        let path = socket_path();

        info!("opening ipc socket at `{}`", path.to_string_lossy());
        _ = fs::remove_file(&path).await;

        let (tx, rx) = broadcast::channel(8);

        let listener = UnixListener::bind(path)?;
        tokio::spawn(async move {
            loop {
                let Some((mut stream, a)) = listener
                    .accept()
                    .await
                    .stream_context("unix socket stream", "failed to accept listener")
                else {
                    continue;
                };

                trace!(
                    "got ipc connection from `{}`",
                    a.as_pathname().and_then(|p| p.to_str()).unwrap_or("<unknown>")
                );

                let mut buf = [0u8; 1024];

                let Some(len) = stream
                    .read(&mut buf)
                    .await
                    .stream_context("unix socket stream", "failed to read from listener")
                else {
                    continue;
                };

                let Some(msg) = serde_json::from_slice(&buf[0..len])
                    .stream_context("unix socket stream", "failed to deserialize from listener")
                else {
                    continue;
                };

                if let Err(e) = tx.send(msg) {
                    warn!("failed to send to ipc stream, closing ipc: {e:#}");
                    return;
                }
            }
        });

        Ok(Self { broadcast: Arc::new(rx) })
    }

    /// returns a subscription which will fire on ipc events
    pub fn get_subscription(&self) -> Subscription<IpcMessage> {
        from_recipe(IpcMonitor(self.broadcast.clone()))
    }
}

/// sends to the ipc socket as a client
pub async fn send(msg: IpcMessage) -> Result<()> {
    UnixStream::connect(socket_path())
        .await
        .context("failed to connect to ipc socket")?
        .write_all(&serde_json::to_vec(&msg).context("failed to serialize message")?)
        .await
        .context("failed to write to ipc socket")
}

struct IpcMonitor(Arc<Receiver<IpcMessage>>);

impl Recipe for IpcMonitor {
    type Output = IpcMessage;

    fn hash(&self, state: &mut Hasher) {
        state.write_str("ipc stream");
    }

    fn stream(self: Box<Self>, _input: EventStream) -> BoxStream<Self::Output> {
        debug!("staring ipc stream subscription");

        BroadcastStream::new(self.0.resubscribe())
            .filter_map(async |r| r.stream_context("ipc", "failed to receive from broadcast"))
            .boxed()
    }
}
