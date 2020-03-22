//! Futures and other types that allow asynchronous interaction with channels.

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use crate::*;
use futures::{Stream, stream::FusedStream, future::FusedFuture};

impl<T> Receiver<T> {
    #[inline]
    fn poll(&self, cx: &mut Context<'_>) -> Poll<Result<T, RecvError>> {
        let mut buf = self.buffer.borrow_mut();

        let res = if let Some(msg) = buf.pop_front() {
            return Poll::Ready(Ok(msg));
        } else {
            self
                .shared
                .poll_inner()
                .map(|inner| self
                    .shared
                    .try_recv(
                        move || inner,
                        &mut buf,
                        &self.mode,
                    )
                )
        };

        let poll = match res {
            Some(Ok(msg)) => Poll::Ready(Ok(msg)),
            Some(Err((_, TryRecvError::Disconnected))) => Poll::Ready(Err(RecvError::Disconnected)),
            Some(Err((mut inner, TryRecvError::Empty))) => {
                // Inform the sender that we need waking
                inner.recv_wakers.push_back(cx.waker().clone());
                Poll::Pending
            },
            // Can't access the inner lock, try again
            None => {
                cx.waker().wake_by_ref();
                Poll::Pending
            },
        };

        poll
    }
}

/// A future  used to receive a value from the channel.
pub struct RecvFuture<'a, T> {
    recv: &'a mut Receiver<T>,
}

impl<'a, T> RecvFuture<'a, T> {
    pub(crate) fn new(recv: &mut Receiver<T>) -> RecvFuture<T> {
        RecvFuture { recv }
    }
}

impl<'a, T> Future for RecvFuture<'a, T> {
    type Output = Result<T, RecvError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.recv.poll(cx)
    }
}

impl<'a, T> FusedFuture for RecvFuture<'a, T> {
    fn is_terminated(&self) -> bool {
        self.recv.mode.get() == Mode::Finished
    }
}

impl<T> Stream for Receiver<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.poll(cx).map(|ready| ready.ok())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
	    (self.buffer.borrow().len(), None)
    }
}

impl<T> FusedStream for Receiver<T> {
    fn is_terminated(&self) -> bool {
        self.mode.get() == Mode::Finished
    }
}
