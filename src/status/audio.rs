use std::{hash::Hasher as _, sync::Arc};

use iced::{
    Element, Renderer, Subscription, Task, Theme,
    advanced::subscription::{EventStream, Hasher, Recipe, from_recipe},
    mouse::ScrollDelta,
    widget::mouse_area,
};
use iced_winit::futures::BoxStream;
use liischte_lib::{
    StreamContext,
    pipewire::{PipewireInstance, default::DefaultState, node::NodeState},
};
use log::debug;
use lucide_icons::Icon;

use crate::{
    status::{Status, StatusMessage},
    ui::icon,
};

pub const AUDIO_STATUS_IDENTIFIER: &str = "audio";

impl StatusMessage for AudioMessage {}
#[derive(Clone, Debug)]
pub enum AudioMessage {
    DefaultState(DefaultState),
    SinkState(Vec<NodeState>),

    ToggleMute,
    ChangeVolume(f32),

    Ok,
}

pub struct AudioStatus {
    pipewire: Arc<PipewireInstance>, // this is an arc to implement efficient subscriptions

    defaults: DefaultState,
    sinks: Vec<NodeState>,

    selected: Option<NodeState>,
}

impl AudioStatus {
    pub fn new() -> Self {
        Self {
            pipewire: Arc::new(PipewireInstance::start()),
            defaults: DefaultState::default(),
            sinks: Vec::new(),
            selected: None,
        }
    }
}

impl Status for AudioStatus {
    type Message = AudioMessage;

    fn subscribe(&self) -> Subscription<Self::Message> {
        Subscription::batch(vec![
            from_recipe(DefaultMonitor(self.pipewire.clone())).map(AudioMessage::DefaultState),
            from_recipe(SinksMonitor(self.pipewire.clone())).map(AudioMessage::SinkState),
        ])
    }

    fn update(&mut self, message: &Self::Message) -> Task<Self::Message> {
        match (message, &self.selected) {
            (AudioMessage::DefaultState(defaults), _) => self.defaults = defaults.clone(),
            (AudioMessage::SinkState(nodes), _) => self.sinks = nodes.clone(),

            (AudioMessage::ToggleMute, Some(selected)) => {
                self.pipewire.set_mute(&selected.name, !selected.mute).ok();
            }
            (AudioMessage::ChangeVolume(offset), Some(selected)) => {
                self.pipewire
                    .set_volume(
                        &selected.name,
                        &selected.volume.iter().map(|v| v + offset).collect::<Vec<_>>(),
                    )
                    .ok();
            }

            _ => {}
        };

        self.selected = self.sinks.iter().find(|sink| sink.name == self.defaults.sink).cloned();

        Task::none()
    }

    fn render(&self) -> Element<'_, Self::Message, Theme, Renderer> {
        let Some(sink) = self.selected.as_ref() else {
            return icon(Icon::VolumeOff).into();
        };

        let icon = if sink.mute {
            icon(Icon::VolumeX)
        } else {
            let volume = sink.volume.iter().sum::<f32>() / sink.volume.len() as f32;

            match () {
                _ if volume <= 0.33 => icon(Icon::Volume),
                _ if volume <= 0.66 => icon(Icon::Volume1),
                _ => icon(Icon::Volume2),
            }
        };

        mouse_area(icon)
            .on_scroll(|event| {
                if let ScrollDelta::Pixels { y, .. } = event {
                    AudioMessage::ChangeVolume(if y < 0f32 { -0.05 } else { 0.05 })
                } else {
                    AudioMessage::Ok
                }
            })
            .on_release(AudioMessage::ToggleMute)
            .into()
    }
}

struct SinksMonitor(Arc<PipewireInstance>);

impl Recipe for SinksMonitor {
    type Output = Vec<NodeState>;

    fn hash(&self, state: &mut Hasher) {
        state.write_str("audio sink events");
    }

    fn stream(self: Box<Self>, _input: EventStream) -> BoxStream<Self::Output> {
        debug!("staring audio sink listener");

        let stream = self.0.listen_sinks();
        self.0.trigger_update().stream_log("pipewire sink listener"); // we want to get values immediately

        stream
    }
}

struct DefaultMonitor(Arc<PipewireInstance>);

impl Recipe for DefaultMonitor {
    type Output = DefaultState;

    fn hash(&self, state: &mut Hasher) {
        state.write_str("audio default events");
    }

    fn stream(self: Box<Self>, _input: EventStream) -> BoxStream<Self::Output> {
        debug!("staring audio default listener");

        let stream = self.0.listen_defaults();
        self.0.trigger_update().stream_log("pipewire default listener"); // we want to get values immediately

        stream
    }
}
