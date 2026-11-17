//! Published values: what an implementation of a service publishes for its consumers to read and
//! follow, from any thread. A [`StateCell`] keeps the latest value, read synchronously; an
//! [`EventStream`] delivers each event to its current subscribers and keeps none.
//!
//! Both notify outside their locks, in subscription order, one publish at a time in the order
//! the publishes were made (a publish made during another's delivery is delivered right after it,
//! by the delivering thread), so a subscriber may read the cell, publish again or subscribe from
//! its callback, and the last value each subscriber heard is the cell's. A UI follows them
//! on its own thread through its binding (in `rayx_app_nsh`, an NSH signal over a cell).
//!
//! ```
//! use rayx_app::{EventStream, StateCell};
//! use std::sync::{Arc, Mutex};
//!
//! let language = StateCell::new("en".to_owned());
//! let seen = Arc::new(Mutex::new(Vec::new()));
//! let log = Arc::clone(&seen);
//! let subscription = language.subscribe(move |value| log.lock().unwrap().push(value.to_string()));
//! language.publish("hr".to_owned());
//! assert_eq!(*language.get(), "hr");
//! assert_eq!(*seen.lock().unwrap(), ["hr"]);
//! drop(subscription);
//!
//! let quits = EventStream::<u32>::new();
//! let _ignored = quits.subscribe(|code| assert_eq!(*code, 0));
//! quits.publish(0);
//! ```

use std::collections::VecDeque;
use std::fmt;
use std::sync::{Arc, Mutex, PoisonError, RwLock};

use crate::change::{ChangeSubscription, Listeners};

/// A subscriber of a [`StateCell`].
type StateListener<T> = dyn Fn(&Arc<T>) + Send + Sync;

/// A subscriber of an [`EventStream`].
type EventListener<E> = dyn Fn(&E) + Send + Sync;

/// What a cell or stream has published and not yet delivered, and whether a thread is delivering
/// now: publishes reach the subscribers one at a time, in the order they were made.
struct Deliveries<V: ?Sized> {
    delivering: bool,
    pending: VecDeque<Arc<V>>,
}

impl<V: ?Sized> Default for Deliveries<V> {
    fn default() -> Self {
        Self {
            delivering: false,
            pending: VecDeque::new(),
        }
    }
}

/// Delivers what is pending to `deliver`, one value at a time, until nothing is: what the thread
/// that published first does while it is the one delivering. A publish made meanwhile (from a
/// subscriber's callback, or another thread) only queues its value, so every subscriber hears
/// every value once, in publishing order, and no lock is held while a subscriber runs.
fn deliver<V: ?Sized>(deliveries: &Mutex<Deliveries<V>>, deliver: impl Fn(&Arc<V>)) {
    /// Lets another thread deliver again if a subscriber panics (disarmed on a normal return,
    /// which clears the flag itself).
    struct OnUnwind<'a, V: ?Sized>(Option<&'a Mutex<Deliveries<V>>>);

    impl<V: ?Sized> Drop for OnUnwind<'_, V> {
        fn drop(&mut self) {
            if let Some(deliveries) = self.0 {
                deliveries
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .delivering = false;
            }
        }
    }

    let mut on_unwind = OnUnwind(Some(deliveries));
    loop {
        let next = {
            let mut pending = deliveries.lock().unwrap_or_else(PoisonError::into_inner);
            let next = pending.pending.pop_front();
            if next.is_none() {
                // In the same critical section as the empty queue: a publish that queues after
                // this delivers itself.
                pending.delivering = false;
            }
            next
        };
        match next {
            Some(value) => deliver(&value),
            None => {
                on_unwind.0 = None;
                return;
            }
        }
    }
}

/// Queues `value`, after `stored` ran under the same lock (so writes and deliveries keep one
/// order); whether this thread is to deliver it.
fn queue<V: ?Sized>(
    deliveries: &Mutex<Deliveries<V>>,
    value: Arc<V>,
    stored: impl FnOnce(&Arc<V>),
) -> bool {
    let mut deliveries = deliveries.lock().unwrap_or_else(PoisonError::into_inner);
    stored(&value);
    deliveries.pending.push_back(value);
    !std::mem::replace(&mut deliveries.delivering, true)
}

/// The latest value of a piece of state, published by its owner and read from any thread.
///
/// [`get`](Self::get) gives the value now, as an `Arc` (a short read lock, never held while a
/// subscriber runs); [`publish`](Self::publish) replaces it and calls every subscriber with it.
/// Publishes reach the subscribers one at a time, in the order they were made: a publish made
/// while another is being delivered (by a subscriber, or on another thread) is delivered right
/// after it, by the thread already delivering. So each subscriber hears every value in order, and
/// the last value it heard is the cell's. Clones share the value and the subscribers: the owner
/// keeps one and hands out `&StateCell<T>` (or a clone) to consumers.
pub struct StateCell<T: ?Sized> {
    inner: Arc<StateInner<T>>,
}

