use std::{cell::RefCell, collections::HashMap, io::Cursor, rc::Rc};

use log::{debug, trace, warn};
use pipewire::{
    node::{Node, NodeListener},
    spa::{
        param::ParamType,
        pod::{
            Object, Pod, Property, PropertyFlags, Value, ValueArray, deserialize::PodDeserializer,
            object, serialize::PodSerializer,
        },
        sys::{self, SPA_PROP_channelVolumes, SPA_PROP_mute},
        utils::{SpaTypes, dict::DictRef},
    },
};
use tokio::sync::broadcast::Sender;

/// all the nodes we are interrested here
#[derive(Clone, Copy, Debug, PartialEq)]
enum NodeClass {
    Source,
    Sink,
}

struct NodeTrackerObject {
    proxy: Node,
    _listener: NodeListener,

    class: NodeClass, // yes, we wrongly assume that this won't change for a node
    state: NodeState,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodeState {
    /// name of the node
    pub name: String,
    /// description (human readable name) of the node
    pub description: String,

    /// whether the node is muted
    pub mute: bool,
    /// current volume of each channel
    pub volume: Vec<f32>,
}

impl NodeState {
    fn update_props(&mut self, props: &DictRef) -> bool {
        let mut changed = false;

        if let Some(name) = props.get("node.name") {
            changed |= name != self.name;
            self.name = name.to_owned();
        }

        if let Some(description) = props.get("node.description") {
            changed |= description != self.description;
            self.description = description.to_owned();
        }

        return changed;
    }

    fn update_params(&mut self, params: &Object) -> bool {
        let mut changed = false;

        for prop in &params.properties {
            match (prop.key, &prop.value) {
                (sys::SPA_PROP_channelVolumes, Value::ValueArray(ValueArray::Float(value))) => {
                    // convert the volume to "visual" form, because linear is not really useful
                    let mut value = value.clone();
                    for ele in &mut value {
                        *ele = ele.powf(1f32 / 3f32);
                    }

                    changed |= *value != self.volume;
                    self.volume = value.clone();
                }
                (sys::SPA_PROP_mute, Value::Bool(value)) => {
                    changed |= *value != self.mute;
                    self.mute = *value;
                }
                _ => {}
            }
        }

        return changed;
    }
}

impl Default for NodeState {
    fn default() -> Self {
        Self { name: String::new(), description: String::new(), mute: false, volume: Vec::new() }
    }
}

pub(crate) struct NodeTracker {
    sink_updates: Sender<Vec<NodeState>>,
    source_updates: Sender<Vec<NodeState>>,

    nodes: RefCell<HashMap<u32, NodeTrackerObject>>,
}

impl NodeTracker {
    pub fn new(
        sink_updates: Sender<Vec<NodeState>>,
        source_updates: Sender<Vec<NodeState>>,
    ) -> Self {
        Self { nodes: RefCell::new(HashMap::new()), sink_updates, source_updates }
    }

    /// tries to add a node, if it is of interrest
    pub fn add<F>(self: &Rc<Self>, id: u32, props: &DictRef, bind: F)
    where
        F: FnOnce() -> Option<Node>,
    {
        let class = match props.get("media.class") {
            None => return,
            Some("Audio/Sink") => NodeClass::Sink,
            Some("Audio/Source") => NodeClass::Source,
            Some(class) => {
                trace!("skipping bind to node of class '{}'", class);
                return;
            }
        };

        // it is the correct class, bind!
        let Some(node) = bind() else {
            return;
        };

        let listener = node
            .add_listener_local()
            .info({
                let this = self.clone();
                move |info| {
                    if let Some(props) = info.props() {
                        this.update_props(id, props);
                    }
                }
            })
            .param({
                let this = self.clone();
                move |_, what, _, _, pod| {
                    if let (ParamType::Props, Some(pod)) = (what, pod) {
                        this.update_params(id, pod);
                    }
                }
            })
            .register();

        // subscribe to prop params
        node.enum_params(0, Some(ParamType::Props), 0, u32::MAX);
        node.subscribe_params(&[ParamType::Props]);

        let mut state = NodeState::default();
        state.update_props(props);

        debug!("adding node {id} to tracker ('{}')", state.name);
        self.nodes.borrow_mut().insert(
            id,
            NodeTrackerObject { proxy: node, _listener: listener, class: class, state: state },
        );

        self.update(class);
    }

