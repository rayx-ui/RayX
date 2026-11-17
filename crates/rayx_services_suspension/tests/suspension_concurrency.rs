//! RayX's suspension service changed from many threads at once: the app asking (and no longer
//! asking) before it quits on its threads while events arrive on others. Every change reaches the
//! published state: none overwrites another.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Barrier, Mutex};
use std::thread;

use futures::FutureExt as _;
use rayx_app::services::suspension::*;
use rayx_services_suspension::testing::FedSuspension;

const THREADS: usize = 4;
const ROUNDS: u32 = 2_000;

#[test]
fn asking_and_events_from_many_threads_all_reach_the_published_state() {
    for _ in 0..5 {
        let fed = FedSuspension::new();
        let suspension = fed.service();
        let last_heard = Arc::new(Mutex::new(None));
        let heard = Arc::clone(&last_heard);
        let _subscription = suspension
            .suspension()
            .subscribe(move |snapshot| *heard.lock().unwrap() = Some(**snapshot));
        let barrier = Arc::new(Barrier::new(THREADS * 2));
        let mut threads = Vec::new();
        for _ in 0..THREADS {
            // Events, as the engine thread gives them.
            let (fed, start) = (fed.clone(), Arc::clone(&barrier));
            threads.push(thread::spawn(move || {
                start.wait();
                for _ in 0..ROUNDS {
                    fed.emit(SuspensionEvent::new(
                        SuspensionEventKind::Woke,
                        SuspensionEventReason::SystemWoke,
                    ));
                }
            }));
            // The app asking and no longer asking, each thread's last call asking.
            let (suspension, start) = (Arc::clone(&suspension), Arc::clone(&barrier));
            threads.push(thread::spawn(move || {
                start.wait();
                for round in 0..ROUNDS {
                    suspension
                        .ask_before_quit(round % 2 == 1)
                        .now_or_never()
                        .unwrap()
                        .unwrap();
                }
            }));
        }
        for thread in threads {
            thread.join().unwrap();
        }

        let state = *suspension.suspension().get();
        assert_eq!(state.wakes, THREADS as u32 * ROUNDS, "a wake was lost");
        assert!(state.asks_before_quit, "the last ask was lost");
        assert_eq!(
            *last_heard.lock().unwrap(),
            Some(state),
            "subscribers last heard the published state"
        );
    }
}