struct StateInner<T: ?Sized> {
    value: RwLock<Arc<T>>,
    deliveries: Mutex<Deliveries<T>>,
    listeners: Listeners<StateListener<T>>,
}

impl<T: ?Sized> Clone for StateCell<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T: Send + Sync + 'static> StateCell<T> {
    /// A cell holding `initial`.
    pub fn new(initial: T) -> Self {
        Self::from_arc(Arc::new(initial))
    }
}

impl<T: ?Sized + Send + Sync + 'static> StateCell<T> {
    /// A cell holding `initial`, shared.
    pub fn from_arc(initial: Arc<T>) -> Self {
        Self {
            inner: Arc::new(StateInner {
                value: RwLock::new(initial),
                deliveries: Mutex::default(),
                listeners: Listeners::default(),
            }),
        }
    }

    /// The value now.
    pub fn get(&self) -> Arc<T> {
        Arc::clone(
            &self
                .inner
                .value
                .read()
                .unwrap_or_else(PoisonError::into_inner),
        )
    }

    /// Replaces the value with `value` and calls every subscriber with it, in the order they
    /// subscribed: on this thread, or on the thread delivering an earlier publish.
    pub fn publish(&self, value: T)
    where
        T: Sized,
    {
        self.publish_arc(Arc::new(value));
    }

    /// [`publish`](Self::publish) with a shared value.
    pub fn publish_arc(&self, value: Arc<T>) {
        let inner = &self.inner;
        let delivers = queue(&inner.deliveries, value, |value| {
            *inner.value.write().unwrap_or_else(PoisonError::into_inner) = Arc::clone(value);
        });
        if delivers {
            deliver(&inner.deliveries, |value| {
                inner.listeners.notify(|listener| listener(value));
            });
        }
    }

    /// Calls `listener` with each value published from now on, until the subscription is
    /// dropped. It does not get the value now: read it with [`get`](Self::get).
    pub fn subscribe(
        &self,
        listener: impl Fn(&Arc<T>) + Send + Sync + 'static,
    ) -> ChangeSubscription {
        self.inner.listeners.add(Arc::new(listener))
    }

    /// How many subscribers there are.
    pub fn subscribers(&self) -> usize {
        self.inner.listeners.len()
    }
}

impl<T: Default + Send + Sync + 'static> Default for StateCell<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for StateCell<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = self
            .inner
            .value
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        f.debug_struct("StateCell")
            .field("value", &&**value)
            .finish()
    }
}

/// Events published by their owner to whoever subscribed, from any thread. Nothing is kept: a
/// subscriber hears the events published after it subscribed, once each, in the order they were
/// published (one at a time, as a [`StateCell`] delivers).
///
/// Clones share the subscribers.
pub struct EventStream<E: ?Sized> {
    inner: Arc<EventInner<E>>,
}

struct EventInner<E: ?Sized> {
    deliveries: Mutex<Deliveries<E>>,
    listeners: Listeners<EventListener<E>>,
}

impl<E: ?Sized> Clone for EventStream<E> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<E: ?Sized> Default for EventStream<E> {
    fn default() -> Self {
        Self {
            inner: Arc::new(EventInner {
                deliveries: Mutex::default(),
                listeners: Listeners::default(),
            }),
        }
    }
}

impl<E: ?Sized + Send + Sync + 'static> EventStream<E> {
    /// A stream with no subscribers.
    pub fn new() -> Self {
        Self::default()
    }

    /// Calls every current subscriber with `event`, in the order they subscribed: on this
    /// thread, or on the thread delivering an earlier event.
    pub fn publish(&self, event: E)
    where
        E: Sized,
    {
        self.publish_arc(Arc::new(event));
    }

    /// [`publish`](Self::publish) with a shared event.
    pub fn publish_arc(&self, event: Arc<E>) {
        let inner = &self.inner;
        if queue(&inner.deliveries, event, |_| {}) {
            deliver(&inner.deliveries, |event| {
                inner.listeners.notify(|listener| listener(event));
            });
        }
    }

    /// Calls `listener` with each event published from now on, until the subscription is
    /// dropped.
    pub fn subscribe(&self, listener: impl Fn(&E) + Send + Sync + 'static) -> ChangeSubscription {
        self.inner.listeners.add(Arc::new(listener))
    }

    /// How many subscribers there are.
    pub fn subscribers(&self) -> usize {
        self.inner.listeners.len()
    }
}

impl<E: ?Sized> fmt::Debug for EventStream<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventStream").finish_non_exhaustive()
    }
}
