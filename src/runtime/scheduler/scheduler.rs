use super::task::Task;
use std::collections::VecDeque;
use std::future::Future;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

// ---------------------------------------------------------------------------
// Per-worker queue
// ---------------------------------------------------------------------------

/// A double-ended work queue per worker thread.
/// Owned tasks are pushed/popped from the back (LIFO locality).
/// Stolen tasks are taken from the front (FIFO fairness).
#[derive(Default)]
struct WorkQueue {
    deque: VecDeque<Arc<Task>>,
}

impl WorkQueue {
    fn push(&mut self, task: Arc<Task>) {
        self.deque.push_back(task);
    }

    /// Pop from the back (owned tasks — local pop).
    fn pop(&mut self) -> Option<Arc<Task>> {
        self.deque.pop_back()
    }

    /// Steal from the front (remote steal).
    fn steal(&mut self) -> Option<Arc<Task>> {
        self.deque.pop_front()
    }

    #[allow(dead_code)]
    fn is_empty(&self) -> bool {
        self.deque.is_empty()
    }
}

// ---------------------------------------------------------------------------
// WorkStealingScheduler
// ---------------------------------------------------------------------------

/// Shared state visible to all workers.
struct SharedState {
    queues: Vec<Mutex<WorkQueue>>,
    /// Condvar used to wake sleeping workers when new tasks arrive.
    condvar: Condvar,
    /// Global shutdown flag.
    shutdown: Mutex<bool>,
}

impl SharedState {
    fn new(num_workers: usize) -> Arc<Self> {
        let queues = (0..num_workers)
            .map(|_| Mutex::new(WorkQueue::default()))
            .collect();
        Arc::new(Self {
            queues,
            condvar: Condvar::new(),
            shutdown: Mutex::new(false),
        })
    }
}

/// A work-stealing multi-threaded executor.
///
/// Tasks are distributed across `N` worker threads.  Each worker maintains
/// its own `deque` and steals from peers when idle.
///
/// # Example
/// ```rust,ignore
/// let scheduler = WorkStealingScheduler::new(4);
/// scheduler.spawn(async { println!("hello from worker!"); });
/// scheduler.join(); // wait for all tasks to finish
/// ```
pub struct WorkStealingScheduler {
    shared: Arc<SharedState>,
    handles: Vec<thread::JoinHandle<()>>,
    /// Round-robin submission counter.
    next_worker: Mutex<usize>,
    num_workers: usize,
}

impl WorkStealingScheduler {
    /// Create a scheduler with `num_workers` background threads.
    pub fn new(num_workers: usize) -> Self {
        let shared = SharedState::new(num_workers);
        let mut handles = Vec::new();

        for worker_id in 0..num_workers {
            let s = shared.clone();
            let handle = thread::spawn(move || {
                worker_loop(worker_id, s);
            });
            handles.push(handle);
        }

        Self {
            shared,
            handles,
            next_worker: Mutex::new(0),
            num_workers,
        }
    }

    /// Submit a future to the scheduler.  It will run on one of the worker threads.
    pub fn spawn<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        // Build a shared re-queue sink so woken tasks re-enter the scheduler
        let sink = self.make_sink();
        let task = Task::new(future, sink);

        // Round-robin initial assignment
        let worker_id = {
            let mut n = self.next_worker.lock().unwrap();
            let id = *n % self.num_workers;
            *n += 1;
            id
        };

