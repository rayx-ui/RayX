//! Two surfaces over one NSH storage in a process (two windows, two engines given the same
//! backend), headless: their providers' stores share NSH's entries, so each reads what the other
//! wrote as soon as the write answered, from any thread, and what either writes reaches the
//! device once; a surface over another NSH storage keeps its own entries.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use futures::executor::block_on;
use nsh_core::storage::{StorageBackend, StorageChange};
use nsh_core::{ApproxTextEngine, MountBatch, prelude::*};
use nsh_engine::{App, Engine, EngineConfig, MountSink};
use rayx_app::services::storage::KeyValueStore;
use rayx_app::testing::add_host_fakes;
use rayx_app_nsh::{
    AppState, RayXAppBuilder, ServiceProvider, StartAppBuilderExt as _, app_services, app_state,
};
use rayx_services_storage::StorageAppBuilderExt as _;

/// The ready roots of the surfaces, as each engine's thread starts its app.
static ROOTS: Mutex<Vec<ServiceProvider>> = Mutex::new(Vec::new());

#[component]
fn Root() -> impl IntoElement {
    add_host_fakes(RayXAppBuilder::new().add_storage()).start();
    dynamic(move || match app_state() {
        AppState::Ready => {
            ROOTS.lock().unwrap().push(app_services().unwrap());
            text("ready").into_element()
        }
        AppState::Failed(message) => text(format!("failed: {message}")).into_element(),
        _ => text("starting").into_element(),
    })
}

struct ChannelSink(Sender<MountBatch>);

impl MountSink for ChannelSink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.0.send(batch);
    }
}

/// The device's storage.
#[derive(Default)]
struct Device(Mutex<BTreeMap<String, String>>);

impl StorageBackend for Device {
    fn load(&self) -> Result<BTreeMap<String, String>, String> {
        Ok(self.0.lock().unwrap().clone())
    }

    fn write(&self, change: StorageChange) -> Result<(), String> {
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
        Ok(())
    }
}

/// A surface over `device`, on an engine thread of its own.
fn surface(device: &Arc<Device>) -> Engine {
    let (tx, _rx) = crossbeam_channel::unbounded();
    Engine::start(
        EngineConfig {
            storage: Some(Arc::clone(device) as Arc<dyn StorageBackend>),
            ..EngineConfig::new(App::Rust(nsh_core::element::Root::of::<Root>()))
        },
        || ApproxTextEngine,
        ChannelSink(tx),
    )
    .unwrap()
}

/// Waits until `count` surfaces started, and gives their roots' stores.
fn stores(count: usize) -> Vec<Arc<dyn KeyValueStore>> {
    let deadline = Instant::now() + Duration::from_secs(10);
    while ROOTS.lock().unwrap().len() < count && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let roots = ROOTS.lock().unwrap();
    assert_eq!(roots.len(), count, "the surfaces started");
    roots
        .iter()
        .map(|root| root.required_service::<dyn KeyValueStore>().unwrap())
        .collect()
}

fn get(store: &Arc<dyn KeyValueStore>, key: &str) -> Option<Vec<u8>> {
    block_on(store.get("prefs".into(), key.into())).unwrap()
}

fn wait_for(what: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !what() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    what()
}

#[test]
fn surfaces_over_one_storage_share_its_entries() {
    let device = Arc::new(Device::default());
    device
        .0
        .lock()
        .unwrap()
        .insert("rayx/prefs/theme".into(), "tdark".into());
    let first_surface = surface(&device);
    let first = stores(1).remove(0);
    let second_surface = surface(&device);
    let second = stores(2).remove(1);
    assert_eq!(get(&second, "theme"), Some(b"dark".to_vec()));

    // Written off the engine threads through one surface's provider: the other's reads it as
    // soon as the write answered, and it reaches the device once.
    block_on(first.set("prefs".into(), "lang".into(), b"hr".to_vec())).unwrap();
    assert_eq!(get(&second, "lang"), Some(b"hr".to_vec()));
    block_on(second.set("prefs".into(), "lang".into(), b"de".to_vec())).unwrap();
    assert_eq!(get(&first, "lang"), Some(b"de".to_vec()));
    assert!(block_on(second.remove("prefs".into(), "theme".into())).unwrap());
    assert_eq!(get(&first, "theme"), None);
    assert!(wait_for(|| {
        let device = device.0.lock().unwrap();
        device.get("rayx/prefs/lang").map(String::as_str) == Some("tde")
            && !device.contains_key("rayx/prefs/theme")
    }));

    // The first surface goes: its provider's store answers that its engine is gone, and the
    // second's keeps the entries.
    drop(first_surface);
    assert!(block_on(first.set("prefs".into(), "later".into(), b"1".to_vec())).is_err());
    block_on(second.set("prefs".into(), "later".into(), b"1".to_vec())).unwrap();
    assert_eq!(get(&second, "lang"), Some(b"de".to_vec()));
    assert!(wait_for(|| {
        device
            .0
            .lock()
            .unwrap()
            .get("rayx/prefs/later")
            .map(String::as_str)
            == Some("t1")
    }));

    // A surface over another storage keeps its own entries.
    let other_device = Arc::new(Device::default());
    let _other_surface = surface(&other_device);
    let other = stores(3).remove(2);
    assert_eq!(get(&other, "lang"), None);
    drop(second_surface);
}
