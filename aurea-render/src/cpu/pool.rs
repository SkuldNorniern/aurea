//! The threads row bands draw on, kept between frames: starting them per frame costs more than
//! drawing a simple frame. One set for the process, so more windows don't mean more threads.

use std::panic::{self, AssertUnwindSafe};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, TryLockError};
use std::thread::{self, JoinHandle};

/// One task, with its type erased so a worker can hold it.
struct Job {
    data: *mut (),
    run: unsafe fn(*mut ()),
}

// SAFETY: a job only comes from `run`'s tasks, which are `Send`, and `run` waits for it to end.
unsafe impl Send for Job {}

impl Job {
    fn of<F: FnMut() + Send>(task: &mut F) -> Self {
        /// # Safety
        /// `data` is a live `F` nothing else uses.
        unsafe fn call<F: FnMut()>(data: *mut ()) {
            unsafe { (*data.cast::<F>())() }
        }
        Self {
            data: ptr::from_mut(task).cast(),
            run: call::<F>,
        }
    }
}

#[derive(Default)]
struct Inbox {
    job: Option<Job>,
    stop: bool,
}

#[derive(Default)]
struct Slot {
    inbox: Mutex<Inbox>,
    wake: Condvar,
}

struct Worker {
    slot: Arc<Slot>,
    thread: JoinHandle<()>,
}

/// How many jobs handed out have not ended, and whether one panicked.
struct Finish {
    left: Mutex<usize>,
    done: Condvar,
    panicked: AtomicBool,
}

static WORKERS: Mutex<Vec<Worker>> = Mutex::new(Vec::new());
static FINISH: Finish = Finish {
    left: Mutex::new(0),
    done: Condvar::new(),
    panicked: AtomicBool::new(false),
};

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn work(slot: &Slot) {
    loop {
        let job = {
            let mut inbox = lock(&slot.inbox);
            loop {
                if let Some(job) = inbox.job.take() {
                    break job;
                }
                if inbox.stop {
                    return;
                }
                inbox = slot
                    .wake
                    .wait(inbox)
                    .unwrap_or_else(PoisonError::into_inner);
            }
        };
        // SAFETY: `run` keeps the task alive until `left` says this job ended.
        let ran = panic::catch_unwind(AssertUnwindSafe(|| unsafe { (job.run)(job.data) }));
        if ran.is_err() {
            FINISH.panicked.store(true, Ordering::Relaxed);
        }
        let mut left = lock(&FINISH.left);
        *left -= 1;
        if *left == 0 {
            FINISH.done.notify_one();
        }
    }
}

/// Waits for the handed out jobs when dropped, so a panic here can't free what they borrow.
struct Wait;

impl Drop for Wait {
    fn drop(&mut self) {
        let mut left = lock(&FINISH.left);
        while *left > 0 {
            left = FINISH
                .done
                .wait(left)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

/// Starts workers until there are `count`, or the system refuses more.
fn grow(workers: &mut Vec<Worker>, count: usize) {
    while workers.len() < count {
        let slot = Arc::new(Slot::default());
        let theirs = Arc::clone(&slot);
        let started = thread::Builder::new()
            .name("aurea-raster".into())
            .spawn(move || work(&theirs));
        match started {
            Ok(thread) => workers.push(Worker { slot, thread }),
            Err(error) => {
                log::warn!("raster thread: {error}");
                return;
            }
        }
    }
}

/// Runs every task, the first on this thread, and returns false if a worker's panicked. Tasks run
/// here one by one while another frame has the workers, or when no thread can start.
pub(super) fn run<F: FnMut() + Send>(tasks: &mut [F]) -> bool {
    let Some((own, rest)) = tasks.split_first_mut() else {
        return true;
    };
    let mut workers = match WORKERS.try_lock() {
        Ok(workers) => workers,
        // A panic on a caller's thread: `Wait` saw its workers finish, so they are fine.
        Err(TryLockError::Poisoned(poisoned)) => {
            WORKERS.clear_poison();
            poisoned.into_inner()
        }
        Err(TryLockError::WouldBlock) => {
            own();
            rest.iter_mut().for_each(|task| task());
            return true;
        }
    };
    grow(&mut workers, rest.len());
    // A frame that panicked here never read this.
    FINISH.panicked.store(false, Ordering::Relaxed);
    let (given, left_over) = rest.split_at_mut(workers.len().min(rest.len()));
    *lock(&FINISH.left) = given.len();
    let wait = Wait;
    for (task, worker) in given.iter_mut().zip(workers.iter()) {
        lock(&worker.slot.inbox).job = Some(Job::of(task));
        worker.slot.wake.notify_one();
    }
    own();
    left_over.iter_mut().for_each(|task| task());
    drop(wait);
    !FINISH.panicked.swap(false, Ordering::Relaxed)
}

/// Ends the workers and what their threads hold. The next frame starts them again.
pub(super) fn release() {
    let mut workers = lock(&WORKERS);
    for worker in workers.drain(..) {
        lock(&worker.slot.inbox).stop = true;
        worker.slot.wake.notify_one();
        let _ = worker.thread.join();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Whether a run gets workers: other tests can hold them, so it is tried a few times.
    fn reaches_a_worker() -> bool {
        (0..200).any(|_| {
            let here = thread::current().id();
            let mut ids = [here; 3];
            let mut tasks: Vec<_> = ids
                .iter_mut()
                .map(|id| move || *id = thread::current().id())
                .collect();
            assert!(run(&mut tasks));
            drop(tasks);
            thread::sleep(Duration::from_millis(1));
            ids.iter().any(|id| *id != here)
        })
    }

    #[test]
    fn a_panic_on_the_callers_thread_leaves_the_workers_usable() {
        let unwound = panic::catch_unwind(|| {
            let mut tasks = [|| panic!("the caller's task"), || {}];
            run(&mut tasks);
        });
        assert!(unwound.is_err());
        assert!(reaches_a_worker(), "workers are still used after a panic");
    }

    #[test]
    fn a_workers_panic_is_reported_once() {
        let mut reported = false;
        for _ in 0..200 {
            let mut tasks = [false, true].map(|fails| {
                move || {
                    assert!(!fails, "a worker's task");
                }
            });
            // With the workers busy elsewhere it runs here, and simply unwinds.
            match panic::catch_unwind(AssertUnwindSafe(|| run(&mut tasks))) {
                Ok(finished) => {
                    assert!(!finished, "the failed task is reported");
                    reported = true;
                    break;
                }
                Err(_) => thread::sleep(Duration::from_millis(1)),
            }
        }
        assert!(reported);
        let mut next = [|| {}, || {}];
        assert!(run(&mut next), "the next frame starts clean");
    }

    #[test]
    fn every_task_runs_once_and_workers_come_back_after_a_release() {
        for _ in 0..50 {
            let mut hits = [0_u32; 6];
            let mut tasks: Vec<_> = hits.iter_mut().map(|hit| || *hit += 1).collect();
            assert!(run(&mut tasks));
            drop(tasks);
            assert_eq!(hits, [1; 6]);
        }
        release();
        let mut hits = [0_u32; 3];
        let mut tasks: Vec<_> = hits.iter_mut().map(|hit| || *hit += 1).collect();
        assert!(run(&mut tasks));
        drop(tasks);
        assert_eq!(hits, [1; 3]);
    }
}
