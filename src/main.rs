#![feature(hasher_prefixfree_extras)]
use std::{
    any::{Any, TypeId},
    collections::HashMap,
};

use clock::{Clock, ClockMessage};
use config::CONFIG;
use futures::StreamExt;
use hyprland::{Hyprland, HyprlandMessage};
use iced::{
    Color, Font, Length, Limits, Subscription, Task, Theme,
    alignment::Horizontal,
    application,
    runtime::platform_specific::wayland::layer_surface::{
        IcedMargin, IcedOutput, SctkLayerSurfaceSettings,
    },
    widget::{Column, column, vertical_space},
    window::Id as SurfaceId,
};
use iced_winit::commands::{
    layer_surface::get_layer_surface,
    subsurface::{Anchor, Layer},
};
use log::{info, trace};
use lucide_icons::lucide_font_bytes;
use status::{AbstractStatus, Status, StatusMessage, power::PowerStatus};
use ui::{separator, window::layer_window};

mod clock;
mod hyprland;
mod status;

pub mod config;
pub mod info;
mod ui;

#[tokio::main]
async fn main() -> iced::Result {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let app = layer_window::<_, Message, _, _, iced::executor::Default>(
        Liischte::update,
        Liischte::view,
        Liischte::subscription,
    )
    .style(|_, _| application::Appearance {
        background_color: Color::TRANSPARENT,
        text_color: CONFIG.looks.foreground,
        icon_color: CONFIG.looks.foreground,
    })
    .settings(iced::Settings {
        default_font: Font::with_name(&CONFIG.looks.font),
        default_text_size: 16.into(),
        antialiasing: true,
        fonts: vec![lucide_font_bytes().into()],
        ..Default::default()
    });

    let mut liischte = Liischte::new();

    // add statusses
    liischte.add_status(Box::new(PowerStatus::new()));

    liischte.initialize().await;

    // run iced app with surface
    app.run_with(move || {
        let surface = get_layer_surface(SctkLayerSurfaceSettings {
            layer: Layer::Top,
            anchor: Anchor::TOP
                | if CONFIG.right { Anchor::RIGHT } else { Anchor::LEFT }
                | Anchor::BOTTOM,
            output: IcedOutput::Active,

            margin: IcedMargin {
                bottom: CONFIG.looks.padding as i32,
                left: CONFIG.looks.padding as i32,
                top: CONFIG.looks.padding as i32,
                right: 0,
            },
            size: Some((Some(CONFIG.looks.width), None)),
            exclusive_zone: CONFIG.looks.width as i32,
            size_limits: Limits::NONE,

            pointer_interactivity: true,
            namespace: CONFIG.namespace.clone(),

            ..Default::default()
        });

        (liischte, surface)
    })
}

#[derive(Debug, Clone)]
enum Message {
    Clock(ClockMessage),
    Hyprland(HyprlandMessage),
    Status(Box<dyn StatusMessage>),
}

struct Liischte {
    clock: Clock,
    hyprland: Hyprland,
    status: HashMap<TypeId, Box<dyn AbstractStatus>>,
}

impl Liischte {
    pub fn new() -> Self {
        Self { status: HashMap::new(), clock: Clock::new(), hyprland: Hyprland::new() }
    }

    pub async fn initialize(&mut self) {
        info!("initializing all widgets");

        self.hyprland.initialize().await;

        for status in self.status.values_mut() {
            status.initialize().await
        }
    }

    pub fn add_status(&mut self, status: Box<dyn AbstractStatus>) {
        self.status.insert(status.message_type(), status);
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Clock(msg) => self.clock.update(msg),
            Message::Hyprland(msg) => return self.hyprland.update(msg).map(Message::Hyprland),
            Message::Status(msg) => {
                trace!(
                    "passing status message {}",
                    (*msg).type_name().rsplit("::").next().unwrap_or_default()
                );

                self.status.get_mut(&(*msg).type_id()).expect("wth").update(msg)
            }
        }

        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![
            self.clock.subscribe().map(Message::Clock),
            self.hyprland.subscribe().map(Message::Hyprland),
            // messages for status
            Subscription::batch(
                self.status.values().map(|status| status.subscribe().map(Message::Status)),
            ),
        ])
    }

    fn view(&self, _: SurfaceId) -> iced::Element<'_, Message, Theme, iced::Renderer> {
        let status = Column::from_iter(
            self.status.values().map(|status| status.render().map(Message::Status)),
        )
        .spacing(4);

        column![
            self.hyprland.render().map(Message::Hyprland),
            vertical_space(),
            status,
            separator(),
            self.clock.render()
        ]
        .spacing(12)
        .align_x(Horizontal::Center)
        .width(Length::Fill)
        .into()
    }
}
