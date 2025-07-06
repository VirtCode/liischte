#![feature(hasher_prefixfree_extras)]
use std::collections::HashMap;

use anyhow::{Context, Result};
use clock::{Clock, ClockMessage};
use config::CONFIG;
use futures::StreamExt;
use hyprland::{Hyprland, HyprlandMessage};
use iced::{
    Background, Border, Color, Font, Length, Limits, Padding, Subscription, Task, Theme,
    alignment::{Horizontal, Vertical},
    application,
    runtime::platform_specific::wayland::layer_surface::{
        IcedMargin, IcedOutput, SctkLayerSurfaceSettings,
    },
    widget::{Column, column, container::Style, vertical_space},
    window::Id as SurfaceId,
};
use iced_winit::commands::{
    layer_surface::get_layer_surface,
    subsurface::{Anchor, Layer},
};
use indexmap::IndexMap;
use log::{error, info};
use lucide_icons::lucide_font_bytes;
use module::{
    AbstractModule, ModuleMessage,
    audio::{AUDIO_MODULE_IDENTIFIER, AudioModule},
    backlight::{BACKLIGHT_MODULE_IDENTIFIER, BacklightModule},
    network::{NETWORK_MODULE_IDENTIFIER, NewtorkModule},
    power::{POWER_MODULE_IDENTIFIER, PowerModule},
    process::{PROCESS_MODULE_IDENTIFIER, ProcessModule},
    timer::{TIMER_MODULE_IDENTIFIER, TimerModule},
};
use ui::{empty, separator, window::layer_window};

use iced::widget::container as create_container;

use crate::{
    cli::{Command, read_command},
    ipc::{IpcMessage, IpcServer},
    ui::{
        outputs::{OutputHandler, OutputMessage},
        runtime::ExistingRuntime,
    },
};
use crate::{
    module::ModuleId,
    osd::{OsdHandler, OsdMessage},
    ui::PILL_RADIUS,
};

mod clock;
mod hyprland;
mod module;

mod cli;
pub mod config;
mod ipc;
mod osd;
mod ui;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    // read command from the cli
    match read_command() {
        Some(Command::Pass { module, message }) => {
            ipc::send(IpcMessage::ModuleUpdate(module, message)).await?;
            return Ok(());
        }
        None => {}
    }

    let app = layer_window::<_, Message, _, _, ExistingRuntime>(
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

    info!("starting liischte");
    let mut liischte = Liischte::new();
    liischte.init().await;

    // run iced app with surface
    app.run_with(move || (liischte, Task::none())).context("failed to start iced application")
}

#[derive(Debug, Clone)]
enum Message {
    Clock(ClockMessage),
    Hyprland(HyprlandMessage),
    Module(Box<dyn ModuleMessage>),

    Osd(OsdMessage),
    Output(OutputMessage),
    Ipc(IpcMessage),
}

struct Liischte {
    clock: Clock,
    hyprland: Option<Hyprland>,
    modules: IndexMap<ModuleId, Box<dyn AbstractModule>>,

    osd: Option<OsdHandler>,

    module_names: HashMap<String, ModuleId>,
    ipc: Option<IpcServer>,

    outputs: OutputHandler,
    alive: bool, // whether the surface is alive
    surface: SurfaceId,
}

impl Liischte {
    pub fn new() -> Self {
        Self {
            modules: IndexMap::new(),
            clock: Clock::new(),
            hyprland: None,

            osd: if CONFIG.osd.enabled { Some(OsdHandler::new()) } else { None },

            module_names: HashMap::new(),
            ipc: None,

            outputs: OutputHandler::new(),
            alive: false,
            surface: SurfaceId::unique(),
        }
    }

    /// initializes the liischte by initializing all required modules
    pub async fn init(&mut self) {
        if CONFIG.ipc {
            match IpcServer::run().await {
                Ok(server) => self.ipc = Some(server),
                Err(e) => {
                    error!("failed to start ipc server: {e:#}");
                }
            }
        }

        if CONFIG.hyprland.enabled {
            match Hyprland::new().await {
                Ok(hl) => self.hyprland = Some(hl),
                Err(e) => {
                    error!("failed to initialize hyprland: {e:#}");
                }
            }
        }

        for status in CONFIG.modules.iter().rev() {
            let module = match status.as_str() {
                POWER_MODULE_IDENTIFIER => PowerModule::new().await.map(module::boxed),
                BACKLIGHT_MODULE_IDENTIFIER => BacklightModule::new().await.map(module::boxed),
                NETWORK_MODULE_IDENTIFIER => NewtorkModule::new().await.map(module::boxed),
                PROCESS_MODULE_IDENTIFIER => ProcessModule::new().map(module::boxed),
                TIMER_MODULE_IDENTIFIER => Ok(module::boxed(TimerModule::new())),
                AUDIO_MODULE_IDENTIFIER => Ok(module::boxed(AudioModule::new())),
                status => panic!("status `{status}` does not exist in this version"),
            };

            match module {
                Ok(module) => {
                    info!("adding module `{status}` to bar");

                    self.module_names.insert(status.clone(), module.message_type());
                    self.modules.insert(module.message_type(), module);
                }
                Err(e) => {
                    error!("failed to initialize module `{status}`: {e:#}")
                }
            }
        }
    }

