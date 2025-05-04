#![feature(hasher_prefixfree_extras)]

use std::{
    any::{Any, TypeId},
    collections::HashMap,
    time::Duration,
};

use chrono::{DateTime, Local, Timelike};
use futures::StreamExt;
use iced::{
    Color, Font, Length, Limits, Subscription, Task, Theme,
    alignment::Horizontal,
    application, color,
    runtime::platform_specific::wayland::layer_surface::{
        IcedMargin, IcedOutput, SctkLayerSurfaceSettings,
    },
    time,
    widget::{Column, column, text, vertical_space},
    window::Id as SurfaceId,
};
use iced_winit::commands::{
    layer_surface::get_layer_surface,
    subsurface::{Anchor, Layer},
};
use log::{debug, info, trace};
use lucide_icons::lucide_font_bytes;
use status::{AbstractStatus, Status, StatusMessage, power::PowerStatus};
use ui::{separator, window::layer_window};

mod status;
pub mod system;
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
        text_color: color!(0xcdd5ff),
        icon_color: color!(0xcdd5ff),
    })
    .settings(iced::Settings {
        default_font: Font::with_name("JetBrains Mono"),
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
        let padding = 10;
        let width = 32;

        let surface = get_layer_surface(SctkLayerSurfaceSettings {
            layer: Layer::Top,
            anchor: Anchor::TOP | Anchor::LEFT | Anchor::BOTTOM,
            output: IcedOutput::Active,

            margin: IcedMargin { bottom: padding, left: padding, top: padding, right: 0 },
            size: Some((Some(width), None)),
            exclusive_zone: width as i32,
            size_limits: Limits::NONE,

            pointer_interactivity: true,
            namespace: String::from("liischte"),

            ..Default::default()
        });

        (liischte, surface)
    })
}

#[derive(Debug, Clone)]
enum Message {
    Clock(DateTime<Local>),
    Status(Box<dyn StatusMessage>),
}

struct Liischte {
    time: DateTime<Local>,
    status: HashMap<TypeId, Box<dyn AbstractStatus>>,
}

impl Liischte {
    pub fn new() -> Self {
        Self { status: HashMap::new(), time: Local::now() }
    }

    pub async fn initialize(&mut self) {
        info!("initializing all widgets");

        for status in self.status.values_mut() {
            status.initialize().await
        }
    }

    pub fn add_status(&mut self, status: Box<dyn AbstractStatus>) {
        self.status.insert(status.message_type(), status);
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Clock(time) => self.time = time,
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
            time::every(Duration::from_secs(1)).map(|_| Message::Clock(Local::now())),
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

        let clock = column![
            text!("{:0>2}", self.time.hour()),
            text!("{:0>2}", self.time.minute()),
            text!("{:0>2}", self.time.second())
        ];

        column![vertical_space(), status, separator(), clock]
            .spacing(12)
            .align_x(Horizontal::Center)
            .width(Length::Fill)
            .into()
    }
}
