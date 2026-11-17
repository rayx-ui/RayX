//! A service's published values in a Rust component, headless: an NSH signal over a `StateCell`
//! and a listener over an `EventStream` follow publishes from the engine's thread and from other
//! threads, on the engine's thread, and stop with the component's owner.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use nsh_core::{ApproxTextEngine, MountBatch, MountItem, prelude::*};
use nsh_engine::{App, EngineConfig, LocalEngine, MountSink};
use rayx_app_nsh::{EventStream, StateCell, use_event_stream, use_state_cell};

struct ChannelSink(Sender<MountBatch>);

impl MountSink for ChannelSink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.0.send(batch);
    }
}

/// What a service publishes, shared by the test and the UI.
struct Published {
    language: StateCell<String>,
    quits: EventStream<u32>,
    /// Whether the panel is shown (its owner goes when it is not).
    shown: StateCell<bool>,
    /// The events the panel heard, and on which thread.
    heard: Mutex<Vec<(u32, bool)>>,
    /// The engine's thread.
    engine: OnceLock<thread::ThreadId>,
}

fn published() -> &'static Published {
    static PUBLISHED: OnceLock<Published> = OnceLock::new();
    PUBLISHED.get_or_init(|| Published {
        language: StateCell::new("en".to_owned()),
        quits: EventStream::new(),
        shown: StateCell::new(true),
        heard: Mutex::default(),
        engine: OnceLock::new(),
    })
}

/// Follows the language and hears quits while it is shown (made in the root's `dynamic`, so its
/// owner goes when it is hidden).
fn panel() -> impl IntoElement {
    let language = use_state_cell(&published().language);
    use_event_stream(&published().quits, |code| {
        let on_engine = published().engine.get() == Some(&thread::current().id());
        published().heard.lock().unwrap().push((code, on_engine));
    });
    text(move || format!("language {}", language.get()))
}

#[component]
fn Root() -> impl IntoElement {
    let _ = published().engine.set(thread::current().id());
    let shown = use_state_cell(&published().shown);
    // Publishing on the engine's thread, once the UI runs.
    spawn(async { published().language.publish("de".to_owned()) });
    dynamic(move || {
        if *shown.get() {
            panel().into_element()
        } else {
            text("hidden").into_element()
        }
    })
}

fn pump_until(
    engine: &mut LocalEngine<ApproxTextEngine>,
    rx: &Receiver<MountBatch>,
    wanted: &str,
) -> Vec<String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut shown = Vec::new();
    while std::time::Instant::now() < deadline {
        engine.pump();
        while let Ok(batch) = rx.recv_timeout(Duration::from_millis(5)) {
            for item in &batch.items {
                if let MountItem::Text { text, .. } = item {
                    shown.push(text.text.clone());
                }
            }
        }
        if shown.iter().any(|text| text == wanted) {
            break;
        }
    }
    shown
}

fn pump_for(engine: &mut LocalEngine<ApproxTextEngine>, rx: &Receiver<MountBatch>) {
    let deadline = std::time::Instant::now() + Duration::from_millis(200);
    while std::time::Instant::now() < deadline {
        engine.pump();
        while rx.recv_timeout(Duration::from_millis(5)).is_ok() {}
    }
}

#[test]
fn a_component_follows_published_values_and_stops_with_its_owner() {
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut engine = LocalEngine::start(
        EngineConfig::new(App::Rust(nsh_core::element::Root::of::<Root>())),
        ApproxTextEngine,
        ChannelSink(tx),
    )
    .unwrap();
    let state = published();

    // The value at mount, then a publish on the engine's thread.
    let shown = pump_until(&mut engine, &rx, "language de");
    assert!(shown.iter().any(|text| text == "language en"), "{shown:?}");
    assert!(shown.iter().any(|text| text == "language de"), "{shown:?}");

    // A publish from another thread reaches the signal on the engine's thread.
    thread::spawn(|| published().language.publish("hr".to_owned()))
        .join()
        .unwrap();
    let shown = pump_until(&mut engine, &rx, "language hr");
    assert!(shown.iter().any(|text| text == "language hr"), "{shown:?}");

    // Many publishes from several threads: the signal settles on the cell's last value.
    let threads: Vec<_> = (0..4)
        .map(|n| {
            thread::spawn(move || {
                for i in 0..25 {
                    published().language.publish(format!("t{n}-{i}"));
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    let last = format!("language {}", state.language.get());
    let shown = pump_until(&mut engine, &rx, &last);
    assert_eq!(shown.last(), Some(&last), "{shown:?}");

    // Events from this thread and another reach the listener, in order, on the engine's thread.
    state.quits.publish(1);
    thread::spawn(|| published().quits.publish(2))
        .join()
        .unwrap();
    pump_for(&mut engine, &rx);
    assert_eq!(*state.heard.lock().unwrap(), [(1, true), (2, true)]);
    assert_eq!(state.language.subscribers(), 1);
    assert_eq!(state.quits.subscribers(), 1);

    // The panel's owner goes: its subscriptions go with it.
    state.shown.publish(false);
    let shown = pump_until(&mut engine, &rx, "hidden");
    assert!(shown.iter().any(|text| text == "hidden"), "{shown:?}");
    assert_eq!(state.language.subscribers(), 0);
    assert_eq!(state.quits.subscribers(), 0);
    state.quits.publish(3);
    pump_for(&mut engine, &rx);
    assert_eq!(
        state.heard.lock().unwrap().len(),
        2,
        "no event after the owner went"
    );
}
