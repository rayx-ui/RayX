//! Services whose values are neither `Send` nor `Sync`, kept on the thread that made them.
//!
//! The provider only holds a [`ThreadBoundSlot`] (an id, the owning thread's id and the service's
//! name), which is `Send + Sync`; the value itself lives in a `thread_local!` table of the thread
//! that made it. Resolving on that thread reads the table; anywhere else it fails with
//! [`ServiceResolutionError::WrongThread`] without touching the value. A slot dropped on its own
//! thread removes the value; one dropped elsewhere queues its id on the owning thread's orphan
//! list, drained on that thread's next table access, that is its next thread-bound resolve (or by
//! [`release_orphaned_thread_bound_services`]), and a thread's table drops with the thread. So a
//! value is never dropped off its thread, and no `unsafe` is involved.

use super::registry::ServiceResolutionError;
use std::any::Any;
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, ThreadId};

/// A thread-bound service's value with its type erased: what a keyed thread-bound factory makes
/// and what [`ServiceProvider::thread_bound_service_by_key`](super::ServiceProvider::thread_bound_service_by_key)
/// returns.
pub type LocalService = Rc<dyn Any>;

type Orphans = Arc<Mutex<Vec<u64>>>;

struct LocalTable {
    values: HashMap<u64, LocalService>,
    orphans: Orphans,
}

impl LocalTable {
    /// Takes the values whose slots were dropped on other threads, for the caller to drop once
    /// the table is no longer borrowed (a value's drop may reach the table again).
    fn take_orphans(&mut self) -> Vec<LocalService> {
        let ids = self
            .orphans
            .lock()
            .map(|mut ids| std::mem::take(&mut *ids))
            .unwrap_or_default();
        ids.into_iter()
            .filter_map(|id| self.values.remove(&id))
            .collect()
    }
}

thread_local! {
    static TABLE: RefCell<LocalTable> = RefCell::new(LocalTable {
        values: HashMap::new(),
        orphans: Arc::new(Mutex::new(Vec::new())),
    });
}

/// Runs `access` on this thread's table, after draining its orphans; the drained values (and
/// whatever `access` hands back for dropping) are dropped after the table is released.
fn with_table<R>(access: impl FnOnce(&mut LocalTable) -> R) -> Option<R> {
    TABLE
        .try_with(|table| {
            let (released, result) = {
                let mut table = table.try_borrow_mut().ok()?;
                let released = table.take_orphans();
                (released, access(&mut table))
            };
            drop(released);
            Some(result)
        })
        .ok()
        .flatten()
}

/// Drops, on this thread, the thread-bound values whose last provider reference was released
/// on another thread, and returns how many there were. The next thread-bound resolve on this
/// thread does the same, so calling this is only needed to release them sooner (for example from
/// an event loop's idle step).
pub fn release_orphaned_thread_bound_services() -> usize {
    let released = TABLE
        .try_with(|table| {
            table
                .try_borrow_mut()
                .map(|mut table| table.take_orphans())
                .unwrap_or_default()
        })
        .unwrap_or_default();
    // Dropped here, after the table's borrow ended.
    released.len()
}

/// What the provider keeps for a thread-bound value: `Send + Sync`, never the value itself.
pub(crate) struct ThreadBoundSlot {
    id: u64,
    owner: ThreadId,
    orphans: Orphans,
    service: Cow<'static, str>,
}

impl ThreadBoundSlot {
    /// Stores `value` in this thread's table and returns its slot.
    pub(crate) fn store(
        value: LocalService,
        service: Cow<'static, str>,
    ) -> Result<Self, ServiceResolutionError> {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let orphans = with_table(|table| {
            table.values.insert(id, value);
            Arc::clone(&table.orphans)
        })
        .ok_or_else(|| {
            ServiceResolutionError::failed(
                service.clone(),
                "this thread's thread-bound table is unavailable",
            )
        })?;
        Ok(Self {
            id,
            owner: thread::current().id(),
            orphans,
            service,
        })
    }

    /// The value, on its own thread; [`ServiceResolutionError::WrongThread`] anywhere else.
    pub(crate) fn get(&self) -> Result<LocalService, ServiceResolutionError> {
        if thread::current().id() != self.owner {
            return Err(ServiceResolutionError::WrongThread {
                service: self.service.clone(),
            });
        }
        with_table(|table| table.values.get(&self.id).cloned())
            .flatten()
            .ok_or_else(|| {
                ServiceResolutionError::failed(
                    self.service.clone(),
                    "its thread-bound value is no longer available",
                )
            })
    }
}

impl Drop for ThreadBoundSlot {
    fn drop(&mut self) {
        if thread::current().id() == self.owner {
            let removed = with_table(|table| table.values.remove(&self.id));
            if removed.is_some() {
                // Dropped here, outside the table's borrow.
                return;
            }
        }
        // Another thread, or the table is busy or gone: the owner releases it on its next access
        // (a table already gone dropped its values with its thread).
        if let Ok(mut ids) = self.orphans.lock() {
            ids.push(self.id);
        }
    }
}
