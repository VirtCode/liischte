use iced::Subscription;
use iced::runtime::platform_specific::wayland::layer_surface::IcedOutput;

use iced::{
    Event as IcedEvent,
    event::{
        PlatformSpecific as PlatformEvent, listen_with,
        wayland::{Event as WaylandEvent, OutputEvent},
    },
};
use log::debug;
use wayland_client::protocol::wl_output::WlOutput;

use crate::config::CONFIG;

#[derive(Clone, Debug)]
pub enum OutputMessage {
    Change(Output),
    Removed(Output),
}

pub struct OutputHandler {
    outputs: Vec<Output>,
}

impl OutputHandler {
    pub fn new() -> Self {
        Self { outputs: Vec::new() }
    }

    pub fn subscribe(&self) -> Subscription<OutputMessage> {
        // thanks @wash2 for showing me this!
        listen_with(|e, _, _| match e {
            IcedEvent::PlatformSpecific(PlatformEvent::Wayland(WaylandEvent::Output(
                OutputEvent::Created(Some(info)),
                wloutput,
            ))) => Some(OutputMessage::Change(Output::new(
                wloutput,
                info.name.unwrap_or_default(),
                info.description.unwrap_or_default(),
            ))),
            IcedEvent::PlatformSpecific(PlatformEvent::Wayland(WaylandEvent::Output(
                OutputEvent::InfoUpdate(info),
                wloutput,
            ))) => Some(OutputMessage::Change(Output::new(
                wloutput,
                info.name.unwrap_or_default(),
                info.description.unwrap_or_default(),
            ))),
            IcedEvent::PlatformSpecific(PlatformEvent::Wayland(WaylandEvent::Output(
                OutputEvent::Removed,
                wloutput,
            ))) => Some(OutputMessage::Removed(Output::empty(wloutput))),

            _ => None,
        })
    }

    pub fn update(&mut self, message: OutputMessage) {
        match message {
            OutputMessage::Change(output) => {
                debug!("mapping output {} ({})", output.name, output.description);
                self.outputs.retain(|o| *o != output);
                self.outputs.push(output);
            }
            OutputMessage::Removed(output) => {
                self.outputs.retain(|o| *o != output);
            }
        }
    }

    pub fn get_configured(&self) -> Option<IcedOutput> {
        let setting = CONFIG.output.to_lowercase();

        if setting == "active" {
            Some(IcedOutput::Active)
        } else if let Some(desc) = setting.strip_prefix("desc:") {
            self.outputs
                .iter()
                .find(|out| out.description.to_lowercase().starts_with(desc.trim()))
                .map(|out| IcedOutput::Output(out.wl.clone()))
        } else {
            self.outputs
                .iter()
                .find(|out| out.name.to_lowercase() == setting)
                .map(|out| IcedOutput::Output(out.wl.clone()))
        }
    }
}

#[derive(Clone, Debug)]
pub struct Output {
    pub name: String,
    pub description: String,
    pub wl: WlOutput,
}

impl Output {
    pub fn new(wl: WlOutput, name: String, description: String) -> Self {
        Self { wl, name, description }
    }

    pub fn empty(wl: WlOutput) -> Self {
        Self { wl, name: String::new(), description: String::new() }
    }
}

impl PartialEq for Output {
    fn eq(&self, other: &Self) -> bool {
        self.wl == other.wl
    }
}
