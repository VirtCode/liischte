use iced_winit::commands::subsurface::Layer;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Deserialize, Serialize, Debug, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum WindowLayer {
    Overlay,
    Top,
    Bottom,
    Background,
}

impl From<WindowLayer> for Layer {
    fn from(val: WindowLayer) -> Self {
        match val {
            WindowLayer::Overlay => Layer::Overlay,
            WindowLayer::Top => Layer::Top,
            WindowLayer::Bottom => Layer::Bottom,
            WindowLayer::Background => Layer::Background,
        }
    }
}
