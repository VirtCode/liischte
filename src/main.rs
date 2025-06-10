#![feature(hasher_prefixfree_extras)]
use std::{any::TypeId, collections::HashMap, time::Duration};

use clock::{Clock, ClockMessage};
use config::CONFIG;
use hyprland::{Hyprland, HyprlandMessage};
use iced::{
    Background, Border, Color, Font, Length, Limits, Radius, Subscription, Task, Theme,
    alignment::{Horizontal, Vertical},
    application, color,
    runtime::platform_specific::wayland::layer_surface::{
        IcedMargin, IcedOutput, SctkLayerSurfaceSettings,
    },
    task::Handle,
    widget::{Column, Space, column, container::Style, text, vertical_space},
    window::Id as SurfaceId,
};
use iced_winit::commands::{
    layer_surface::{destroy_layer_surface, get_layer_surface},
    subsurface::{Anchor, Layer},
};
use log::{error, info};
use lucide_icons::lucide_font_bytes;
use status::{
    AbstractStatus, StatusMessage,
    audio::{AUDIO_STATUS_IDENTIFIER, AudioStatus},
    power::{POWER_STATUS_IDENTIFIER, PowerStatus},
};
use tokio::time::sleep;
use ui::{empty, separator, window::layer_window};

use iced::widget::container as create_container;

use crate::{
    osd::{OsdHandler, OsdMessage},
    status::network::{NETWORK_STATUS_IDENTIFIER, NetworkStatus},
    ui::PILL_RADIUS,
};

mod clock;
mod hyprland;
mod status;

pub mod config;
mod osd;
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

    if CONFIG.hyprland.enabled {
        liischte.set_hyprland(Hyprland::new().await.unwrap());
    }

    for status in &CONFIG.statuses {
        liischte.add_status(match status.as_str() {
            POWER_STATUS_IDENTIFIER => Box::new(PowerStatus::new().await.unwrap()),
            NETWORK_STATUS_IDENTIFIER => Box::new(NetworkStatus::new().await.unwrap()),
            AUDIO_STATUS_IDENTIFIER => Box::new(AudioStatus::new()),
            status => panic!("status `{status}` does not exist in this version"),
        });
    }

    // run iced app with surface
    app.run_with(move || {
        let task = liischte.init();
        (liischte, task)
    })
}

#[derive(Debug, Clone)]
enum Message {
    Clock(ClockMessage),
    Hyprland(HyprlandMessage),
    Status(Box<dyn StatusMessage>),

    Osd(OsdMessage),
}

struct Liischte {
    clock: Clock,
    hyprland: Option<Hyprland>,
    status: HashMap<TypeId, Box<dyn AbstractStatus>>,

    osd: OsdHandler,

    pub surface_bar: SurfaceId,
}

impl Liischte {
    pub fn new() -> Self {
        Self {
            status: HashMap::new(),
            clock: Clock::new(),
            hyprland: None,

            osd: OsdHandler::new(),

            surface_bar: SurfaceId::unique(),
        }
    }

    /// set the hyprland instance
    pub fn set_hyprland(&mut self, hyprland: Hyprland) {
        self.hyprland = Some(hyprland);
    }

    /// add a status to the bar
    pub fn add_status(&mut self, status: Box<dyn AbstractStatus>) {
        self.status.insert(status.message_type(), status);
    }

    fn init(&mut self) -> Task<Message> {
        get_layer_surface(SctkLayerSurfaceSettings {
            id: self.surface_bar,

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
        })
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Clock(msg) => self.clock.update(msg).map(Message::Clock),

            Message::Hyprland(msg) => self
                .hyprland
                .as_mut()
                .map(|hl| hl.update(msg).map(Message::Hyprland))
                .unwrap_or(Task::none()),

            Message::Status(msg) => {
                let id = (*msg).type_id();

                let (task, want_osd) = self
                    .status
                    .get_mut(&id)
                    .expect("received status message for non-existent status")
                    .update(msg);

                if want_osd {
                    Task::batch(vec![
                        task.map(Message::Status),
                        self.osd.request_osd(id).map(Message::Osd),
                    ])
                } else {
                    task.map(Message::Status)
                }
            }

            Message::Osd(msg) => self.osd.update(msg).map(Message::Osd),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![
            self.clock.subscribe().map(Message::Clock),
            self.hyprland
                .as_ref()
                .map(|hl| hl.subscribe().map(Message::Hyprland))
                .unwrap_or(Subscription::none()),
            Subscription::batch(
                self.status.values().map(|status| status.subscribe().map(Message::Status)),
            ),
        ])
    }

    fn view(&self, id: SurfaceId) -> iced::Element<'_, Message, Theme, iced::Renderer> {
        if id == self.surface_bar {
            self.view_bar()
        } else if id == self.osd.surface_osd {
            self.view_osd()
        } else {
            error!("tried to view unknown surface with id `{id}`");
            column![].into()
        }
    }

    fn view_bar(&self) -> iced::Element<'_, Message, Theme, iced::Renderer> {
        column![
            self.hyprland
                .as_ref()
                .map(|hl| hl.render().map(Message::Hyprland))
                .unwrap_or_else(|| column![].into()),
            vertical_space(),
            Column::from_iter(
                self.status.values().map(|status| status.render().map(Message::Status)),
            )
            .spacing(4),
            separator(),
            self.clock.render().map(Message::Clock)
        ]
        .spacing(12)
        .align_x(Horizontal::Center)
        .width(Length::Fill)
        .into()
    }

    fn view_osd(&self) -> iced::Element<'_, Message, Theme, iced::Renderer> {
        let widget: iced::Element<'_, Message, Theme, iced::Renderer> =
            if let Some(ref id) = self.osd.get_active() {
                self.status
                    .get(id)
                    .expect("tried to show osd from non-existent status")
                    .render_osd()
                    .map(Message::Status)
            } else {
                empty().into()
            };

        create_container(
            create_container(widget)
                .style(move |_| Style {
                    background: Some(Background::Color(CONFIG.looks.background)),
                    border: Border { color: CONFIG.looks.border, width: 1f32, radius: PILL_RADIUS },
                    ..Default::default()
                })
                .width(CONFIG.looks.width as f32)
                .align_x(Horizontal::Center)
                .align_y(Vertical::Center),
        )
        .height(Length::Fill)
        .width(Length::Fill)
        .align_x(Horizontal::Center)
        .align_y(Vertical::Center)
        .into()
    }
}
