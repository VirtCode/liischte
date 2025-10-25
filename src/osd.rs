use std::time::Duration;

use iced::{
    Limits, Task,
    runtime::platform_specific::wayland::layer_surface::{
        IcedMargin, IcedOutput, SctkLayerSurfaceSettings,
    },
    task::Handle,
    window::Id,
};
use iced_winit::commands::{
    layer_surface::{destroy_layer_surface, get_layer_surface},
    subsurface::{Anchor, Layer},
};
use log::debug;
use tokio::time::sleep;

use crate::{config::CONFIG, module::ModuleId};

/// an id that can be returned by a module to differentiate betweent it's own
/// different osds, different ids will cause respawning
pub type OsdId = u32;

pub struct OsdHandler {
    current: Option<(ModuleId, OsdId)>,
    last: Option<(ModuleId, OsdId)>, // iced re-renders before the surface is closed

    timeout: Option<Handle>,
    respawning: bool,

    pub output: Option<IcedOutput>,
    pub surface: Id,
}

#[derive(Debug, Clone)]
pub enum OsdMessage {
    Close,
    Respawn,
}

impl OsdHandler {
    /// create a new handler
    pub fn new() -> Self {
        Self {
            current: None,
            last: None,
            timeout: None,
            respawning: false,
            surface: Id::unique(),
            output: None,
        }
    }

    /// update the inner state based on a message
    pub fn update(&mut self, message: OsdMessage) -> Task<OsdMessage> {
        match message {
            OsdMessage::Close => {
                debug!("closing osd layer");
                let last = self.current.take();

                self.destroy_surface(last)
            }
            OsdMessage::Respawn => {
                debug!("respawning osd layer");
                self.respawning = false;

                Task::batch(vec![self.create_surface(), self.reset_timeout()])
            }
        }
    }

    /// requests the osd for a given id
    pub fn request_osd(&mut self, id: ModuleId, osd: OsdId) -> Task<OsdMessage> {
        let same = self.current == Some((id, osd));
        let alive = self.current.is_some();

        let last = self.current;
        self.current = Some((id, osd));

        let task = match (alive, same, self.respawning) {
            // spawn surface if not alive and not respawning
            (false, _, false) => {
                debug!("spawning osd layer");
                self.create_surface()
            }
            // respawn surface if alive but not the same (and not already respawning)
            (true, false, false) => {
                debug!("closing osd layer for respawn");
                self.respawning = true;

                Task::batch(vec![
                    self.destroy_surface(last),
                    Task::future(async {
                        sleep(Duration::from_millis(CONFIG.osd.respawn_time)).await;
                        OsdMessage::Respawn
                    }),
                ])
            }
            _ => Task::none(),
        };

        Task::batch(vec![task, self.reset_timeout()])
    }

    /// returns the active osd
    pub fn get_active(&self) -> Option<(ModuleId, u32)> {
        self.current.or(self.last)
    }

    fn reset_timeout(&mut self) -> Task<OsdMessage> {
        let (timeout, handle) = Task::abortable(Task::future(async {
            sleep(Duration::from_millis(CONFIG.osd.timeout)).await;
            OsdMessage::Close
        }));

        self.timeout = Some(handle.abort_on_drop());
        timeout
    }

    fn destroy_surface(&mut self, last: Option<(ModuleId, u32)>) -> Task<OsdMessage> {
        self.last = last;

        destroy_layer_surface(self.surface)
    }

    fn create_surface(&mut self) -> Task<OsdMessage> {
        let Some(output) = self.output.clone() else {
            self.current = None;
            return Task::none();
        };

        get_layer_surface(SctkLayerSurfaceSettings {
            output,
            id: self.surface,

            layer: CONFIG.osd.layer.into(),
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
            exclusive_zone: -1,
            size_limits: Limits::NONE,

            pointer_interactivity: false,
            namespace: format!("{}-osd", CONFIG.namespace),

            ..Default::default()
        })
    }
}
