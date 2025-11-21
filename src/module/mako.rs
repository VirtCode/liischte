use std::hash::Hasher as _;

use anyhow::{Context, Result};
use futures::{
    StreamExt,
    stream::{self},
};
use iced::{
    Element, Renderer, Subscription, Task, Theme,
    advanced::subscription::{EventStream, Hasher, Recipe, from_recipe},
    widget::mouse_area,
};
use iced_winit::futures::BoxStream;
use liischte_lib::{StreamContext, mako::Mako};
use log::debug;
use lucide_icons::Icon;
use serde::Deserialize;

use crate::{
    config::{CONFIG, deserialize_icon},
    module::{Module, ModuleMessage},
    osd::OsdId,
    ui::icon,
};

pub const MAKO_MODULE_IDENTIFIER: &str = "mako";

#[derive(Deserialize)]
#[serde(default)]
struct MakoModuleConfig {
    /// modes to show an indicator for
    modes: Vec<MakoModuleConfigMode>,
}

#[derive(Deserialize)]
struct MakoModuleConfigMode {
    /// name of the mode in mako
    name: String,
    /// icon to show in that case
    #[serde(deserialize_with = "deserialize_icon")]
    icon: Icon,
}

impl Default for MakoModuleConfig {
    fn default() -> Self {
        Self {
            modes: vec![MakoModuleConfigMode {
                name: "do-not-disturb".into(),
                icon: Icon::CircleMinus,
            }],
        }
    }
}

impl ModuleMessage for MakoMessage {}
#[derive(Clone, Debug)]
pub enum MakoMessage {
    /// new modes just dropped
    Modes(Vec<String>),
    /// disable a given mode
    Disable(String),
}

pub struct MakoModule {
    config: MakoModuleConfig,

    mako: Mako,
    modes: Vec<String>,
}

impl MakoModule {
    pub async fn new() -> Result<Self> {
        Ok(Self {
            config: CONFIG.module(MAKO_MODULE_IDENTIFIER),
            mako: Mako::connnect().await.context("failed to connect to mako")?,
            modes: vec![],
        })
    }
}

impl Module for MakoModule {
    type Message = MakoMessage;

    fn subscribe(&self) -> Subscription<Self::Message> {
        from_recipe(ModesMonitor(self.mako.clone())).map(Self::Message::Modes)
    }

    fn update(&mut self, message: &Self::Message) -> (Task<Self::Message>, Option<OsdId>) {
        match message {
            MakoMessage::Modes(items) => {
                self.modes = items.clone();
                (Task::none(), None)
            }
            MakoMessage::Disable(mode) => {
                let modes =
                    self.modes.iter().filter(|active| *active != mode).cloned().collect::<Vec<_>>();
                let mako = self.mako.clone();

                (
                    Task::future(async move {
                        mako.set_modes(&modes).await.stream_log("failed to change modes for mako")
                    })
                    .discard(),
                    None,
                )
            }
        }
    }

    fn render_info(&self) -> Vec<Element<'_, Self::Message, Theme, Renderer>> {
        self.modes
            .iter()
            .filter_map(|mode| self.config.modes.iter().find(|def| def.name == *mode))
            .map(|indicator| {
                mouse_area(icon(indicator.icon))
                    .on_release(Self::Message::Disable(indicator.name.clone()))
                    .into()
            })
            .collect::<Vec<_>>()
    }
}

struct ModesMonitor(Mako);

impl Recipe for ModesMonitor {
    type Output = Vec<String>;

    fn hash(&self, state: &mut Hasher) {
        state.write_str("mako modes");
    }

    fn stream(self: Box<Self>, _input: EventStream) -> BoxStream<Self::Output> {
        debug!("starting mako mode listener");

        stream::once(async move { self.0.listen_modes().await }).flatten().boxed()
    }
}
