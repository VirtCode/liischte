use std::{rc::Rc, thread};

use anyhow::{Context as _, Result, anyhow};
use futures::StreamExt;
use log::{trace, warn};
use pipewire::{
    channel::{self as pwchannel, Receiver as PwReceiver, Sender as PwSender},
    context::ContextRc,
    main_loop::MainLoopRc,
    registry::{GlobalObject, RegistryRc},
    spa::utils::dict::DictRef,
};
use tokio::sync::broadcast::{self, Receiver as BcReceiver, Sender as BcSender};
use tokio_stream::wrappers::BroadcastStream;

use crate::{
    StaticStream, StreamContext,
    pipewire::{
        default::{DefaultState, DefaultTracker},
        node::{NodeState, NodeTracker},
    },
};

pub mod default;
pub mod node;

pub struct PipewireInstance {
    sinks: BcReceiver<Vec<NodeState>>,
    sources: BcReceiver<Vec<NodeState>>,
    defaults: BcReceiver<DefaultState>,
    actions: PwSender<PipewireAction>,
}

impl PipewireInstance {
    /// start the pipewire instance
    /// this will create a new thread which will communicate directly with
    /// pipewire
    pub fn start() -> Self {
        let (sinks_tx, sinks_rx) = broadcast::channel(1);
        let (sources_tx, sources_rx) = broadcast::channel(1);
        let (defaults_tx, defaults_rx) = broadcast::channel(1);
        let (actions_tx, actions_rx) = pwchannel::channel();

        thread::spawn(|| {
            if let Err(e) = PipewireThread::run(sinks_tx, sources_tx, defaults_tx, actions_rx) {
                warn!("failed to run pipewire thread: {e:#}");
            };
        });

        PipewireInstance {
            sinks: sinks_rx,
            sources: sources_rx,
            defaults: defaults_rx,
            actions: actions_tx,
        }
    }

    /// listen to changes to the system's used default devices (sink and source)
    pub fn listen_defaults(&self) -> StaticStream<DefaultState> {
        BroadcastStream::new(self.defaults.resubscribe())
            .filter_map(async |r| {
                r.stream_context("pw defaults", "failed to receive from broadcast")
            })
            .boxed()
    }

    /// listen to changes to the system's sinks
    pub fn listen_sinks(&self) -> StaticStream<Vec<NodeState>> {
        BroadcastStream::new(self.sinks.resubscribe())
            .filter_map(async |r| r.stream_context("pw sinks", "failed to receive from broadcast"))
            .boxed()
    }

    /// listen to changes to the system's sources
    pub fn listen_sources(&self) -> StaticStream<Vec<NodeState>> {
        BroadcastStream::new(self.sources.resubscribe())
            .filter_map(async |r| {
                r.stream_context("pw sources", "failed to receive from broadcast")
            })
            .boxed()
    }

    /// set the default sink the system uses
    pub fn set_default_sink(&self, name: &str) -> Result<()> {
        self.send_command(PipewireAction::DefaultSink(name.to_string()))
    }

    /// set the default source the system uses
    pub fn set_default_source(&self, name: &str) -> Result<()> {
        self.send_command(PipewireAction::DefaultSource(name.to_string()))
    }

    /// sets the given node's volume for each channel
    /// make sure your channel slice has the right amount of entries
    pub fn set_volume(&self, name: &str, volume: &[f32]) -> Result<()> {
        self.send_command(PipewireAction::NodeVolume(name.to_string(), volume.to_owned()))
    }

    /// sets the given node's mute state
    pub fn set_mute(&self, name: &str, mute: bool) -> Result<()> {
        self.send_command(PipewireAction::NodeMute(name.to_string(), mute))
    }

    /// triggers a manual update to be sent thorugh every listening channel
    pub fn trigger_update(&self) -> Result<()> {
        self.send_command(PipewireAction::Update)
    }

    /// sends a command through the channel to the thread
    fn send_command(&self, command: PipewireAction) -> Result<()> {
        self.actions
            .send(command)
            .map_err(|_| anyhow!("failed to communicate with pipewire thread"))
    }
}

/// this can be sent to the pipewire thread to do something
/// usually takes the device name as first argument
enum PipewireAction {
    DefaultSink(String),
    DefaultSource(String),
    NodeVolume(String, Vec<f32>),
    NodeMute(String, bool),
    Update, // sends an update through every channel
}

struct PipewireThread {
    registry: RegistryRc,

    default: DefaultTracker,
    nodes: Rc<NodeTracker>,
}

impl PipewireThread {
    fn run(
        sinks: BcSender<Vec<NodeState>>,
        sources: BcSender<Vec<NodeState>>,
        defaults: BcSender<DefaultState>,
        actions: PwReceiver<PipewireAction>,
    ) -> Result<()> {
        let mainloop = MainLoopRc::new(None).context("failed to create new pipewire mainloop")?;

        trace!("connecting to pipewire");
        let context =
            ContextRc::new(&mainloop, None).context("failed to create pipewire context")?;
        let core = context.connect_rc(None).context("failed to connect to pipewire")?;
        let registry = core.get_registry_rc().context("failed to retrieve pipewire registry")?;

        let state = Rc::new(Self {
            registry: registry,

            default: DefaultTracker::new(defaults),
            nodes: Rc::new(NodeTracker::new(sinks, sources)),
        });

        let _global = state
            .registry
            .add_listener_local()
            .global({
                let state = state.clone();
                move |global| {
                    state.global(global);
                }
            })
            .global_remove({
                let state = state.clone();
                move |id| {
                    state.global_remove(id);
                }
            })
            .register();

        let _attached = actions.attach(mainloop.loop_(), move |action| {
            state.action(action);
        });

        trace!("entering pipewire mainloop");
        mainloop.run();
        trace!("exited pipewire mainloop");

        Ok(())
    }

    fn global_remove(self: &Rc<Self>, id: u32) {
        self.default.detach(id);
        self.nodes.remove(id);
    }

    fn global(self: &Rc<Self>, global: &GlobalObject<&DictRef>) {
        match global.type_ {
            pipewire::types::ObjectType::Metadata => {
                let Some(props) = global.props else {
                    return;
                };

                if props.get("metadata.name") == Some("default") {
                    let Ok(metadata) = self.registry.bind(global) else {
                        warn!("failed to bind to metadata object");
                        return;
                    };

                    self.default.attach(metadata, global.id);
                }
            }
            pipewire::types::ObjectType::Node => {
                let Some(props) = global.props else {
                    return;
                };

                self.nodes.add_node(global.id, props, || self.registry.bind(global).ok());
            }
            pipewire::types::ObjectType::Device => {
                let Some(props) = global.props else {
                    return;
                };

                let Ok(device) = self.registry.bind(global) else {
                    return;
                };

                self.nodes.add_device(global.id, props, device);
            }
            _ => {}
        }
    }

    fn action(self: &Rc<Self>, action: PipewireAction) {
        match action {
            PipewireAction::DefaultSink(name) => self.default.set_sink(Some(&name)),
            PipewireAction::DefaultSource(name) => self.default.set_source(Some(&name)),
            PipewireAction::NodeVolume(name, volume) => self.nodes.set_volume(&name, volume),
            PipewireAction::NodeMute(name, mute) => self.nodes.set_mute(&name, mute),

            PipewireAction::Update => {
                self.default.trigger_update();
                self.nodes.trigger_update();
            }
        }
    }
}
