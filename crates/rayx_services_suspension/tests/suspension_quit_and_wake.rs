//! The suspension service's quitting and wakes: an app asks before it quits while it listens
//! for quit requests (`on_quit_request`), a `QuitRequested` reaches that listener and changes
//! nothing else, `quit()` reaches the implementation, and a `Woke` counts the system's wakes as a
//! change.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use futures::FutureExt as _;
use rayx_app::RayXAppBuilder;
use rayx_app::services::suspension::*;
use rayx_app::testing::FakeSuspension;
use rayx_services_suspension::SuspensionAppBuilderExt as _;

fn quit_requested() -> SuspensionEvent {
    SuspensionEvent::new(
        SuspensionEventKind::QuitRequested,
        SuspensionEventReason::UserAskedToQuit,
    )
}

#[test]
fn the_app_asks_before_it_quits_while_it_listens_and_quits_through_its_service() {
    let fake = FakeSuspension::new();
    let suspension: Arc<dyn SuspensionService> = Arc::clone(&fake) as Arc<dyn SuspensionService>;
    assert!(!suspension.suspension().get().asks_before_quit);

    let requests = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&requests);
    let asking = suspension.on_quit_request(move || {
        counted.fetch_add(1, Ordering::SeqCst);
    });
    assert!(suspension.suspension().get().asks_before_quit);

    // Other events do not reach the listener.
    fake.emit(SuspensionEvent::new(
        SuspensionEventKind::Woke,
        SuspensionEventReason::SystemWoke,
    ));
    assert_eq!(requests.load(Ordering::SeqCst), 0);

    let before = *suspension.suspension().get();
    fake.emit(quit_requested());
    assert!(
        before.same_state(&suspension.suspension().get()),
        "a request changes nothing until the app quits"
    );
    assert_eq!(requests.load(Ordering::SeqCst), 1);

    suspension.quit().now_or_never().unwrap().unwrap();
    assert_eq!(fake.quits(), 1);

    // Once the listener goes, the app no longer asks and hears no request.
    drop(asking);
    assert!(!suspension.suspension().get().asks_before_quit);
    fake.emit(quit_requested());
    assert_eq!(requests.load(Ordering::SeqCst), 1);
}

#[test]
fn rayx_suspension_asks_while_a_listener_listens_without_an_engine() {
    let services = RayXAppBuilder::new().add_suspension().build().unwrap();
    let suspension = services
        .required_service::<dyn SuspensionService>()
        .unwrap();
    let asking = suspension.on_quit_request(|| {});
    assert!(suspension.suspension().get().asks_before_quit);
    // Kept for the engine thread until the service follows NSH.
    suspension.quit().now_or_never().unwrap().unwrap();
    drop(asking);
    assert!(!suspension.suspension().get().asks_before_quit);
}

#[test]
fn a_wake_is_counted_and_heard_as_a_change() {
    let suspension = FakeSuspension::new();
    let changes = Arc::new(Mutex::new(Vec::new()));
    let heard = Arc::clone(&changes);
    let _subscription = suspension
        .suspension()
        .subscribe(move |snapshot| heard.lock().unwrap().push(snapshot.wakes));
    for _ in 0..2 {
        suspension.emit(SuspensionEvent::new(
            SuspensionEventKind::Woke,
            SuspensionEventReason::SystemWoke,
        ));
    }
    let current = *suspension.suspension().get();
    assert_eq!(current.wakes, 2);
    assert_eq!(*changes.lock().unwrap(), [1, 2]);
    assert!(
        !SuspensionSnapshot::default().same_state(&SuspensionSnapshot::default().apply(
            &SuspensionEvent::new(SuspensionEventKind::Woke, SuspensionEventReason::SystemWoke,)
        ))
    );
    // A wake leaves the lifecycle as it was.
    assert_eq!(current.lifecycle, SuspensionLifecycleState::NotStarted);
}
