use std::time::Duration;

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
    widget::{column, text, vertical_space},
    window::Id as SurfaceId,
};
use iced_winit::commands::{
    layer_surface::get_layer_surface,
    subsurface::{Anchor, Layer},
};
use ui::{icon, separator, window::layer_window};

mod ui;

pub const FONT: &str = "JetBrains Mono";
pub const ICON_FONT: &str = "Material Symbols Outlined";

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

        ..Default::default()
    });

    app.run_with(Liischte::new)
}

#[derive(Debug, Clone)]
enum Message {
    Clock(DateTime<Local>),
}

struct Liischte {
    time: DateTime<Local>,
}

impl Liischte {
    pub fn new() -> (Self, Task<Message>) {
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

        (Self { time: Local::now() }, surface)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Clock(time) => self.time = time,
        }

        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        time::every(Duration::from_secs(1)).map(|_| Message::Clock(Local::now()))
    }

    fn view(&self, _: SurfaceId) -> iced::Element<'_, Message, Theme, iced::Renderer> {
        column![
            column![],
            vertical_space(),
            column![icon(''), icon(''), icon('')].spacing(4),
            separator(),
            column![
                text!("{:0>2}", self.time.hour()),
                text!("{:0>2}", self.time.minute()),
                text!("{:0>2}", self.time.second())
            ]
        ]
        .spacing(12)
        .align_x(Horizontal::Center)
        .width(Length::Fill)
        .into()
    }
}
