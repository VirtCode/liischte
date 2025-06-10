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

/// this trait makes sure downcasting works for the message of the custom status
pub trait StatusMessage: DynClone + Any + Send + Debug {}
clone_trait_object!(StatusMessage);
downcast!(dyn StatusMessage);

/// a status is an icon on the bar which is _permanently_ there and conveys the
/// status of some system function
#[async_trait]
pub trait Status: Send {
    type Message: StatusMessage;

    /// the iced subscribe method which returns subscriptions
    fn subscribe(&self) -> Subscription<Self::Message>;

    /// the iced update method which mutates the state based on messages
    /// received
    fn update(&mut self, message: &Self::Message) -> (Task<Self::Message>, bool);

    /// the iced render method, which renders the status based on internal state
    fn render(&self) -> Element<'_, Self::Message, Theme, Renderer>;

    /// the iced render method, which renders the osd
    fn render_osd(&self) -> Element<'_, Self::Message, Theme, Renderer> {
        // just show nothing if the status did not implement it
        empty().into()
    }
}

/// a trait which removes the implementation specific types and makes the status
/// handling extensible. this trait will down- and upcast all messages to allow
/// handling of statusses in one place. see the `Status` trait for method
/// descriptions
#[async_trait]
pub trait AbstractStatus: Send {
    fn message_type(&self) -> TypeId;

    fn subscribe(&self) -> Subscription<Box<dyn StatusMessage>>;

    fn update(&mut self, message: Box<dyn StatusMessage>) -> (Task<Box<dyn StatusMessage>>, bool);

    fn render(&self) -> Element<'_, Box<dyn StatusMessage>, Theme, Renderer>;

    fn render_osd(&self) -> Element<'_, Box<dyn StatusMessage>, Theme, Renderer>;
}

#[async_trait]
impl<T: Status> AbstractStatus for T {
    fn message_type(&self) -> TypeId {
        TypeId::of::<<T as Status>::Message>()
    }

    fn subscribe(&self) -> Subscription<Box<dyn StatusMessage>> {
        Status::subscribe(self).map(|msg| -> Box<dyn StatusMessage> { Box::new(msg) })
    }

    fn update(&mut self, message: Box<dyn StatusMessage>) -> (Task<Box<dyn StatusMessage>>, bool) {
        trace!(
            "passing status message for {}",
            std::any::type_name_of_val(self).rsplit("::").next().unwrap_or_default()
        );

        let Ok(heap) = message
            .downcast::<<T as Status>::Message>()
            .map_err(|e| panic!("received invalid type for status message: {e:#}"));

        let (task, osd) = Status::update(self, &heap);

        (task.map(|msg| -> Box<dyn StatusMessage> { Box::new(msg) }), osd)
    }

    fn render(&self) -> Element<'_, Box<dyn StatusMessage>, Theme, Renderer> {
        Status::render(self).map(|msg| -> Box<dyn StatusMessage> { Box::new(msg) })
    }

    fn render_osd(&self) -> Element<'_, Box<dyn StatusMessage>, Theme, Renderer> {
        Status::render_osd(self).map(|msg| -> Box<dyn StatusMessage> { Box::new(msg) })
    }
}
