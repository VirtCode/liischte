use futures::io::Error;
use iced::Executor;
use log::trace;
use tokio::runtime::Handle;

/// this is an iced runtime which uses the currently running tokio runtime. this
/// will only work if the iced application is spawned in a thread that already
/// belongs to a tokio runtime
pub struct ExistingRuntime {
    handle: Handle,
}

impl Executor for ExistingRuntime {
    fn new() -> Result<Self, Error>
    where
        Self: Sized,
    {
        trace!("using existing tokio runtime for iced executor");
        Ok(Self { handle: Handle::try_current().map_err(Error::other)? })
    }

    fn spawn(&self, future: impl Future<Output = ()> + iced_winit::futures::MaybeSend + 'static) {
        _ = self.handle.spawn(future);
    }

    fn enter<R>(&self, f: impl FnOnce() -> R) -> R {
        // we don't do something special here cause everything should already be running
        // in this runtime
        f()
    }
}
