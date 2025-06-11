use crate::{config::CONFIG, ui::PILL_RADIUS};
use iced::{
    Background, Border, Color, Element, Length, Rectangle, Size,
    core::{
        self, Layout, Widget,
        layout::{self, Node},
        mouse, renderer,
        widget::Tree,
    },
};

/// creates a vertical progress bar, takes a value between 0 and 1
pub fn vertical_progress(value: f32, height: f32, inner: f32, outer: f32) -> VerticalProgress {
    VerticalProgress {
        value,
        height,
        width_inner: inner,
        width_outer: outer,
        color_inner: CONFIG.looks.semi,
        color_outer: CONFIG.looks.foreground,
    }
}

pub struct VerticalProgress {
    value: f32,

    height: f32,
    width_outer: f32,
    width_inner: f32,

    color_inner: Color,
    color_outer: Color,
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for VerticalProgress
where
    Message: Clone,
    Renderer: core::Renderer,
{
    fn size(&self) -> Size<Length> {
        Size { width: Length::Shrink, height: self.height.into() }
    }

    fn layout(&self, _tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) -> Node {
        layout::atomic(limits, self.width_outer, self.height)
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let rounding = Border { color: Color::TRANSPARENT, width: 0.0, radius: PILL_RADIUS };

        let offset = (1.0 - self.value).clamp(0.0, 1.0);
        let x = bounds.x + bounds.width / 2.0;

        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle {
                    x: x - self.width_inner / 2.0,
                    y: bounds.y + (self.width_outer - self.width_inner) / 2.0,
                    width: self.width_inner,
                    height: bounds.height - (self.width_outer - self.width_inner),
                },
                border: rounding,
                ..renderer::Quad::default()
            },
            Background::Color(self.color_inner),
        );

        // rendering a quad with height 0 crashes tiny-skia
        if offset == 1f32 {
            return;
        };

        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle {
                    x: x - self.width_outer / 2.0,
                    y: bounds.y + bounds.height * offset,
                    width: self.width_outer,
                    height: bounds.height - bounds.height * offset,
                },
                border: rounding,
                ..renderer::Quad::default()
            },
            Background::Color(self.color_outer),
        );
    }
}

impl<'a, Message, Theme, Renderer> From<VerticalProgress> for Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Renderer: core::Renderer + 'a,
{
    fn from(progress: VerticalProgress) -> Element<'a, Message, Theme, Renderer> {
        Element::new(progress)
    }
}
