use anyhow::Result;
use futures::{Stream, stream::BoxStream};
use log::warn;
use scan::ScanOwning;

pub mod scan;
pub mod udev;

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

impl<T: ?Sized> StreamCustomExt for T where T: Stream {}

pub trait StreamCustomExt: Stream {
    fn scan_owning<S, B, Fut, F>(self, initial_state: S, f: F) -> ScanOwning<Self, S, Fut, F>
    where
        F: FnMut(S, Self::Item) -> Fut,
        Fut: Future<Output = Option<(S, B)>>,
        Self: Sized,
    {
        ScanOwning::new(self, initial_state, f)
    }
}
