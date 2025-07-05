use std::{cell::RefCell, cmp::max, collections::HashMap, io::Cursor, rc::Rc};

use log::{debug, error, trace, warn};
use pipewire::{
    device::{Device, DeviceListener},
    node::{Node, NodeListener},
    spa::{
        param::ParamType,
        pod::{
            Object, Pod, Property, PropertyFlags, Value, ValueArray, deserialize::PodDeserializer,
            object, serialize::PodSerializer,
        },
        sys::{
            self, SPA_PARAM_ROUTE_device, SPA_PARAM_ROUTE_index, SPA_PARAM_ROUTE_props,
            SPA_PARAM_ROUTE_save, SPA_PROP_channelVolumes, SPA_PROP_mute,
        },
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

    device: Option<u32>,
    class: NodeClass, // yes, we wrongly assume that this won't change for a node
    state: NodeState,
}

struct DeviceTrackerObject {
    proxy: Device,
    _listener: DeviceListener,

    indices: HashMap<u32, u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodeState {
    /// internal pipewire node id
    pub id: u32,

    /// name of the node
    pub name: String,
    /// description (human readable name) of the node
    pub description: String,

    /// whether the node is muted
    pub mute: bool,
    /// current volume of each channel
    pub volume: Vec<f32>,

    /// profile that this node is on a given device
    pub route: Option<u32>,
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

        if let Some(id) = props.get("card.profile.device").and_then(|id| {
            id.parse::<u32>().map_err(|_| warn!("card profile is not an integer")).ok()
        }) {
            changed |= Some(id) != self.route;
            self.route = Some(id);
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

impl NodeState {
    fn new(id: u32) -> Self {
        Self {
            id,
            name: String::new(),
            description: String::new(),
            mute: false,
            volume: Vec::new(),
            route: None,
        }
    }

    pub fn average_volume(&self) -> f32 {
        self.volume.iter().sum::<f32>() / max(self.volume.len(), 1) as f32
    }
}

pub(crate) struct NodeTracker {
    sink_updates: Sender<Vec<NodeState>>,
    source_updates: Sender<Vec<NodeState>>,

    nodes: RefCell<HashMap<u32, NodeTrackerObject>>,
    devices: RefCell<HashMap<u32, DeviceTrackerObject>>,
}

impl NodeTracker {
    pub fn new(
        sink_updates: Sender<Vec<NodeState>>,
        source_updates: Sender<Vec<NodeState>>,
    ) -> Self {
        Self {
            nodes: RefCell::new(HashMap::new()),
            devices: RefCell::new(HashMap::new()),
            sink_updates,
            source_updates,
        }
    }

    /// tries to add a node, if it is of interrest
    pub fn add_node<F>(self: &Rc<Self>, id: u32, props: &DictRef, bind: F)
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

        let device = props.get("device.id").and_then(|id| {
            id.parse::<u32>().map_err(|_| warn!("node device.id was not an integer")).ok()
        });

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
                        this.update_params_node(id, pod);
                    }
                }
            })
            .register();

        // subscribe to prop params
        node.enum_params(0, Some(ParamType::Props), 0, u32::MAX);
        node.subscribe_params(&[ParamType::Props]);

        let mut state = NodeState::new(id);
        state.update_props(props);

        debug!(
            "adding node {id} to tracker ('{}', device {})",
            state.name,
            device.map(|a| a.to_string()).unwrap_or("<none>".to_string())
        );

        self.nodes.borrow_mut().insert(
            id,
            NodeTrackerObject { proxy: node, _listener: listener, class, state, device },
        );

        self.update(class);
    }

    /// adds a device to be tracked
    pub fn add_device(self: &Rc<Self>, id: u32, _props: &DictRef, device: Device) {
        let listener = device
            .add_listener_local()
            .info({
                let this = self.clone();
                move |info| {
                    for param in info.params() {
                        // we enumerate the route param if it changed
                        // subscribing doesn't cut it for some reason
                        if param.id() == ParamType::Route
                            && let Some(device) = this.devices.borrow().get(&id)
                        {
                            device.proxy.enum_params(0, Some(ParamType::Route), 0, u32::MAX);
                        }
                    }
                }
            })
            .param({
                let this = self.clone();
                move |_, what, _, _, pod| {
                    if let (ParamType::Route, Some(pod)) = (what, pod) {
                        this.update_params_device(id, pod);
                    }
                }
            })
            .register();

        device.enum_params(0, Some(ParamType::Route), 0, u32::MAX);
        device.subscribe_params(&[ParamType::Route]); // does nothing but we do it anyways

        debug!("adding device {id}");

        self.devices.borrow_mut().insert(
            id,
            DeviceTrackerObject { proxy: device, _listener: listener, indices: HashMap::new() },
        );
    }

    /// removes an object
    pub fn remove(&self, id: u32) {
        let result = self.nodes.borrow_mut().remove(&id); // for borrow lifetime
        if let Some(removed) = result {
            debug!("removing node {id} from tracker");
            self.update(removed.class);
        }

        if self.devices.borrow_mut().remove(&id).is_some() {
            debug!("removing device {id} from tracker");
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

    fn update_params_device(&self, id: u32, params: &Pod) {
        trace!("updating device params for {id}");

        let mut route = None;
        let mut index = None;
        let mut node_params = None;

        match PodDeserializer::deserialize_any_from(params.as_bytes()) {
            Err(e) => warn!("failed to deserialize params for device: {e:?}"),
            Ok((_, Value::Object(obj))) => {
                for prop in obj.properties {
                    match (prop.key, prop.value) {
                        (sys::SPA_PARAM_ROUTE_device, Value::Int(value)) => {
                            route = Some(value as u32)
                        }
                        (sys::SPA_PARAM_ROUTE_index, Value::Int(value)) => {
                            index = Some(value as u32)
                        }
                        (sys::SPA_PARAM_ROUTE_props, Value::Object(value)) => {
                            node_params = Some(value)
                        }
                        _ => {}
                    }
                }
            }
            Ok((_, _)) => {
                warn!("received non-object body for device params");
            }
        }

        if let (Some(route), Some(index), Some(params)) = (route, index, node_params) {
            let mut changed = None;

            if let Some(device) = self.devices.borrow_mut().get_mut(&id) {
                device.indices.insert(route, index);
            } else {
                warn!("received route index update for device {id} that does not exist");
            }

            if let Some(node) = self
                .nodes
                .borrow_mut()
                .values_mut()
                .find(|node| node.device == Some(id) && node.state.route == Some(route))
            {
                if node.state.update_params(&params) {
                    changed = Some(node.class);
                }
            } else {
                debug!("received update for route {route} on device {id}, but no node consumed it");
            }

            if let Some(changed) = changed {
                self.update(changed);
            }
        } else {
            warn!("received incomplete device route param update")
        }
    }

    /// updates the params of a node if it is tracked
    fn update_params_node(&self, id: u32, params: &Pod) {
        let mut changed = None;

        if let Some(node) = self.nodes.borrow_mut().get_mut(&id) {
            if node.device.is_some() {
                debug!("skipping param update for node {id}, because it has a device");
                return;
            }

            trace!("updating node params for {id}");

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

        if let Some(device_id) = node.device {
            trace!("setting properties on device {device_id} for `{}`", node.state.name);
            let Some(route) = node.state.route else {
                error!("no route found for node `{}`", node.state.name);
                return;
            };

            let state = self.devices.borrow();
            let Some(device) = state.get(&device_id) else {
                error!("device {device_id} was not tracked, cannot set property");
                return;
            };

            let Some(index) = device.indices.get(&route) else {
                error!("no index for route {route} of device {device_id}");
                return;
            };

            let flags = PropertyFlags::empty();
            let object = object! {
                SpaTypes::ObjectParamRoute,
                ParamType::Route,
                Property { key: SPA_PARAM_ROUTE_device, value: Value::Int(route as i32), flags },
                Property { key: SPA_PARAM_ROUTE_index, value: Value::Int(*index as i32), flags },
                Property { key: SPA_PARAM_ROUTE_props, value: Value::Object(object), flags },
                Property { key: SPA_PARAM_ROUTE_save, value: Value::Bool(true), flags },
            };

            let Ok(bytes) =
                PodSerializer::serialize(Cursor::new(Vec::new()), &Value::Object(object))
                    .map(|(c, _)| c.into_inner())
            else {
                error!("failed to serialize property for node '{name}'");
                return;
            };

            let Some(pod) = Pod::from_bytes(&bytes) else {
                error!("failed to create pod from bytes for node '{name}'");
                return;
            };

            device.proxy.set_param(ParamType::Route, 0, pod);
        } else {
            trace!("setting properties directly `{}`", node.state.name);

            let Ok(bytes) =
                PodSerializer::serialize(Cursor::new(Vec::new()), &Value::Object(object))
                    .map(|(c, _)| c.into_inner())
            else {
                error!("failed to serialize property for node '{name}'");
                return;
            };

            let Some(pod) = Pod::from_bytes(&bytes) else {
                error!("failed to create pod from bytes for node '{name}'");
                return;
            };

            node.proxy.set_param(ParamType::Props, 0, pod);
        }
    }

    /// triggers a manual update in the channel
    pub fn trigger_update(&self) {
        self.update(NodeClass::Sink);
        self.update(NodeClass::Source);
    }
}
