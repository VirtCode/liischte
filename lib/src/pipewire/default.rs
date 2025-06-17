use std::{cell::RefCell, rc::Rc};

use log::{info, warn};
use pipewire::metadata::{Metadata, MetadataListener};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::Sender;

pub(crate) struct DefaultTracker {
    inner: RefCell<Option<(u32, Metadata, MetadataListener)>>,

    updates: Sender<DefaultState>,
    state: Rc<RefCell<DefaultState>>,
}

impl DefaultTracker {
    /// create a new metadata tracker
    pub fn new(updates: Sender<DefaultState>) -> Self {
        Self {
            inner: RefCell::new(None),
            state: Rc::new(RefCell::new(DefaultState::default())),
            updates,
        }
    }

    /// attaches a given metadata proxy to the tracker
    pub fn attach(&self, metadata: Metadata, id: u32) {
        let listener = metadata
            .add_listener_local()
            .property({
                let state = self.state.clone();
                let updates = self.updates.clone();

                move |_, key, _, value| {
                    let mut state = state.borrow_mut();

                    if state.update(key, value) {
                        if updates.send(state.clone()).is_err() {
                            warn!("failed to send default update to channel");
                        }
                    }

                    0
                }
            })
            .register();

        *self.inner.borrow_mut() = Some((id, metadata, listener));
    }

    /// potentially detaches the metadata proxy if ids match
    pub fn detach(&self, id: u32) {
        if self.inner.borrow().as_ref().map(|(ours, _, _)| *ours == id).unwrap_or_default() {
            info!("default metadata was removed from graph");
            *self.inner.borrow_mut() = None;
        }
    }

    /// set a property on the metadata
    fn set_default(&self, key: &str, value: Option<&str>) {
        let json = value.map(|s| {
            serde_json::to_string(&DefaultNodeValue { name: s.to_string() })
                .expect("should be able to serialize")
        });

        if let Some((_, proxy, _)) = &*self.inner.borrow() {
            proxy.set_property(0, key, Some("Spa:String:JSON"), json.as_ref().map(|s| s.as_str()));
        } else {
            warn!("cannot set default device, default metadata is not attached");
        }
    }

    /// set the configured default sink
    pub fn set_sink(&self, sink: Option<&str>) {
        self.set_default("default.configured.audio.sink", sink);
    }

    /// set the configured default source
    pub fn set_source(&self, source: Option<&str>) {
        self.set_default("default.configured.audio.source", source);
    }

    /// triggers a manual update in the channel
    pub fn trigger_update(&self) {
        if self.updates.send(self.state.borrow().clone()).is_err() {
            warn!("failed to send triggered update to channel");
        }
    }
}

const DEFAULT_STATE_UNKNOWN: &str = "unknown";

#[derive(Clone, Debug)]
pub struct DefaultState {
    /// sink the user chose as default
    pub configured_sink: String,
    /// actual sink used as default
    pub sink: String,
    /// source the user chose as default
    pub configured_source: String,
    /// actual source used as default
    pub source: String,
}

impl Default for DefaultState {
    fn default() -> Self {
        Self {
            configured_sink: DEFAULT_STATE_UNKNOWN.to_string(),
            sink: DEFAULT_STATE_UNKNOWN.to_string(),
            configured_source: DEFAULT_STATE_UNKNOWN.to_string(),
            source: DEFAULT_STATE_UNKNOWN.to_string(),
        }
    }
}

impl DefaultState {
    fn update(&mut self, key: Option<&str>, value: Option<&str>) -> bool {
        let Some(key) = key else {
            // the docs mention that a null key means the removal of all values,
            // but this is sometimes sent for no reason so we just ignore it here
            // *self = Self::default();

            return false;
        };

        let var = match key {
            "default.configured.audio.sink" => &mut self.configured_sink,
            "default.audio.sink" => &mut self.sink,
            "default.configured.audio.source" => &mut self.configured_source,
            "default.audio.source" => &mut self.source,

            _ => {
                warn!("unrecognized key '{key}' for default metadata");
                return false;
            }
        };

        *var = value
            .and_then(|s| -> Option<String> {
                serde_json::from_str::<DefaultNodeValue>(s)
                    .map(|v| v.name)
                    .map_err(|_| warn!("failed to parse value for '{key}': {s}"))
                    .ok()
            })
            .unwrap_or_else(|| DEFAULT_STATE_UNKNOWN.to_string());

        return true;
    }
}

#[derive(Deserialize, Serialize)]
struct DefaultNodeValue {
    name: String,
}