    fn open(&mut self, output: IcedOutput) -> Task<Message> {
        info!("opening bar layer surface");
        self.alive = true;

        if let Some(ref mut osd) = self.osd {
            osd.output = Some(output.clone());
        }

        get_layer_surface(SctkLayerSurfaceSettings {
            output,
            id: self.surface,

            layer: Layer::Top,
            anchor: Anchor::TOP
                | if CONFIG.right { Anchor::RIGHT } else { Anchor::LEFT }
                | Anchor::BOTTOM,

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

            Message::Module(msg) => {
                let id = (*msg).type_id();

                let (task, osd) = self
                    .modules
                    .get_mut(&id)
                    .expect("received status message for non-existent status")
                    .update(msg);

                if let Some(osd_id) = osd
                    && let Some(osd) = &mut self.osd
                {
                    Task::batch(vec![
                        task.map(Message::Module),
                        osd.request_osd(id, osd_id).map(Message::Osd),
                    ])
                } else {
                    task.map(Message::Module)
                }
            }

            Message::Osd(msg) => self
                .osd
                .as_mut()
                .expect("received osd without it enabled")
                .update(msg)
                .map(Message::Osd),

            Message::Output(msg) => {
                self.outputs.update(msg);

                if !self.alive
                    && let Some(output) = self.outputs.get_configured()
                {
                    self.open(output)
                } else {
                    Task::none()
                }
            }

            Message::Ipc(msg) => match msg {
                IpcMessage::ModuleUpdate(module, msg) => {
                    if let Some(module) =
                        self.module_names.get(&module).and_then(|id| self.modules.get(id))
                    {
                        if let Some(message) = module.pass_message(&msg) {
                            Task::done(Message::Module(message))
                        } else {
                            Task::none()
                        }
                    } else {
                        info!("module `{module}` not found when passing message");
                        Task::none()
                    }
                }
            },
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
                self.modules.values().map(|status| status.subscribe().map(Message::Module)),
            ),
            self.outputs.subscribe().map(Message::Output),
            self.ipc
                .as_ref()
                .map(|s| s.get_subscription().map(Message::Ipc))
                .unwrap_or(Subscription::none()),
        ])
    }

    fn view(&self, id: SurfaceId) -> iced::Element<'_, Message, Theme, iced::Renderer> {
        if id == self.surface {
            self.view_bar()
        } else if let Some(osd) = &self.osd
            && id == osd.surface
        {
            self.view_osd()
        } else {
            error!("tried to view unknown surface with id `{id}`");
            empty().into()
        }
    }

    fn view_bar(&self) -> iced::Element<'_, Message, Theme, iced::Renderer> {
        let mut infos = self
            .modules
            .values()
            .flat_map(|module| module.render_info().into_iter())
            .map(|info| info.map(Message::Module))
            .peekable();
        let has_infos = infos.peek().is_some();

        let status = self
            .modules
            .values()
            .filter(|module| module.has_status())
            .map(|module| module.render_status().map(Message::Module));

        column![
            self.hyprland
                .as_ref()
                .map(|hl| hl.render().map(Message::Hyprland))
                .unwrap_or_else(|| column![].into()),
            vertical_space(),
            Column::from_iter(infos).spacing(4),
            separator(has_infos),
            Column::from_iter(status).spacing(4),
            separator(true),
            self.clock.render().map(Message::Clock)
        ]
        .padding(Padding::ZERO.top(10).bottom(5)) // gives some visual balance
        .spacing(12)
        .align_x(Horizontal::Center)
        .width(Length::Fill)
        .into()
    }

    fn view_osd(&self) -> iced::Element<'_, Message, Theme, iced::Renderer> {
        let widget: iced::Element<'_, Message, Theme, iced::Renderer> =
            if let Some((ref id, ref osd)) =
                self.osd.as_ref().expect("rendering osd without enabled").get_active()
            {
                self.modules
                    .get(id)
                    .expect("tried to show osd from non-existent status")
                    .render_osd(*osd)
                    .map(Message::Module)
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
