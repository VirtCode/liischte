use std::hash::Hasher as _;

use futures::{StreamExt, stream};
use iced::widget::{Column, container};
use iced::{
    Background, Border, Color, Radius, Subscription, Theme,
    advanced::subscription::{EventStream, Hasher, Recipe, from_recipe},
    widget::{Space, container::Style},
};
use iced::color;
use iced_winit::futures::BoxStream;
use log::debug;

use crate::{
    Message,
    info::{
        hyprland::{HyprlandInstance, WorkspaceState},
        util::StreamErrorLog,
    },
};

pub type HyprlandMessage = (i64, Vec<WorkspaceState>);

pub struct Hyprland {
    instance: HyprlandInstance,

    selected: i64,
    workspaces: Vec<WorkspaceState>,
}

impl Hyprland {
    pub fn new() -> Self {
        Self { instance: HyprlandInstance::env().unwrap(), selected: 0, workspaces: vec![] }
    }

    pub async fn initialize(&mut self) {
        let monitor = 0u64;

        self.selected = self.instance.get_active_workspace().await.unwrap().id;
        self.workspaces = self.instance.get_all_workspaces().await.unwrap();

        self.workspaces.retain(|state| state.monitor_id == monitor && state.id >= 0);
    }

    pub fn subscribe(&self) -> Subscription<HyprlandMessage> {
        let monitor = 0u64;

        from_recipe(WorkspaceMonitor(self.instance.clone(), monitor))
    }

    pub fn update(&mut self, message: HyprlandMessage) {
        self.selected = message.0;
        self.workspaces = message.1;
    }

    pub fn render(&self) -> iced::Element<'_, Message, Theme, iced::Renderer> {
        Column::from_vec(
            self.workspaces
                .iter()
                .map(|state| {
                    let background = match (state.id == self.selected, state.window_amount > 0) {
                        (true, _) => color!(0x6E6F89),
                        (false, true) => color!(0xCDD5FF),
                        _ => Color::TRANSPARENT,
                    };

                    let radius = if state.fullscreen { 6 } else { 6 };

                    container(Space::new(17, 17))
                        .style(move |_| Style {
                            background: Some(Background::Color(background)),
                            border: Border {
                                color: color!(0xCDD5FF),
                                width: 1.5f32,
                                radius: Radius::new(radius),
                            },
                            ..Default::default()
                        })
                        .into()
                })
                .collect(),
        )
        .spacing(8)
        .into()
    }
}

struct WorkspaceMonitor(HyprlandInstance, u64);

impl Recipe for WorkspaceMonitor {
    type Output = HyprlandMessage;

    fn hash(&self, state: &mut Hasher) {
        state.write_str("hyprland workspace events");
    }

    fn stream(self: Box<Self>, _input: EventStream) -> BoxStream<Self::Output> {
        debug!("staring hyprland workspace listener");

        stream::once(self.0.listen_workspaces(self.1))
            .filter_map(async |res| res.stream_log("hyprland workspace stream"))
            .flatten()
            .boxed()
    }
}
