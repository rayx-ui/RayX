//! The app's key-value store on NSH (`add_storage()`), headless: it reads what the device's
//! storage held, writes through NSH's storage on the engine thread at once, and a call made
//! outside the engine answers once the engine ran it (through the engine runner), its write on
//! the device by then.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{
    cell::RefCell,
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use crossbeam_channel::{Receiver, Sender};
use futures::FutureExt as _;
use futures::executor::block_on;
use nsh_core::storage::{StorageBackend, StorageChange};
use nsh_core::{ApproxTextEngine, MountBatch, MountItem, prelude::*};
use nsh_engine::{App, EngineConfig, LocalEngine, MountSink};
use rayx_app::services::storage::KeyValueStore;
use rayx_app_nsh::{
    AppState, RayXAppBuilder, ServiceProvider, StartAppBuilderExt as _, app_services, app_state,
};
use rayx_services_storage::StorageAppBuilderExt as _;

thread_local! {
    static ROOT: RefCell<Option<ServiceProvider>> = const { RefCell::new(None) };
}

struct ChannelSink(Sender<MountBatch>);

impl MountSink for ChannelSink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.0.send(batch);
    }
}

/// The device's storage: what it held when the app started, and every change NSH hands it.
#[derive(Default)]
struct Device(Mutex<BTreeMap<String, String>>);

impl StorageBackend for Device {
    fn load(&self) -> Result<BTreeMap<String, String>, String> {
        Ok(self.0.lock().unwrap().clone())
    }

    fn write(&self, change: StorageChange) {
        let mut entries = self.0.lock().unwrap();
        match change {
            StorageChange::Set(key, value) => {
                entries.insert(key, value);
            }
            StorageChange::Remove(key) => {
                entries.remove(&key);
            }
            StorageChange::Clear => entries.clear(),
        }
    }
}

fn build_app(builder: RayXAppBuilder) -> RayXAppBuilder {
    builder.add_storage()
}

/// Starts the app and, once it is ready, reads what the device held and writes on the engine
/// thread.
#[component]
fn Root() -> impl IntoElement {
    build_app(::rayx_app::testing::add_host_fakes(RayXAppBuilder::new())).start();
    dynamic(move || match app_state() {
        AppState::Ready => {
            let services = app_services().unwrap();
            ROOT.with_borrow_mut(|root| *root = Some(services.clone()));
            let store = services.required_service::<dyn KeyValueStore>().unwrap();
            // On the engine thread, where NSH's storage is reachable: the store answers at once.
            let theme = store
                .get("prefs".into(), "theme".into())
                .now_or_never()
                .unwrap()
                .unwrap()
                .unwrap_or_default();
            store
                .set("prefs".into(), "lang".into(), b"hr".to_vec())
                .now_or_never()
                .unwrap()
                .unwrap();
            text(format!("theme {}", String::from_utf8(theme).unwrap())).into_element()
        }
        AppState::Failed(message) => text(format!("failed: {message}")).into_element(),
        _ => text("starting").into_element(),
    })
}

fn pump_until(
    engine: &mut LocalEngine<ApproxTextEngine>,
    rx: &Receiver<MountBatch>,
    done: impl Fn(&[String]) -> bool,
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
        if done(&shown) {
            break;
        }
    }
    shown
}

#[test]
fn the_store_keeps_the_app_s_values_in_nsh_s_storage() {
    let device = Arc::new(Device::default());
    device.0.lock().unwrap().extend([
        ("rayx/prefs/theme".to_string(), "tdark".to_string()),
        ("draft".to_string(), "the TSX's own".to_string()),
    ]);
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut engine = LocalEngine::start(
        EngineConfig {
            storage: Some(device.clone() as Arc<dyn StorageBackend>),
            ..EngineConfig::new(App::Rust(nsh_core::element::Root::of::<Root>()))
        },
        ApproxTextEngine,
        ChannelSink(tx),
    )
    .unwrap();
    let shown = pump_until(&mut engine, &rx, |shown| {
        shown
            .iter()
            .any(|text| text.starts_with("theme") || text.starts_with("failed"))
    });
    assert!(
        shown.iter().any(|text| text == "theme dark"),
        "shown: {shown:?}"
    );
    // Written on the engine thread: on the device at once.
    assert_eq!(device.0.lock().unwrap()["rayx/prefs/lang"], "thr");
    assert_eq!(device.0.lock().unwrap()["draft"], "the TSX's own");

    let services = ROOT
        .with_borrow(|root| root.clone())
        .expect("the app started");
    let store = services.required_service::<dyn KeyValueStore>().unwrap();
    assert!(store.capabilities().persistent);
    // On another thread (the engine runs only while it is pumped): each call answers once the
    // engine ran it, through the engine runner, the write on the device by then.
    let other = Arc::clone(&store);
    let caller = std::thread::spawn(move || {
        assert_eq!(
            block_on(other.get("prefs".into(), "lang".into())).unwrap(),
            Some(b"hr".to_vec())
        );
        block_on(other.set("prefs".into(), "later".into(), b"1".to_vec())).unwrap();
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !caller.is_finished() && std::time::Instant::now() < deadline {
        engine.pump();
        std::thread::sleep(Duration::from_millis(5));
    }
    caller.join().unwrap();
    assert_eq!(
        device
            .0
            .lock()
            .unwrap()
            .get("rayx/prefs/later")
            .map(String::as_str),
        Some("t1")
    );
}
