use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use crate::Error;

// Returns Pending as long as its inner bool is false.
#[derive(Default)]
pub struct FrameFuture {
    done: bool,
}

impl Future for FrameFuture {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, _context: &mut Context) -> Poll<Self::Output> {
        if self.done {
            // We were told to step, meaning this future gets destroyed and we run
            // the main future until we call next_frame again and end up in this poll
            // function again.
            Poll::Ready(())
        } else {
            self.done = true;
            Poll::Pending
        }
    }
}

pub struct FileLoadingFuture {
    pub contents: Arc<Mutex<Option<Result<Vec<u8>, Error>>>>,
}

impl Future for FileLoadingFuture {
    type Output = Result<Vec<u8>, Error>;

    fn poll(self: Pin<&mut Self>, _context: &mut Context) -> Poll<Self::Output> {
        let mut contents = self.contents.lock().unwrap();
        if let Some(contents) = contents.take() {
            Poll::Ready(contents)
        } else {
            Poll::Pending
        }
    }
}

fn waker() -> &'static Waker {
    struct FrameWaker;
    impl std::task::Wake for FrameWaker {
        fn wake(self: Arc<Self>) {
            panic!("macroquad does not support waking futures; use coroutines or next_frame");
        }
    }
    static WAKER: std::sync::OnceLock<Waker> = std::sync::OnceLock::new();
    WAKER.get_or_init(|| Waker::from(Arc::new(FrameWaker)))
}

/// returns Some(T) if future is done, None if it would block
pub(crate) fn resume<T>(future: &mut Pin<Box<dyn Future<Output = T>>>) -> Option<T> {
    let waker = waker();
    let mut futures_context = std::task::Context::from_waker(&waker);
    match future.as_mut().poll(&mut futures_context) {
        Poll::Ready(v) => Some(v),
        Poll::Pending => None,
    }
}
