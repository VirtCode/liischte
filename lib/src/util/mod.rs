use futures::Stream;
use scan::ScanOwning;

pub mod scan;
#[cfg(feature = "power")]
pub mod udev;

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
