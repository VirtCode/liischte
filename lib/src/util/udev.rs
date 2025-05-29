// blatantly stolen from https://github.com/jeandudey/tokio-udev/blob/master/tokio-udev/src/lib.rs but I need it to be send

use futures_core::stream::Stream;
use std::{io, pin::Pin, sync::Mutex, task::Poll};
use tokio::io::unix::AsyncFd;
use udev::{Event, MonitorSocket};

/// Asynchronous stream of device events.
pub struct AsyncMonitorSocket {
    inner: Mutex<Inner>,
}

impl AsyncMonitorSocket {
    /// Construct a tokio-udev [`AsyncMonitorSocket`] from an existing one.
    pub fn new(monitor: MonitorSocket) -> io::Result<AsyncMonitorSocket> {
        Ok(AsyncMonitorSocket { inner: Mutex::new(Inner::new(monitor)?) })
    }
}

impl Stream for AsyncMonitorSocket {
    type Item = Result<udev::Event, io::Error>;

    fn poll_next(self: Pin<&mut Self>, ctx: &mut std::task::Context) -> Poll<Option<Self::Item>> {
        self.inner.lock().unwrap().poll_receive(ctx)
    }
}

struct Inner {
    fd: AsyncFd<MonitorSocket>,
}

impl Inner {
    fn new(monitor: MonitorSocket) -> io::Result<Inner> {
        Ok(Inner { fd: AsyncFd::new(monitor)? })
    }

    fn poll_receive(
        &mut self,
        ctx: &mut std::task::Context,
    ) -> Poll<Option<Result<Event, io::Error>>> {
        loop {
            if let Some(e) = self.fd.get_mut().iter().next() {
                return Poll::Ready(Some(Ok(e)));
            }
            match self.fd.poll_read_ready(ctx) {
                Poll::Ready(Ok(mut ready_guard)) => {
                    ready_guard.clear_ready();
                }
                Poll::Ready(Err(err)) => return Poll::Ready(Some(Err(err))),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}
