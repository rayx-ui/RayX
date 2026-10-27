//! The change-listener registry services hand subscriptions out of.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use rayx_core::ChangeSubscription;
use rayx_core::change::Listeners;

type Listener = dyn Fn(&str) + Send + Sync;

#[test]
fn listeners_hear_in_order_until_their_subscription_drops() {
    let listeners = Listeners::<Listener>::default();
    let heard = Arc::new(Mutex::new(Vec::new()));
    let first = Arc::clone(&heard);
    let second = Arc::clone(&heard);
    let a = listeners.add(Arc::new(move |event| {
        first.lock().unwrap().push(format!("a:{event}"))
    }));
    let b = listeners.add(Arc::new(move |event| {
        second.lock().unwrap().push(format!("b:{event}"))
    }));
    listeners.notify(|listener| listener("one"));
    drop(a);
    listeners.notify(|listener| listener("two"));
    assert_eq!(*heard.lock().unwrap(), ["a:one", "b:one", "b:two"]);
    assert_eq!(listeners.len(), 1);
    b.detach();
    assert_eq!(listeners.len(), 1);
}

#[test]
fn a_listener_may_subscribe_while_being_notified_and_clones_share_listeners() {
    let listeners = Listeners::<Listener>::default();
    let shared = listeners.clone();
    let kept = Arc::new(Mutex::new(Vec::<ChangeSubscription>::new()));
    let sink = Arc::clone(&kept);
    let _outer = listeners.add(Arc::new(move |_| {
        sink.lock().unwrap().push(shared.add(Arc::new(|_| {})));
    }));
    listeners.notify(|listener| listener("now"));
    assert_eq!(listeners.len(), 2);
}

#[test]
fn a_subscription_runs_its_unsubscribe_once_when_dropped() {
    let calls = Arc::new(Mutex::new(0));
    let counter = Arc::clone(&calls);
    drop(ChangeSubscription::new(move || {
        *counter.lock().unwrap() += 1
    }));
    assert_eq!(*calls.lock().unwrap(), 1);
    let counter = Arc::clone(&calls);
    ChangeSubscription::new(move || *counter.lock().unwrap() += 1).detach();
    assert_eq!(*calls.lock().unwrap(), 1);
}
