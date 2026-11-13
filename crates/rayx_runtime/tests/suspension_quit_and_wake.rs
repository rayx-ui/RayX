//! The suspension service's quitting and wakes: an app asks before it quits while it listens
//! for quit requests (the host is told when that starts and stops), a `QuitRequested` reaches
//! those listeners, `quit()` goes to the host registered with the app, and a `Woke` counts the
//! system's wakes as a change.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use rayx_core::RayXAppBuilder;
use rayx_runtime::services::suspension::*;

/// A host that records what it was asked.
#[derive(Clone, Default)]
struct Recorder(Arc<Mutex<Vec<String>>>);

impl SuspensionHost for Recorder {
    fn quit(&self) {
        self.0.lock().unwrap().push("quit".into());
    }

    fn ask_before_quit(&self, ask: bool) {
        self.0.lock().unwrap().push(format!("ask {ask}"));
    }
}

#[test]
fn the_app_asks_before_it_quits_while_it_listens_and_quits_through_its_host() {
    let host = Recorder::default();
    let services = RayXAppBuilder::new()
        .with_suspension_host(host.clone())
        .add_suspension()
        .build()
        .unwrap();
    let suspension = services.required_service::<SuspensionService>().unwrap();
    assert!(!suspension.asks_before_quit());

    let requests = Arc::new(AtomicUsize::new(0));
    let (first, second) = (Arc::clone(&requests), Arc::clone(&requests));
    let one = suspension.ask_before_quit(move || {
        first.fetch_add(1, Ordering::SeqCst);
    });
    let two = suspension.ask_before_quit(move || {
        second.fetch_add(1, Ordering::SeqCst);
    });
    assert!(suspension.asks_before_quit());

    let changed = suspension.publish(SuspensionEvent::new(
        SuspensionEventKind::QuitRequested,
        SuspensionEventReason::UserAskedToQuit,
    ));
    assert!(!changed, "a request changes nothing until the app quits");
    assert_eq!(requests.load(Ordering::SeqCst), 2);

    suspension.quit();
    // The host stops asking once the last listener goes.
    drop(one);
    assert!(suspension.asks_before_quit());
    drop(two);
    assert!(!suspension.asks_before_quit());
    assert_eq!(
        *host.0.lock().unwrap(),
        ["ask true", "ask true", "quit", "ask false"]
    );
}

#[test]
fn without_a_host_quitting_does_nothing() {
    let services = RayXAppBuilder::new().add_suspension().build().unwrap();
    let suspension = services.required_service::<SuspensionService>().unwrap();
    let asking = suspension.ask_before_quit(|| {});
    suspension.quit();
    drop(asking);
    assert!(!suspension.asks_before_quit());
}

#[test]
fn a_wake_is_counted_and_heard_as_a_change() {
    let suspension = SuspensionService::new();
    let changes = Arc::new(Mutex::new(Vec::new()));
    let heard = Arc::clone(&changes);
    let _subscription = suspension.subscribe_changed(move |change| {
        heard.lock().unwrap().push(change.current.wakes);
    });
    for _ in 0..2 {
        suspension.publish(SuspensionEvent::new(
            SuspensionEventKind::Woke,
            SuspensionEventReason::SystemWoke,
        ));
    }
    assert_eq!(suspension.current().wakes, 2);
    assert_eq!(*changes.lock().unwrap(), [1, 2]);
    // A wake leaves the lifecycle as it was.
    assert_eq!(
        suspension.current().lifecycle,
        SuspensionLifecycleState::NotStarted
    );
}
