//! Change listeners: [`Configuration::on_change`] and [`OptionsMonitor::on_change`].
//!
//! [`Configuration::on_change`]: crate::configuration::Configuration::on_change
//! [`OptionsMonitor::on_change`]: crate::options::OptionsMonitor::on_change

use std::sync::{Arc, Mutex, PoisonError, Weak};

/// A change listener's registration. Dropping it removes the listener; [`Self::detach`] keeps
/// the listener for as long as what it listens to lives.
#[must_use = "dropping the subscription removes the listener at once; call `detach` to keep it"]
pub struct ChangeSubscription {
    unsubscribe: Option<Box<dyn FnOnce() + Send + Sync>>,
}

impl ChangeSubscription {
    /// Keeps the listener registered for good.
    pub fn detach(mut self) {
        self.unsubscribe = None;
    }
}

impl Drop for ChangeSubscription {
    fn drop(&mut self) {
        if let Some(unsubscribe) = self.unsubscribe.take() {
            unsubscribe();
        }
    }
}

impl std::fmt::Debug for ChangeSubscription {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ChangeSubscription")
            .field("attached", &self.unsubscribe.is_some())
            .finish()
    }
}

/// Listeners of type `F` (a `dyn Fn(..)`), called outside the lock in the order they were added.
pub(crate) struct Listeners<F: ?Sized> {
    entries: Arc<Mutex<Entries<F>>>,
}

struct Entries<F: ?Sized> {
    next: u64,
    listeners: Vec<(u64, Arc<F>)>,
}

impl<F: ?Sized> Default for Listeners<F> {
    fn default() -> Self {
        Self {
            entries: Arc::new(Mutex::new(Entries {
                next: 0,
                listeners: Vec::new(),
            })),
        }
    }
}

impl<F> Listeners<F>
where
    F: ?Sized + Send + Sync + 'static,
{
    pub(crate) fn add(&self, listener: Arc<F>) -> ChangeSubscription {
        let id = {
            let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
            let id = entries.next;
            entries.next += 1;
            entries.listeners.push((id, listener));
            id
        };
        let entries: Weak<Mutex<Entries<F>>> = Arc::downgrade(&self.entries);
        ChangeSubscription {
            unsubscribe: Some(Box::new(move || {
                if let Some(entries) = entries.upgrade() {
                    entries
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .listeners
                        .retain(|(entry, _)| *entry != id);
                }
            })),
        }
    }

    /// The listeners now, to call without holding the lock (a listener may add or remove
    /// listeners).
    pub(crate) fn current(&self) -> Vec<Arc<F>> {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .listeners
            .iter()
            .map(|(_, listener)| Arc::clone(listener))
            .collect()
    }

    pub(crate) fn len(&self) -> usize {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .listeners
            .len()
    }
}
