use std::{future::ready, time::Duration};

use chrono::{DateTime, Local, Timelike};
use futures::StreamExt;
use iced::{
    Color, Font, Length, Limits, Subscription, Task, Theme,
    alignment::Horizontal,
    application, color,
    font::Weight,
    runtime::platform_specific::wayland::layer_surface::{
        IcedMargin, IcedOutput, SctkLayerSurfaceSettings,
    },
    time,
    widget::{Column, button, column, text, text_input},
    window::Id as SurfaceId,
};
use iced_winit::commands::{
    layer_surface::get_layer_surface,
    subsurface::{Anchor, KeyboardInteractivity, Layer},
};
use layer_shell::layer_window;
use log::{error, info};
use tokio_udev::{AsyncMonitorSocket, MonitorBuilder};

mod layer_shell;
mod power;

pub const FONT: &str = "JetBrains Mono";
pub const ICON_FONT: &str = "Material Symbols Outlined";

#[tokio::main]
async fn main() -> iced::Result {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let settings = iced::Settings {
        default_font: Font::with_name("JetBrains Mono"),
        default_text_size: 16.into(),
        antialiasing: true,

        id: Some(String::from("liischte")),

        ..Default::default()
    };

    let app = layer_window::<_, Message, _, _, iced::executor::Default>(
        "En Liischte",
        Sidebar::update,
        Sidebar::view,
    )
    .subscription(Sidebar::subscription)
    .style(|_sidebar, _theme| application::Appearance {
        background_color: Color::TRANSPARENT,
        text_color: color!(0xcdd5ff),
        icon_color: color!(0xcdd5ff),
    })
    .settings(settings);

    app.run_with(Sidebar::new)
}

#[derive(Debug, Clone)]
enum Message {
    Clock(DateTime<Local>),
}

struct Sidebar {
    time: DateTime<Local>,
}

impl Sidebar {
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
        column![column![].height(Length::Fill), column![
            text!("{:0>2}", self.time.hour()),
            text!("{:0>2}", self.time.minute()),
            text!("{:0>2}", self.time.second())
        ]]
        .align_x(Horizontal::Center)
        .width(Length::Fill)
        .into()
    }
}