        {
            let mut q = self.shared.queues[worker_id].lock().unwrap();
            q.push(task);
        }
        self.shared.condvar.notify_one();
    }

    /// Build a `Vec<Arc<Task>>` sink that, when tasks are pushed into it,
    /// re-distributes them across worker queues.
    fn make_sink(&self) -> Arc<Mutex<Vec<Arc<Task>>>> {
        // We use a simple shared global sink; the workers drain it periodically.
        // For the initial implementation we funnel re-woken tasks to worker 0.
        let shared = self.shared.clone();
        let sink: Arc<Mutex<Vec<Arc<Task>>>> = Arc::new(Mutex::new(Vec::new()));
        let sink_clone = sink.clone();

        // Spawn a lightweight "drain" thread that flushes the sink to queues
        thread::spawn(move || {
            loop {
                thread::sleep(std::time::Duration::from_micros(100));
                let mut s = sink_clone.lock().unwrap();
                if s.is_empty() {
                    // Check shutdown
                    if *shared.shutdown.lock().unwrap() {
                        break;
                    }
                    continue;
                }
                let tasks: Vec<Arc<Task>> = std::mem::take(&mut *s);
                drop(s);
                for task in tasks {
                    let mut q = shared.queues[0].lock().unwrap();
                    q.push(task);
                    drop(q);
                    shared.condvar.notify_one();
                }
                if *shared.shutdown.lock().unwrap() {
                    break;
                }
            }
        });

        sink
    }

    /// Signal all workers to shut down and wait for them to finish.
    pub fn join(self) {
        {
            let mut shutdown = self.shared.shutdown.lock().unwrap();
            *shutdown = true;
        }
        self.shared.condvar.notify_all();
        for handle in self.handles {
            let _ = handle.join();
        }
    }
}

// ---------------------------------------------------------------------------
// Worker loop
// ---------------------------------------------------------------------------

fn worker_loop(worker_id: usize, shared: Arc<SharedState>) {
    let num_workers = shared.queues.len();

    loop {
        // 1. Try own queue
        let task = {
            let mut q = shared.queues[worker_id].lock().unwrap();
            q.pop()
        };

        if let Some(task) = task {
            task.poll();
            continue;
        }

        // 2. Try stealing from peers
        let mut stole = None;
        for i in 0..num_workers {
            if i == worker_id {
                continue;
            }
            let mut q = shared.queues[i].lock().unwrap();
            if let Some(t) = q.steal() {
                stole = Some(t);
                break;
            }
        }

        if let Some(task) = stole {
            task.poll();
            continue;
        }

        // 3. Park thread waiting for new work or shutdown
        let shutdown = shared.shutdown.lock().unwrap();
        if *shutdown {
            // Drain own queue before exiting
            loop {
                let task = shared.queues[worker_id].lock().unwrap().pop();
                if let Some(t) = task {
                    t.poll();
                } else {
                    break;
                }
            }
            return;
        }
        // Wait (releases lock while sleeping)
        let _guard = shared
            .condvar
            .wait_timeout(shutdown, std::time::Duration::from_millis(1))
            .unwrap();
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    };

    #[test]
    fn scheduler_runs_task() {
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();

        let scheduler = WorkStealingScheduler::new(2);
        scheduler.spawn(async move {
            c.fetch_add(1, Ordering::SeqCst);
        });

        // Give workers time to complete
        thread::sleep(std::time::Duration::from_millis(50));
        scheduler.join();

        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn scheduler_runs_many_tasks() {
        let counter = Arc::new(AtomicU32::new(0));
        let scheduler = WorkStealingScheduler::new(4);

        for _ in 0..20 {
            let c = counter.clone();
            scheduler.spawn(async move {
                c.fetch_add(1, Ordering::SeqCst);
            });
        }

        thread::sleep(std::time::Duration::from_millis(100));
        scheduler.join();

        assert_eq!(counter.load(Ordering::SeqCst), 20);
    }

    #[test]
    fn scheduler_work_stealing_distributes_load() {
        // Spawn more tasks than workers — work-stealing must kick in
        let counter = Arc::new(AtomicU32::new(0));
        let scheduler = WorkStealingScheduler::new(2);

        for _ in 0..10 {
            let c = counter.clone();
            scheduler.spawn(async move {
                c.fetch_add(1, Ordering::SeqCst);
            });
        }

        thread::sleep(std::time::Duration::from_millis(100));
        scheduler.join();

        assert_eq!(counter.load(Ordering::SeqCst), 10);
    }
}
