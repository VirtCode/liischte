use std::hash::Hasher as _;

use anyhow::{Context, Result};
use futures::{StreamExt, stream};
use iced::Task;
use iced::mouse::ScrollDelta;
use iced::widget::{Column, container, mouse_area};
use iced::{
    Background, Border, Color, Radius, Subscription, Theme,
    advanced::subscription::{EventStream, Hasher, Recipe, from_recipe},
    widget::{Space, container::Style},
};
use iced_winit::futures::BoxStream;
use liischte_lib::StreamContext;
use liischte_lib::hyprland::{HyprlandInstance, WorkspaceState};
use log::debug;

use crate::config::{CONFIG, ConfigHyprland};

#[derive(Debug, Clone)]
pub enum HyprlandMessage {
    State(i64, Vec<WorkspaceState>),
    SelectAbsolute(i64),
    SelectRelative(i64),
    Ok,
}

pub struct Hyprland {
    config: &'static ConfigHyprland,
    instance: HyprlandInstance,

    selected: i64,
    workspaces: Vec<WorkspaceState>,
}

impl Hyprland {
    pub async fn new() -> Result<Self> {
        let config = &CONFIG.hyprland;

        let instance = HyprlandInstance::env().context(
            "failed read environment for hyprland instance signature, are you running inside it?",
        )?;

        let selected = instance.get_active_workspace().await?.id;

        let mut workspaces = instance.get_all_workspaces().await?;
        workspaces.retain(|state| state.monitor_id == Some(config.monitor) && state.id >= 0);

        Ok(Self { config, instance, selected, workspaces })
    }

    pub fn subscribe(&self) -> Subscription<HyprlandMessage> {
        from_recipe(WorkspaceMonitor(self.instance.clone(), self.config.monitor))
            .map(|(selected, state)| HyprlandMessage::State(selected, state))
    }

    pub fn update(&mut self, message: HyprlandMessage) -> Task<HyprlandMessage> {
        match message {
            HyprlandMessage::State(selected, workspaces) => {
                self.selected = selected;
                self.workspaces = workspaces;
            }
            HyprlandMessage::SelectAbsolute(id) => {
                let instance = self.instance.clone();

                return Task::future(async move {
                    let _ = instance.run_select_workspace(id).await;
                    HyprlandMessage::Ok
                });
            }
            HyprlandMessage::SelectRelative(offset) => {
                let instance = self.instance.clone();

                return Task::future(async move {
                    let _ = instance.run_select_workspace_relative(offset).await;
                    HyprlandMessage::Ok
                });
            }
            HyprlandMessage::Ok => {}
        }

        Task::none()
    }

    /// renders a single workspace indicator
    fn render_indicator(
        &self,
        state: &WorkspaceState,
    ) -> iced::Element<'_, HyprlandMessage, Theme, iced::Renderer> {
        let (background, border) = match (state.id == self.selected, state.window_amount > 0) {
            (true, _) => (CONFIG.looks.semi, self.config.border),
            (false, true) => (CONFIG.looks.foreground, 0f32),
            _ => (Color::TRANSPARENT, self.config.border),
        };

        let radius = if state.fullscreen && self.config.fullscreen {
            3f32 // this is almost no rounding, just for asthetics
        } else {
            self.config.rounding
        };

        mouse_area(container(Space::new(self.config.size, self.config.size)).style(move |_| {
            Style {
                background: Some(Background::Color(background)),
                border: Border {
                    color: CONFIG.looks.foreground,
                    width: border,
                    radius: Radius::new(radius),
                },
                ..Default::default()
            }
        }))
        .on_release(HyprlandMessage::SelectAbsolute(state.id))
        .into()
    }

    pub fn render(&self) -> iced::Element<'_, HyprlandMessage, Theme, iced::Renderer> {
        mouse_area(
            Column::from_vec(
                self.workspaces.iter().map(|state| self.render_indicator(state)).collect(),
            )
            .spacing(8),
        )
        .on_scroll(|event| {
            if let ScrollDelta::Pixels { y, .. } = event {
                HyprlandMessage::SelectRelative(if y > 0f32 { -1 } else { 1 })
            } else {
                HyprlandMessage::Ok
            }
        })
        .into()
    }
}

struct WorkspaceMonitor(HyprlandInstance, u64);

impl Recipe for WorkspaceMonitor {
    type Output = (i64, Vec<WorkspaceState>);

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
