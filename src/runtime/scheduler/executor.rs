use super::task::Task;
use std::future::Future;
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// Single-threaded Executor
// ---------------------------------------------------------------------------

/// A cooperative, single-threaded async executor.
///
/// Drives futures to completion on the calling thread.  Suitable for the
/// Aura REPL, interpreter mode, and unit tests where threading is not needed.
///
/// # Example
/// ```rust,ignore
/// let mut exec = Executor::new();
/// exec.spawn(async { println!("hello from Aura!") });
/// exec.run();
/// ```
pub struct Executor {
    /// Ready queue — tasks that have been woken and are ready to be polled.
    queue: Arc<Mutex<Vec<Arc<Task>>>>,
}

impl Executor {
    /// Create a new executor with an empty queue.
    pub fn new() -> Self {
        Self {
            queue: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Spawn a future as a task on this executor.
    pub fn spawn<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let task = Task::new(future, self.queue.clone());
        self.queue.lock().unwrap().push(task);
    }

    /// Run all tasks to completion (run-to-completion loop).
    ///
    /// A task that returns `Poll::Pending` will be re-queued when its
    /// waker fires.  This loop exits only when all tasks have completed.
    pub fn run(&self) {
        loop {
            // Drain the queue snapshot — new tasks may be added during polling
            let batch: Vec<Arc<Task>> = {
                let mut q = self.queue.lock().unwrap();
                std::mem::take(&mut *q)
            };

            if batch.is_empty() {
                break;
            }

            for task in batch {
                task.poll(); // if not done, the waker re-queues it
            }
        }
    }

    /// Convenience: spawn a future, run to completion, and return.
    pub fn block_on<F: Future<Output = ()> + Send + 'static>(&self, future: F) {
        self.spawn(future);
        self.run();
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::scheduler::promise::Promise;
    use std::sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    };

    #[test]
    fn executor_runs_simple_task() {
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();
        let exec = Executor::new();
        exec.spawn(async move {
            c.fetch_add(1, Ordering::SeqCst);
        });
        exec.run();
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn executor_runs_multiple_tasks() {
        let counter = Arc::new(AtomicU32::new(0));
        let exec = Executor::new();
        for _ in 0..5 {
            let c = counter.clone();
            exec.spawn(async move {
                c.fetch_add(1, Ordering::SeqCst);
            });
        }
        exec.run();
        assert_eq!(counter.load(Ordering::SeqCst), 5);
    }

    #[test]
    fn executor_resolves_promise() {
        let exec = Executor::new();
        let result = Arc::new(AtomicU32::new(0));
        let r = result.clone();

        exec.spawn(async move {
            let promise = Promise::resolved(42u32);
            let val = promise.await;
            r.store(val, Ordering::SeqCst);
        });
        exec.run();

        assert_eq!(result.load(Ordering::SeqCst), 42);
    }

    #[test]
    fn executor_block_on() {
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();
        let exec = Executor::new();
        exec.block_on(async move {
            c.fetch_add(10, Ordering::SeqCst);
        });
        assert_eq!(counter.load(Ordering::SeqCst), 10);
    }
}
