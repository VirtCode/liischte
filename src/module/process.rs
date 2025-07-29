use std::{hash::Hasher, time::Duration};

use anyhow::{Context, Result};
use iced::{
    Element, Renderer, Subscription, Task, Theme,
    advanced::subscription::{EventStream, Recipe, from_recipe},
    widget::mouse_area,
};
use iced_winit::futures::BoxStream;
use liischte_lib::process::{
    ProcessInfo, ProcessSignal, listen_running_processes, read_running_processes, send_signal,
};
use log::{debug, error};
use lucide_icons::Icon;
use serde::Deserialize;

use crate::{
    config::CONFIG,
    module::{Module, ModuleMessage},
    osd::OsdId,
    ui::icon,
};

pub const PROCESS_MODULE_IDENTIFIER: &str = "process";

#[derive(Deserialize)]
#[serde(default)]
struct ProcessModuleConfig {
    /// polling rate to poll processes in seconds
    polling_rate: u64,

    /// indicators to show based on which processes are running
    indicators: Vec<ProcessModuleConfigItem>,
}

#[derive(Deserialize)]
struct ProcessModuleConfigItem {
    /// start of cmdline of the process
    cmdline: String,
    /// icon to show in that case
    icon: String,
}

impl Default for ProcessModuleConfig {
    fn default() -> Self {
        Self {
            polling_rate: 600, // every 10 minutes
            indicators: Vec::new(),
        }
    }
}

impl ModuleMessage for ProcessMessage {}
#[derive(Clone, Debug)]
pub enum ProcessMessage {
    Processes(Vec<ProcessInfo>),
    Stop(u64),
    Rescan,
    Ok,
}

pub struct ProcessModule {
    rate: Duration,
    config: Vec<(String, Icon)>,

    /// this is actually the current state
    icons: Vec<(u64, Icon)>,
}

impl ProcessModule {
    pub fn new() -> Result<Self> {
        let config: ProcessModuleConfig = CONFIG.module(PROCESS_MODULE_IDENTIFIER);

        let icons = config
            .indicators
            .into_iter()
            .map(|item| {
                let icon = Icon::from_name(&item.icon)
                    .with_context(|| format!("icon `{}` not recognized", item.icon))?;

                Ok((item.cmdline, icon))
            })
            .collect::<Result<_>>()?;

        Ok(Self {
            config: icons,
            icons: Vec::new(),
            rate: Duration::from_secs(config.polling_rate),
        })
    }
}

impl Module for ProcessModule {
    type Message = ProcessMessage;

    fn subscribe(&self) -> Subscription<Self::Message> {
        from_recipe(ProcessMonitor(self.rate)).map(Self::Message::Processes)
    }

    fn pass_message(&self, message: &str) -> Option<Self::Message> {
        if message.eq("rescan") { Some(Self::Message::Rescan) } else { None }
    }

    fn update(&mut self, message: &Self::Message) -> (Task<Self::Message>, Option<OsdId>) {
        match message {
            ProcessMessage::Processes(infos) => {
                self.icons = self
                    .config
                    .iter()
                    .filter_map(|(cmdline, icon)| {
                        infos
                            .iter()
                            .find(|process| process.cmdline.starts_with(cmdline))
                            .map(|process| (process.pid, *icon))
                    })
                    .collect()
            }
            ProcessMessage::Stop(pid) => {
                if let Err(e) = send_signal(*pid, ProcessSignal::SIGTERM) {
                    error!("failed to stop process `{pid}` on click: {e:#}")
                }

                return (
                    Task::perform(read_running_processes(), |result| {
                        result
                            .map_err(|e| {
                                error!("failed to re-read running processes after kill: {e:#}")
                            })
                            .map(ProcessMessage::Processes)
                            .unwrap_or(ProcessMessage::Ok)
                    }),
                    None,
                );
            }
            ProcessMessage::Rescan => {
                return (
                    Task::perform(read_running_processes(), |result| {
                        result
                            .map_err(|e| {
                                error!("failed to re-read running processes on demand: {e:#}")
                            })
                            .map(ProcessMessage::Processes)
                            .unwrap_or(ProcessMessage::Ok)
                    }),
                    None,
                );
            }
            ProcessMessage::Ok => {}
        }

        (Task::none(), None)
    }

    fn render_info(&self) -> Vec<Element<'_, Self::Message, Theme, Renderer>> {
        self.icons
            .iter()
            .map(|(pid, c)| mouse_area(icon(*c)).on_release(Self::Message::Stop(*pid)).into())
            .collect::<Vec<_>>()
    }
}

struct ProcessMonitor(Duration);

impl Recipe for ProcessMonitor {
    type Output = Vec<ProcessInfo>;

    fn hash(&self, state: &mut iced::advanced::subscription::Hasher) {
        state.write_str("running processes stream");
    }

    fn stream(self: Box<Self>, _input: EventStream) -> BoxStream<Self::Output> {
        debug!("starting running processes stream");
        listen_running_processes(self.0)
    }
}
