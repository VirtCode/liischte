use iced::{
    Font, Radius, color,
    widget::{Column, Rule, Space, Text, column, horizontal_rule, rule, text},
};
use lucide_icons::Icon;

use crate::config::CONFIG;

pub mod progress;
pub mod window;

/// radius to use to create a pill shape
pub const PILL_RADIUS: Radius = Radius {
    top_left: f32::MAX,
    top_right: f32::MAX,
    bottom_right: f32::MAX,
    bottom_left: f32::MAX,
};

/// creates a separator for the bar
pub fn separator<'a>() -> Rule<'a> {
    horizontal_rule(2).style(|_| rule::Style {
        color: CONFIG.looks.semi,
        width: 2,
        fill_mode: rule::FillMode::Full,
        radius: Radius::new(2),
    })
}

/// creates an icon with the lucide icon font
pub fn icon<'a>(icon: Icon) -> Text<'a> {
    text(icon.unicode()).font(Font::with_name("lucide")).size(24)
}

/// creates an empty widget
pub fn empty() -> Space {
    Space::new(0, 0)
}
