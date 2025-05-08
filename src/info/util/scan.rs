// also blatantly stolen from https://github.com/rust-lang/futures-rs/blob/master/futures-util/src/stream/stream/scan.rs
// this is not yet released which is why we need to implement it separatly
// see also https://github.com/rust-lang/futures-rs/issues/2171

use core::fmt;
use core::pin::Pin;
use futures_core::future::Future;
use futures_core::ready;
use futures_core::stream::Stream;
use futures_core::task::{Context, Poll};
use pin_project_lite::pin_project;

pin_project! {
    /// UnfoldState used for stream and sink unfolds
    #[project = UnfoldStateProj]
    #[project_replace = UnfoldStateProjReplace]
    #[derive(Debug)]
    pub(crate) enum UnfoldState<T, Fut> {
        Value {
            value: T,
        },
        Future {
            #[pin]
            future: Fut,
        },
        Empty,
    }
}

impl<T, Fut> UnfoldState<T, Fut> {
    pub(crate) fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    pub(crate) fn is_future(&self) -> bool {
        matches!(self, Self::Future { .. })
    }

    pub(crate) fn project_future(self: Pin<&mut Self>) -> Option<Pin<&mut Fut>> {
        match self.project() {
            UnfoldStateProj::Future { future } => Some(future),
            _ => None,
        }
    }

    pub(crate) fn take_value(self: Pin<&mut Self>) -> Option<T> {
        match &*self {
            Self::Value { .. } => match self.project_replace(Self::Empty) {
                UnfoldStateProjReplace::Value { value } => Some(value),
                _ => unreachable!(),
            },
            _ => None,
        }
    }
}

pin_project! {
    /// Stream for the [`scan`](super::StreamExt::scan) method.
    #[must_use = "streams do nothing unless polled"]
    pub struct ScanOwning<St: Stream, S, Fut, F> {
        #[pin]
        stream: St,
        f: F,
        #[pin]
        state: UnfoldState<S, Fut>,
    }
}

impl<St, S, Fut, F> fmt::Debug for ScanOwning<St, S, Fut, F>
where
    St: Stream + fmt::Debug,
    St::Item: fmt::Debug,
    S: fmt::Debug,
    Fut: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Scan")
            .field("stream", &self.stream)
            .field("state", &self.state)
            .field("done_taking", &self.is_done_taking())
            .finish()
    }
}

impl<St: Stream, S, Fut, F> ScanOwning<St, S, Fut, F> {
    /// Checks if internal state is `None`.
    fn is_done_taking(&self) -> bool {
        self.state.is_empty()
    }
}

impl<B, St, S, Fut, F> ScanOwning<St, S, Fut, F>
where
    St: Stream,
    F: FnMut(S, St::Item) -> Fut,
    Fut: Future<Output = Option<(S, B)>>,
{
    pub(super) fn new(stream: St, initial_state: S, f: F) -> Self {
        Self { stream, f, state: UnfoldState::Value { value: initial_state } }
    }
}

impl<B, St, S, Fut, F> Stream for ScanOwning<St, S, Fut, F>
where
    St: Stream,
    F: FnMut(S, St::Item) -> Fut,
    Fut: Future<Output = Option<(S, B)>>,
{
    type Item = B;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<B>> {
        if self.is_done_taking() {
            return Poll::Ready(None);
        }

        let mut this = self.project();

        Poll::Ready(loop {
            if let Some(fut) = this.state.as_mut().project_future() {
                match ready!(fut.poll(cx)) {
                    None => {
                        this.state.set(UnfoldState::Empty);
                        break None;
                    }
                    Some((state, item)) => {
                        this.state.set(UnfoldState::Value { value: state });
                        break Some(item);
                    }
                }
            } else if let Some(item) = ready!(this.stream.as_mut().poll_next(cx)) {
                let state = this.state.as_mut().take_value().unwrap();
                this.state.set(UnfoldState::Future { future: (this.f)(state, item) })
            } else {
                break None;
            }
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.is_done_taking() {
            (0, Some(0))
        } else {
            self.stream.size_hint() // can't know a lower bound, due to the predicate
        }
    }
}
