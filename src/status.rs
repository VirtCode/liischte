use std::{
    any::{TypeId, type_name},
    fmt::Debug,
    time::Duration,
};

use async_trait::async_trait;
use chrono::{Local, Timelike};
use downcast::{Any, Downcast, downcast};
use dyn_clone::{DynClone, clone_trait_object};
use iced::{Element, Renderer, Subscription, Theme, time, widget::text};

/// this trait makes sure downcasting works for the message of the custom status
pub trait StatusMessage: DynClone + Any + Send + Debug {}
clone_trait_object!(StatusMessage);
downcast!(dyn StatusMessage);

/// a status is an icon on the bar which is _permanently_ there and conveys the
/// status of some system function
#[async_trait]
pub trait Status: Send {
    type Message: StatusMessage;

    /// initialize the status by setting up required resources and handlers
    async fn initialize(&mut self);

    /// the iced subscribe method which returns subscriptions
    fn subscribe(&self) -> Subscription<Self::Message>;

    /// the iced update method which mutates the state based on messages
    /// received
    fn update(&mut self, message: &Self::Message);

    /// the iced render method, which renders the status based on internal state
    fn render(&self) -> Element<'_, Self::Message, Theme, Renderer>;
}

/// a trait which removes the implementation specific types and makes the status
/// handling extensible. this trait will down- and upcast all messages to allow
/// handling of statusses in one place. see the `Status` trait for method
/// descriptions
#[async_trait]
pub trait AbstractStatus: Send {
    fn message_type(&self) -> TypeId;

    async fn initialize(&mut self);

    fn subscribe(&self) -> Subscription<Box<dyn StatusMessage>>;

    fn update(&mut self, message: Box<dyn StatusMessage>);

    fn render(&self) -> Element<'_, Box<dyn StatusMessage>, Theme, Renderer>;
}

#[async_trait]
impl<T: Status> AbstractStatus for T {
    fn message_type(&self) -> TypeId {
        TypeId::of::<<T as Status>::Message>()
    }

    async fn initialize(&mut self) {
        Status::initialize(self).await
    }

    fn subscribe(&self) -> Subscription<Box<dyn StatusMessage>> {
        Status::subscribe(self).map(|msg| -> Box<dyn StatusMessage> { Box::new(msg) })
    }

    fn update(&mut self, message: Box<dyn StatusMessage>) {
        let Ok(heap) = message
            .downcast::<<T as Status>::Message>()
            .map_err(|e| panic!("received invalid type for status message: {e:#}"));

        Status::update(self, &heap)
    }

    fn render(&self) -> Element<'_, Box<dyn StatusMessage>, Theme, Renderer> {
        Status::render(self).map(|msg| -> Box<dyn StatusMessage> { Box::new(msg) })
    }
}

#[derive(Clone, Debug)]
pub struct DemoStatusMessage {
    pub message: String,
}
impl StatusMessage for DemoStatusMessage {}

pub struct DemoStatus {}

#[async_trait]
impl Status for DemoStatus {
    type Message = DemoStatusMessage;

    async fn initialize(&mut self) {
        println!("initializing amogus")
    }

    fn subscribe(&self) -> Subscription<Self::Message> {
        time::every(Duration::from_secs(1))
            .map(|_| DemoStatusMessage { message: "hello there".to_owned() })
    }

    fn update(&mut self, message: &Self::Message) {
        println!("received message: {}", message.message)
    }

    fn render(&self) -> Element<'_, Self::Message, Theme, Renderer> {
        let time = Local::now();
        text!("{:0>2}", time.second()).into()
    }
}