    /// removes a node
    pub fn remove(&self, id: u32) {
        let result = self.nodes.borrow_mut().remove(&id);

        if let Some(removed) = result {
            debug!("removing node {id} from tracker");
            self.update(removed.class);
        }
    }

    /// updates the props of a tracked node
    fn update_props(&self, id: u32, props: &DictRef) {
        let mut changed = None;

        if let Some(node) = self.nodes.borrow_mut().get_mut(&id) {
            trace!("updating props for {id}");
            if node.state.update_props(props) {
                changed = Some(node.class);
            }
        } else {
            warn!("tried to update props for node {id} which is not tracked");
        }

        if let Some(changed) = changed {
            self.update(changed);
        }
    }

    /// updates the params of a tracked node
    fn update_params(&self, id: u32, params: &Pod) {
        let mut changed = None;

        if let Some(node) = self.nodes.borrow_mut().get_mut(&id) {
            trace!("updating params for {id}");

            match PodDeserializer::deserialize_any_from(params.as_bytes()) {
                Err(e) => warn!("failed to deserialize params for {id}: {e:?}"),
                Ok((_, Value::Object(obj))) => {
                    if node.state.update_params(&obj) {
                        changed = Some(node.class);
                    }
                }
                Ok((_, _)) => {
                    warn!("received non-object body for params");
                }
            }
        } else {
            warn!("tried to update params for node {id} which is not tracked");
        }

        if let Some(changed) = changed {
            self.update(changed);
        }
    }

    /// broadcasts an update
    fn update(&self, class: NodeClass) {
        trace!("sending update for class {class:?}");

        let data = self
            .nodes
            .borrow()
            .values()
            .filter(|t| t.class == class)
            .map(|t| t.state.clone())
            .collect::<Vec<_>>();

        let sender = match class {
            NodeClass::Source => &self.source_updates,
            NodeClass::Sink => &self.sink_updates,
        };

        if sender.send(data).is_err() {
            warn!("failed to send {class:?} node update to channel");
        }
    }

    /// set the volume of a node
    pub fn set_volume(&self, name: &str, mut volume: Vec<f32>) {
        // we assume the volume is in "visual" form, i.e. not linear like what pw tracks
        for ele in &mut volume {
            *ele = ele.max(0f32).powi(3); // the cube root seems what everyone uses
        }

        self.set(
            name,
            object! {
                SpaTypes::ObjectParamProps,
                ParamType::Props,
                Property {
                    key: SPA_PROP_channelVolumes,
                    flags: PropertyFlags::empty(),
                    value: Value::ValueArray(ValueArray::Float(volume))
                }
            },
        );
    }

    /// set the mute state of a node
    pub fn set_mute(&self, name: &str, mute: bool) {
        self.set(
            name,
            object! {
                SpaTypes::ObjectParamProps,
                ParamType::Props,
                Property {
                    key: SPA_PROP_mute,
                    flags: PropertyFlags::empty(),
                    value: Value::Bool(mute)
                }
            },
        );
    }

    fn set(&self, name: &str, object: Object) {
        let state = self.nodes.borrow();
        let Some(node) = state.values().find(|obj| obj.state.name == name) else {
            warn!("cannot set property for node '{name}', it is not tracked");
            return;
        };

        let Ok(bytes) = PodSerializer::serialize(Cursor::new(Vec::new()), &Value::Object(object))
            .map(|(c, _)| c.into_inner())
        else {
            warn!("failed to serialize property for node '{name}'");
            return;
        };

        let Some(pod) = Pod::from_bytes(&bytes) else {
            warn!("failed to create pod from bytes for node '{name}'");
            return;
        };

        node.proxy.set_param(ParamType::Props, 0, pod);
    }

    /// triggers a manual update in the channel
    pub fn trigger_update(&self) {
        self.update(NodeClass::Sink);
        self.update(NodeClass::Source);
    }
}
