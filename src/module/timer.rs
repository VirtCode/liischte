use std::time::{Duration, Instant};

use iced::{
    Background, Border, Element, Renderer, Subscription, Task, Theme,
    alignment::Horizontal,
    widget::{column, progress_bar},
};
use liischte_lib::StreamContext;
use log::{info, warn};
use lucide_icons::Icon;
use notify_rust::Notification;
use serde::Deserialize;
use tokio::time::sleep;

use crate::{
    config::CONFIG,
    module::{Module, ModuleMessage},
    osd::OsdId,
    ui::{PILL_RADIUS, icon_char},
};

pub const TIMER_MODULE_IDENTIFIER: &str = "timer";

#[derive(Deserialize)]
#[serde(default)]
struct TimerModuleConfig {
    /// default icon to show if none is set
    // TODO: deserialize directly as icon
    default_icon: String,

    /// heading to show in the notification
    heading: String,
    /// set notification to never expire
    persistent: bool,
}

impl Default for TimerModuleConfig {
    fn default() -> Self {
        Self {
            default_icon: Icon::AlarmClock.to_string(),

            heading: "Timer Expired!".to_string(),
            persistent: true,
        }
    }
}

impl ModuleMessage for TimerMessage {}
#[derive(Clone, Debug)]
pub enum TimerMessage {
    Create(char, String, Duration),
    Stop,
    Ok,
}

pub struct TimerModule {
    config: TimerModuleConfig,

    timers: Vec<Timer>,
}

pub struct Timer {
    icon: char,
    message: String,

    start: Instant,
    duration: Duration,
}

impl TimerModule {
    pub fn new() -> Self {
        let config: TimerModuleConfig = CONFIG.module(TIMER_MODULE_IDENTIFIER);

        Self { config, timers: vec![] }
    }
}

impl Module for TimerModule {
    type Message = TimerMessage;

    fn subscribe(&self) -> Subscription<Self::Message> {
        Subscription::none()
    }

    fn pass_message(&self, message: &str) -> Option<Self::Message> {
        let mut desc = None;
        let mut icon = None;
        let mut duration = None;

        for (key, value) in message
            .split('|')
            .filter_map(|s| s.split_once('='))
            .map(|(key, value)| (key.trim(), value.trim()))
        {
            match key {
                "icon" => {
                    let Some(icon_val) = Icon::from_name(value) else {
                        info!("passed invalid icon {value} to timer");
                        continue;
                    };

                    icon = Some(icon_val);
                }
                "duration" => {
                    let Ok(int) = value.parse::<u64>() else {
                        info!("passed invalid integer {value} as duration to timer");
                        continue;
                    };

                    duration = Some(Duration::from_secs(int))
                }
                "message" => desc = Some(value.to_string()),
                _ => {}
            }
        }

        let Some(duration) = duration else {
            warn!("now adding timer because no duration was given");
            return None;
        };

        Some(TimerMessage::Create(
            icon.unwrap_or(Icon::from_name(&self.config.default_icon).unwrap_or(Icon::Clock))
                .unicode(),
            desc.unwrap_or(format!("{} seconds have elapsed", duration.as_secs())),
            duration,
        ))
    }

    fn update(&mut self, message: &Self::Message) -> (Task<Self::Message>, Option<OsdId>) {
        match message {
            TimerMessage::Create(icon, desc, duration) => {
                self.timers.push(Timer {
                    message: desc.clone(),
                    icon: *icon,
                    duration: *duration,
                    start: Instant::now(),
                });

                let duration = *duration;
                (
                    Task::future(async move {
                        sleep(duration + Duration::from_millis(100) /* a bit of leeway */).await;
                        TimerMessage::Stop
                    }),
                    None,
                )
            }
            TimerMessage::Stop => {
                let now = Instant::now();

                (
                    Task::batch(
                        self.timers.extract_if(.., |timer| timer.start + timer.duration < now).map(
                            |timer| {
                                let heading = self.config.heading.clone();
                                let persistent = self.config.persistent;

                                Task::future(async move {
                                    let mut builder = Notification::new();

                                    builder.summary(&heading);
                                    builder.body(&timer.message);
                                    if persistent {
                                        builder.timeout(0);
                                    }

                                    builder
                                        .show_async()
                                        .await
                                        .stream_log("failed to send notification");

                                    TimerMessage::Ok // we need this, with .discard() we have lifetime issues
                                })
                            },
                        ),
                    ),
                    None,
                )
            }
            TimerMessage::Ok => (Task::none(), None),
        }
    }

    fn render_info(&self) -> Vec<Element<'_, Self::Message, Theme, Renderer>> {
        self.timers
            .iter()
            .map(|timer| {
                column![
                    icon_char(timer.icon),
                    progress_bar(
                        0.0..=1.0,
                        1.0 - (Instant::now() - timer.start).as_secs_f32()
                            / timer.duration.as_secs_f32()
                    )
                    .style(|_| progress_bar::Style {
                        background: Background::Color(
                            CONFIG.looks.foreground.scale_alpha(CONFIG.looks.tone_opacity),
                        ),
                        border: Border::default().width(0).rounded(PILL_RADIUS),
                        bar: Background::Color(CONFIG.looks.foreground),
                    })
                    .height(2.0)
                    .width(24)
                ]
                .align_x(Horizontal::Center)
                .spacing(-4.0)
                .into()
            })
            .collect::<Vec<_>>()
    }
}
