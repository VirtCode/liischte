use std::{any::TypeId, fmt::Debug};

use async_trait::async_trait;
use downcast::{Any, Downcast, downcast};
use dyn_clone::{DynClone, clone_trait_object};
use iced::{Element, Renderer, Subscription, Task, Theme, widget::Space};
use log::trace;

use crate::ui::empty;

pub mod audio;
pub mod network;
pub mod power;

/// this trait makes sure downcasting works for the message of the custom module
pub trait ModuleMessage: DynClone + Any + Send + Debug {}
clone_trait_object!(ModuleMessage);
downcast!(dyn ModuleMessage);

/// a module is a part of the bar covering a certain part of system information
#[async_trait]
pub trait Module: Send {
    type Message: ModuleMessage;

    /// the iced subscribe method which returns subscriptions
    fn subscribe(&self) -> Subscription<Self::Message>;

    /// the iced update method which mutates the state based on messages
    /// received
    fn update(&mut self, message: &Self::Message) -> (Task<Self::Message>, bool);

    /// the iced render method, which renders the status based on internal state
    fn render_status(&self) -> Element<'_, Self::Message, Theme, Renderer>;

    /// the iced render method, which renders the osd
    fn render_osd(&self) -> Element<'_, Self::Message, Theme, Renderer> {
        // just show nothing if the status did not implement it
        empty().into()
    }
}

/// a trait which removes the implementation specific types and makes the module
/// handling extensible. this trait will down- and upcast all messages to allow
/// handling of modules in one place. see the `Module` trait for method
/// descriptions
#[async_trait]
pub trait AbstractModule: Send {
    fn message_type(&self) -> TypeId;

    fn subscribe(&self) -> Subscription<Box<dyn ModuleMessage>>;

    fn update(&mut self, message: Box<dyn ModuleMessage>) -> (Task<Box<dyn ModuleMessage>>, bool);

    fn render_status(&self) -> Element<'_, Box<dyn ModuleMessage>, Theme, Renderer>;

    fn render_osd(&self) -> Element<'_, Box<dyn ModuleMessage>, Theme, Renderer>;
}

#[async_trait]
impl<T: Module> AbstractModule for T {
    fn message_type(&self) -> TypeId {
        TypeId::of::<<T as Module>::Message>()
    }

    fn subscribe(&self) -> Subscription<Box<dyn ModuleMessage>> {
        Module::subscribe(self).map(|msg| -> Box<dyn ModuleMessage> { Box::new(msg) })
    }

    fn update(&mut self, message: Box<dyn ModuleMessage>) -> (Task<Box<dyn ModuleMessage>>, bool) {
        trace!(
            "passing module message for {}",
            std::any::type_name_of_val(self).rsplit("::").next().unwrap_or_default()
        );

        let Ok(heap) = message
            .downcast::<<T as Module>::Message>()
            .map_err(|e| panic!("received invalid type for module message: {e:#}"));

        let (task, osd) = Module::update(self, &heap);

        (task.map(|msg| -> Box<dyn ModuleMessage> { Box::new(msg) }), osd)
    }

    fn render_status(&self) -> Element<'_, Box<dyn ModuleMessage>, Theme, Renderer> {
        Module::render_status(self).map(|msg| -> Box<dyn ModuleMessage> { Box::new(msg) })
    }

    fn render_osd(&self) -> Element<'_, Box<dyn ModuleMessage>, Theme, Renderer> {
        Module::render_osd(self).map(|msg| -> Box<dyn ModuleMessage> { Box::new(msg) })
    }
}
