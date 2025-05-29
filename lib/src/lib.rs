use anyhow::Result;
use futures::stream::BoxStream;
use log::warn;

#[cfg(feature = "hyprland")]
pub mod hyprland;
#[cfg(feature = "power")]
pub mod power;
mod util;

/// a boxed stream with a static lifetime
pub type StaticStream<T> = BoxStream<'static, T>;

/// extension trait for anyhow result to log from a stream
pub trait StreamErrorLog<T> {
    fn stream_log(self, name: &str) -> Option<T>;
}

/// log the current error and pretend nothing happened
impl<T> StreamErrorLog<T> for Result<T> {
    fn stream_log(self, name: &str) -> Option<T> {
        match self {
            Ok(r) => Some(r),
            Err(e) => {
                warn!("failure in stream `{name}`: {e:#}");
                None
            }
        }
    }
}
