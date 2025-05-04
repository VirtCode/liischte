use iced::{
    Font, Radius, color,
    widget::{Rule, Text, horizontal_rule, rule, text},
};
use lucide_icons::Icon;

pub mod window;

/// creates a separator for the bar
pub fn separator<'a>() -> Rule<'a> {
    horizontal_rule(2).style(|_| rule::Style {
        color: color!(0xcdd5ff, 0.5),
        width: 2,
        fill_mode: rule::FillMode::Full,
        radius: Radius::new(2),
    })
}

/// creates an icon with the lucide icon font
pub fn icon<'a>(icon: Icon) -> Text<'a> {
    text(icon.unicode()).font(Font::with_name("lucide")).size(24)
}
