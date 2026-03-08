use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

// ---------------------------------------------------------------------------
// Promise — the user-facing async value
// ---------------------------------------------------------------------------

/// The state shared between a `Promise` and its `Resolver`.
#[derive(Debug)]
struct PromiseState<T> {
    value: Option<T>,
    waker: Option<std::task::Waker>,
}

/// A future that resolves to `T` once a `Resolver` provides the value.
///
/// This is the Aura-level primitive for async computation:
///
/// ```rust,ignore
/// let (promise, resolver) = Promise::new();
/// std::thread::spawn(move || {
///     resolver.resolve(42);
/// });
/// let result = executor.block_on(promise); // 42
/// ```
pub struct Promise<T> {
    state: Arc<Mutex<PromiseState<T>>>,
}

/// The send-half that resolves the promise.
pub struct Resolver<T> {
    state: Arc<Mutex<PromiseState<T>>>,
}

impl<T: Send + 'static> Promise<T> {
    /// Create a `(Promise, Resolver)` pair.
    pub fn new() -> (Self, Resolver<T>) {
        let state = Arc::new(Mutex::new(PromiseState {
            value: None,
            waker: None,
        }));
        (
            Promise {
                state: state.clone(),
            },
            Resolver { state },
        )
    }

    /// Create an already-resolved promise (useful for tests).
    pub fn resolved(value: T) -> Self {
        Promise {
            state: Arc::new(Mutex::new(PromiseState {
                value: Some(value),
                waker: None,
            })),
        }
    }
}

impl<T: Send + 'static> Resolver<T> {
    /// Provide the value to the waiting promise.
    pub fn resolve(self, value: T) {
        let mut state = self.state.lock().unwrap();
        state.value = Some(value);
        if let Some(waker) = state.waker.take() {
            waker.wake();
        }
    }
}

impl<T: Send + 'static> Future for Promise<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.state.lock().unwrap();
        if let Some(value) = state.value.take() {
            Poll::Ready(value)
        } else {
            state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}
