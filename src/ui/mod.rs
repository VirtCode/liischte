use iced::{
    Color, Font, Radius,
    widget::{Rule, Space, Text, horizontal_rule, rule, text},
};
use lucide_icons::Icon;

use crate::config::CONFIG;

pub mod outputs;
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
pub fn separator<'a>(visible: bool) -> Rule<'a> {
    horizontal_rule(2)
        .style(move |_| rule::Style {
            color: if visible { CONFIG.looks.semi } else { Color::TRANSPARENT },
            width: 2,
            fill_mode: rule::FillMode::Full,
            radius: Radius::new(2),
        })
        .width(32)
}

/// creates an icon with the lucide icon font
pub fn icon<'a>(icon: Icon) -> Text<'a> {
    text(icon.unicode()).font(Font::with_name("lucide")).size(24)
}

/// creates an icon with the lucide icon font (from a char)
/// TODO: remove me once Icon is copy
pub fn icon_char<'a>(icon: char) -> Text<'a> {
    text(icon).font(Font::with_name("lucide")).size(24)
}

/// creates an empty widget
pub fn empty() -> Space {
    Space::new(0, 0)
}
