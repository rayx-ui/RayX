//! Work the history starts where nothing awaits it (saving a scope behind an operation its
//! event subscriber recorded): polled at once, then again on whichever thread wakes it, with no
//! executor of its own. A store that answers at once finishes it before `spawn` returns.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::task::Context;

use futures::future::BoxFuture;
use futures::task::{ArcWake, waker_ref};

/// Nobody polls it.
const IDLE: u8 = 0;
/// A thread polls it.
const POLLING: u8 = 1;
/// A thread polls it and it was woken meanwhile: polled again.
const WOKEN: u8 = 2;
/// It finished.
const DONE: u8 = 3;

struct Task {
    state: AtomicU8,
    /// Touched only by the thread that moved `state` to `POLLING`.
    future: Mutex<Option<BoxFuture<'static, ()>>>,
}

impl Task {
    /// Polls the future until it is pending with no wake-up waiting, or done; or, when another
    /// thread polls it, makes that thread poll it again.
    fn run(self: &Arc<Self>) {
        loop {
            match self
                .state
                .compare_exchange(IDLE, POLLING, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => break,
                Err(POLLING) => {
                    if self
                        .state
                        .compare_exchange(POLLING, WOKEN, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
                    {
                        return;
                    }
                }
                Err(_) => return,
            }
        }
        loop {
            let ready = {
                let mut future = self.future.lock().unwrap_or_else(PoisonError::into_inner);
                let Some(running) = future.as_mut() else {
                    self.state.store(DONE, Ordering::Release);
                    return;
                };
                let waker = waker_ref(self);
                let ready = running
                    .as_mut()
                    .poll(&mut Context::from_waker(&waker))
                    .is_ready();
                if ready {
                    *future = None;
                }
                ready
            };
            if ready {
                self.state.store(DONE, Ordering::Release);
                return;
            }
            match self
                .state
                .compare_exchange(POLLING, IDLE, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => return,
                // Woken while it was polled: polled again.
                Err(_) => self.state.store(POLLING, Ordering::Release),
            }
        }
    }
}

impl ArcWake for Task {
    fn wake_by_ref(task: &Arc<Self>) {
        task.run();
    }
}

/// Runs `future` to its end, here until it waits and then on the thread that wakes it.
pub(crate) fn spawn(future: BoxFuture<'static, ()>) {
    let task = Arc::new(Task {
        state: AtomicU8::new(IDLE),
        future: Mutex::new(Some(future)),
    });
    task.run();
}
