use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

// ---------------------------------------------------------------------------
// Task ID
// ---------------------------------------------------------------------------

static NEXT_TASK_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// A unique identifier for a scheduled task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TaskId(pub u64);

impl TaskId {
    pub fn new() -> Self {
        TaskId(NEXT_TASK_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }
}

// ---------------------------------------------------------------------------
// Task state
// ---------------------------------------------------------------------------

/// The runnable state of a task: its boxed, pinned future.
type BoxFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// A scheduled async task.
pub struct Task {
    pub id: TaskId,
    future: Mutex<Option<BoxFuture>>,
    /// Queue used to re-schedule this task when it is woken.
    queue: Arc<Mutex<Vec<Arc<Task>>>>,
}

impl Task {
    /// Create a new task from a future.
    pub fn new<F>(future: F, queue: Arc<Mutex<Vec<Arc<Task>>>>) -> Arc<Self>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        Arc::new(Task {
            id: TaskId::new(),
            future: Mutex::new(Some(Box::pin(future))),
            queue,
        })
    }

    /// Poll the task once.  Returns `true` if the task is complete.
    pub fn poll(self: &Arc<Self>) -> bool {
        let waker = task_waker(self.clone());
        let mut cx = Context::from_waker(&waker);

        let mut future_slot = self.future.lock().unwrap();
        if let Some(mut future) = future_slot.take() {
            match future.as_mut().poll(&mut cx) {
                Poll::Ready(()) => {
                    true // task is done
                }
                Poll::Pending => {
                    *future_slot = Some(future); // put it back
                    false
                }
            }
        } else {
            true // already completed
        }
    }

    /// Re-enqueue this task when woken.
    pub fn wake(self: Arc<Self>) {
        let mut q = self.queue.lock().unwrap();
        q.push(self.clone());
    }
}

// ---------------------------------------------------------------------------
// Waker implementation using a raw vtable
// ---------------------------------------------------------------------------

fn clone_waker(data: *const ()) -> RawWaker {
    let arc = unsafe { Arc::from_raw(data as *const Task) };
    let cloned = arc.clone();
    std::mem::forget(arc); // keep original alive
    RawWaker::new(Arc::into_raw(cloned) as *const (), &TASK_WAKER_VTABLE)
}

fn wake_waker(data: *const ()) {
    let arc = unsafe { Arc::from_raw(data as *const Task) };
    arc.wake();
}

fn wake_by_ref_waker(data: *const ()) {
    let arc = unsafe { Arc::from_raw(data as *const Task) };
    let cloned = arc.clone();
    std::mem::forget(arc);
    cloned.wake();
}

fn drop_waker(data: *const ()) {
    unsafe { drop(Arc::from_raw(data as *const Task)) };
}

static TASK_WAKER_VTABLE: RawWakerVTable =
    RawWakerVTable::new(clone_waker, wake_waker, wake_by_ref_waker, drop_waker);

fn task_waker(task: Arc<Task>) -> Waker {
    let raw = RawWaker::new(Arc::into_raw(task) as *const (), &TASK_WAKER_VTABLE);
    unsafe { Waker::from_raw(raw) }
}
