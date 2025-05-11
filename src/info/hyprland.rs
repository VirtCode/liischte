use std::path::PathBuf;

use anyhow::{Context, Result};
use futures::StreamExt;
use serde::Deserialize;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};
use tokio_stream::wrappers::LinesStream;

use crate::info::util::StreamCustomExt;

use super::util::{StaticStream, StreamErrorLog};

#[derive(Deserialize, Clone, Debug)]
pub struct WorkspaceState {
    pub id: i64,
    #[serde(rename = "monitorID")]
    pub monitor_id: u64,
    #[serde(rename = "windows")]
    pub window_amount: u64,
    #[serde(rename = "hasfullscreen")]
    pub fullscreen: bool,
}

#[derive(Clone)]
pub struct HyprlandInstance {
    path: PathBuf,
}

impl HyprlandInstance {
    /// creates a new instance based on the environment variables
    pub fn env() -> Result<Self> {
        let instance = std::env::var("HYPRLAND_INSTANCE_SIGNATURE")
            .context("unable to read HYPRLAND_INSTANCE_SIGNATURE env")?;

        let runtime_dir =
            std::env::var("XDG_RUNTIME_DIR").context("unable to read XDG_RUNTIME_DIR env")?;

        Ok(Self { path: PathBuf::from(format!("{runtime_dir}/hypr/{instance}")) })
    }

    /// dispatches a command over hyprland's socket 1 and reads the result
    async fn dispatch_command(&self, command: &str) -> Result<String> {
        let mut stream = UnixStream::connect(self.path.join(".socket.sock"))
            .await
            .context("failed to connect to hl's socket 1")?;

        stream
            .write_all(format!("j/{command}").as_bytes())
            .await
            .context("failed to write to hl's socket 1")?;

        let mut buf = String::new();
        stream.read_to_string(&mut buf).await.context("failed to read from hl's socket 1")?;

        Ok(buf)
    }

    /// gets the workspace state from socket 1
    pub async fn get_all_workspaces(&self) -> Result<Vec<WorkspaceState>> {
        serde_json::from_str(
            &self
                .dispatch_command("workspaces")
                .await
                .context("failed to run `workspaces` hyprctl command")?,
        )
        .context("failed to deserialize output of `workspaces` hyprctl command")
    }

    /// gets the state of the active workspace from socket 1
    pub async fn get_active_workspace(&self) -> Result<WorkspaceState> {
        serde_json::from_str(
            &self
                .dispatch_command("activeworkspace")
                .await
                .context("failed to run `activeworkspace` hyprctl command")?,
        )
        .context("failed to deserialize output of `activeworkspace` hyprctl command")
    }

    /// runs a dispatcher to select the workspace with the given id
    pub async fn run_select_workspace(&self, id: i64) -> Result<()> {
        self.dispatch_command(&format!("dispatch workspace {id}")).await.map(|_| ())
    }

    /// runs a dispatcher to select a workspace relatively given an offset
    pub async fn run_select_workspace_relative(&self, offset: i64) -> Result<()> {
        self.dispatch_command(&format!(
            "dispatch workspace m{}{offset}",
            if offset > 0 { "+" } else { "" }
        ))
        .await
        .map(|_| ())
    }

    /// listens to socket 2 for all hyprland events and returns them as a stream
    async fn listen_events(self) -> Result<StaticStream<(String, Vec<String>)>> {
        let stream = UnixStream::connect(self.path.join(".socket2.sock"))
            .await
            .context("failed to connect to hl's socket 2")?;

        Ok(LinesStream::new(BufReader::new(stream).lines())
            .filter_map(async |result| result.ok())
            .filter_map(async |string| {
                let mut split = string.split(">>");

                Some::<(String, Vec<String>)>((
                    split.next()?.to_owned(),
                    split.next()?.split(",").map(|str| str.to_owned()).collect(),
                ))
            })
            .boxed())
    }

    /// listens to socket 2 and creates a stream that fires each time with the
    /// current workspace data
    pub async fn listen_workspaces(
        self,
        monitor_id: u64,
    ) -> Result<StaticStream<(i64, Vec<WorkspaceState>)>> {
        /// events that should trigger a whole refetch
        const REFETCH_EVENTS: &[&str] = &[
            "openwindow",
            "closewindow",
            "movewindow",
            "fullscreen",
            "moveworkspace",
            "createworkspace",
            "destroyworkspace",
            "monitorremoved",
            "monitoradded",
        ];

        let mut workspaces = self.get_all_workspaces().await?;
        workspaces.retain(|state| state.monitor_id == monitor_id && state.id >= 0);

        let active = self.get_active_workspace().await?;

        let params = (self.clone(), monitor_id);

        Ok(self
            .listen_events()
            .await?
            .scan_owning(
                (active.id, workspaces, params),
                async |(mut selected, mut state, params), (event, args)| {
                    match event.as_str() {
                        "workspacev2" => {
                            let next = args.first().and_then(|id| id.parse::<i64>().ok())?;

                            if state.iter().any(|ws| next == ws.id) {
                                selected = next;
                            }
                        }
                        event if REFETCH_EVENTS.contains(&event) => {
                            state =
                                params.0.get_all_workspaces().await.stream_log("hl workspaces")?;

                            // remove workspaces on other monitors and ignore special ones
                            state.retain(|state| state.monitor_id == params.1 && state.id >= 0);
                        }

                        // this event does not tell us anything, we don't do anything
                        _ => return Some(((selected, state, params), None)),
                    };

                    Some(((selected, state.clone(), params), Some((selected, state))))
                },
            )
            .filter_map(async |s| s)
            .boxed())
    }
}
