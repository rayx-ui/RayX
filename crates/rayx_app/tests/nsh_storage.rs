//! The app's key-value store on NSH, headless: it loads what the device's storage held at
//! startup, writes through NSH's storage on the engine thread, and a write made outside the
//! engine reaches the device once the engine runs again. History persists through it.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{
    cell::RefCell,
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use crossbeam_channel::{Receiver, Sender};
use nsh_core::storage::{StorageBackend, StorageChange};
use nsh_core::{ApproxTextEngine, MountBatch, MountItem, prelude::*};
use nsh_engine::{App, EngineConfig, LocalEngine, MountSink};
use rayx_app::rayx_runtime::prelude::*;
use rayx_app::rayx_runtime::services::history::{HISTORY_BUCKET, HistoryStore};
use rayx_app::rayx_runtime::services::storage::KeyValueStore;
use rayx_app::{AppState, RayXAppBuilder, ServiceProvider, app_services, app_state, set_app};

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
    builder.add_storage().add_persistent_history()
}

/// Starts the app and, once it is ready, reads what the device held and writes on the engine
/// thread.
#[component]
fn Root() -> impl IntoElement {
    rayx_app::start();
    dynamic(move || match app_state() {
        AppState::Ready => {
            let services = app_services().unwrap();
            ROOT.with_borrow_mut(|root| *root = Some(services.clone()));
            let store = services.required_service::<dyn KeyValueStore>().unwrap();
            let theme = store.get("prefs", "theme").unwrap().unwrap_or_default();
            store.set("prefs", "lang", b"hr".to_vec()).unwrap();
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
    set_app(build_app);
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
    // Outside the engine (the engine runs only while it is pumped): read from the mirror, and
    // written to the device once the engine runs again.
    assert_eq!(store.get("prefs", "lang").unwrap(), Some(b"hr".to_vec()));
    store.set("prefs", "later", b"1".to_vec()).unwrap();
    assert!(!device.0.lock().unwrap().contains_key("rayx/prefs/later"));
    let written = || device.0.lock().unwrap().get("rayx/prefs/later").cloned();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while written().is_none() && std::time::Instant::now() < deadline {
        engine.pump();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(written().as_deref(), Some("t1"));

    // History is kept in the same store, so on the device.
    let history = services.required_service::<dyn HistoryStore>().unwrap();
    let scope = rayx_app::rayx_runtime::services::history::HistoryScopeId::new("doc").unwrap();
    history
        .save(
            rayx_app::rayx_runtime::services::history::HistoryJournalV2::new(1, scope, Vec::new()),
            None,
        )
        .unwrap();
    let key = format!("rayx/{HISTORY_BUCKET}/doc");
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !device.0.lock().unwrap().contains_key(&key) && std::time::Instant::now() < deadline {
        engine.pump();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(device.0.lock().unwrap().contains_key(&key));
}
