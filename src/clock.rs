use std::time::Duration;

use chrono::{DateTime, Local, Timelike};
use iced::{
    Subscription, Theme, time,
    widget::{column, text},
};

use crate::Message;

pub type ClockMessage = DateTime<Local>;

pub struct Clock {
    time: DateTime<Local>,
}

impl Clock {
    pub fn new() -> Self {
        Self { time: Local::now() }
    }

    pub fn subscribe(&self) -> Subscription<ClockMessage> {
        time::every(Duration::from_secs(1)).map(|_| Local::now())
    }

    pub fn update(&mut self, message: ClockMessage) {
        self.time = message;
    }

    pub fn render(&self) -> iced::Element<'_, Message, Theme, iced::Renderer> {
        column![
            text!("{:0>2}", self.time.hour()),
            text!("{:0>2}", self.time.minute()),
            text!("{:0>2}", self.time.second())
        ]
        .into()
    }
}
