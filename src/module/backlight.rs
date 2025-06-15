use std::hash::Hasher;

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use futures::{StreamExt, stream};
use iced::{
    Element, Padding, Renderer, Subscription, Task, Theme,
    advanced::subscription::{EventStream, Recipe, from_recipe},
    alignment::Horizontal,
    widget::column,
};
use iced_winit::futures::BoxStream;
use liischte_lib::sysfs::backlight::BacklightDevice;
use log::{debug, error, info};
use lucide_icons::Icon;
use serde::Deserialize;

use crate::{
    config::CONFIG,
    osd::OsdId,
    ui::{icon, progress::vertical_progress},
};

use super::{Module, ModuleMessage};

pub const BACKLIGHT_MODULE_IDENTIFIER: &str = "backlight";

#[derive(Deserialize, Default)]
#[serde(default)]
struct BacklightModuleConfig {
    /// force the use of a specific backlight (we use the first one otherwise)
    device: Option<String>,
}

impl ModuleMessage for BacklightModulemessage {}
#[derive(Clone, Debug)]
pub enum BacklightModulemessage {
    Brightness(f64),
}

pub struct BacklightModule {
    backlight: BacklightDevice,
    brightness: f64,
}

impl BacklightModule {
    pub async fn new() -> Result<Self> {
        let config: BacklightModuleConfig = CONFIG.module(BACKLIGHT_MODULE_IDENTIFIER);

        info!("reading available backlight devices from sysfs");
        let mut selected = None;

        for device in BacklightDevice::read_all().await.context("failed to read power devices")? {
            debug!("checking backlight device with name `{}`", device.device.name);

            if selected.is_none()
                && (config.device.as_ref() == Some(&device.device.name) || config.device.is_none())
            {
                selected = Some(device);
            }
        }

        if let Some(selected) = selected {
            info!("using backlight {}", selected.device.name);

            Ok(Self { brightness: selected.read_brightness().await?, backlight: selected })
        } else {
            Err(anyhow!("desired backlight device was not found"))
        }
    }
}

#[async_trait]
impl Module for BacklightModule {
    type Message = BacklightModulemessage;

    fn subscribe(&self) -> Subscription<Self::Message> {
        from_recipe(BrightnessMonitor(self.backlight.clone())).map(Self::Message::Brightness)
    }

    fn update(&mut self, message: &Self::Message) -> (Task<Self::Message>, Option<OsdId>) {
        match message {
            BacklightModulemessage::Brightness(b) => self.brightness = *b,
        }

        (Task::none(), Some(0))
    }

    fn render_osd(&self, _id: OsdId) -> Element<'_, Self::Message, Theme, Renderer> {
        let symbol = match () {
            _ if self.brightness > 0.66 => Icon::Sun,
            _ if self.brightness > 0.33 => Icon::SunMedium,
            _ => Icon::SunDim,
        };

        column![
            vertical_progress(self.brightness as f32, 100f32, 4f32, 6f32),
            icon(symbol).size(20)
        ]
        .padding(Padding::ZERO.top(CONFIG.looks.width as f32 / 2f32 - 2f32).bottom(8))
        .spacing(8)
        .align_x(Horizontal::Center)
        .into()
    }
}

struct BrightnessMonitor(BacklightDevice);

impl Recipe for BrightnessMonitor {
    type Output = f64;

    fn hash(&self, state: &mut iced::advanced::subscription::Hasher) {
        state.write_str(&format!("brightness events for {}", self.0.device.name));
    }

    fn stream(self: Box<Self>, _input: EventStream) -> BoxStream<Self::Output> {
        debug!("starting brightness listener for {}", self.0.device.name);

        match self.0.listen_brightness() {
            Ok(s) => s,
            Err(e) => {
                error!("failed to start brightness listening: {e:#}");
                stream::empty().boxed()
            }
        }
    }
}
