//! RayX's NSH store keeps no copy of NSH's entries, headless: a surface over another NSH storage
//! holding a copy of the first one's entries (a copied data directory, a seeded backend) keeps
//! its own, and NSH's storage cleared (by the app's TSX, by Rust's `Storage::clear()`) is what
//! every store reads from then on, a window opened afterwards included.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use futures::executor::block_on;
use nsh_core::element::Registry;
use nsh_core::storage::{Storage, StorageBackend, StorageChange};
use nsh_core::{ApproxTextEngine, MountBatch, prelude::*};
use nsh_engine::{App, Engine, EngineConfig, MountSink};
use rayx_app::services::storage::KeyValueStore;
use rayx_app::testing::add_host_fakes;
use rayx_app_nsh::{
    AppState, EngineRunner, RayXAppBuilder, ServiceProvider, StartAppBuilderExt as _, app_services,
    app_state, testing,
};
use rayx_services_storage::StorageAppBuilderExt as _;

/// The ready roots of the surfaces, as each engine's thread starts its app.
static ROOTS: Mutex<Vec<ServiceProvider>> = Mutex::new(Vec::new());

/// The tests of surfaces take turns: they count the roots in [`ROOTS`].
fn one_at_a_time() -> MutexGuard<'static, ()> {
    static TURN: Mutex<()> = Mutex::new(());
    let turn = TURN.lock().unwrap_or_else(PoisonError::into_inner);
    ROOTS.lock().unwrap().clear();
    turn
}

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

impl Device {
    fn entries(&self) -> BTreeMap<String, String> {
        self.0.lock().unwrap().clone()
    }
}

impl StorageBackend for Device {
    fn load(&self) -> Result<BTreeMap<String, String>, String> {
        Ok(self.entries())
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

/// Waits until `count` surfaces started, and gives the last one's root.
fn started_root(count: usize) -> ServiceProvider {
    let deadline = Instant::now() + Duration::from_secs(10);
    while ROOTS.lock().unwrap().len() < count && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let roots = ROOTS.lock().unwrap();
    assert_eq!(roots.len(), count, "the surfaces started");
    roots[count - 1].clone()
}

fn store(root: &ServiceProvider) -> Arc<dyn KeyValueStore> {
    root.required_service::<dyn KeyValueStore>().unwrap()
}

fn get(store: &Arc<dyn KeyValueStore>, key: &str) -> Option<Vec<u8>> {
    block_on(store.get("prefs".into(), key.into())).unwrap()
}

fn set(store: &Arc<dyn KeyValueStore>, key: &str, value: &[u8]) {
    block_on(store.set("prefs".into(), key.into(), value.to_vec())).unwrap();
}

#[test]
fn a_surface_over_a_copy_of_a_storage_keeps_its_own_entries() {
    let _turn = one_at_a_time();
    let device = Arc::new(Device::default());
    let _first_surface = surface(&device);
    let first = store(&started_root(1));
    set(&first, "lang", b"hr");
    // Another storage holding a copy of the first one's entries.
    let copy = Arc::new(Device(Mutex::new(device.entries())));
    let _second_surface = surface(&copy);
    let second = store(&started_root(2));
    assert_eq!(
        get(&second, "lang"),
        Some(b"hr".to_vec()),
        "the copied value"
    );

    set(&second, "lang", b"de");
    set(&second, "only_second", b"2");
    assert_eq!(get(&first, "lang"), Some(b"hr".to_vec()));
    assert_eq!(get(&first, "only_second"), None);
    assert_eq!(
        device.entries(),
        BTreeMap::from([("rayx/prefs/lang".into(), "thr".into())])
    );
    assert_eq!(
        copy.entries(),
        BTreeMap::from([
            ("rayx/prefs/lang".into(), "tde".into()),
            ("rayx/prefs/only_second".into(), "t2".into()),
        ])
    );
}

#[test]
fn a_window_opened_after_the_storage_was_cleared_shares_one_view() {
    let _turn = one_at_a_time();
    let device = Arc::new(Device::default());
    let _first_surface = surface(&device);
    let first_root = started_root(1);
    let first = store(&first_root);
    set(&first, "lang", b"hr");

    // The app clears NSH's storage on the engine thread (TSX's `Storage.clear()` clears the same
    // entries).
    let runner = first_root.required_service::<EngineRunner>().unwrap();
    block_on(runner.run(|| {
        Storage::clear();
        std::future::ready(())
    }))
    .unwrap();
    assert_eq!(
        get(&first, "lang"),
        None,
        "the store reads the cleared storage"
    );

    // A window opened now over the same storage: one view of its entries.
    let _second_surface = surface(&device);
    let second = store(&started_root(2));
    assert_eq!(get(&second, "lang"), None);
    set(&second, "lang", b"de");
    assert_eq!(get(&first, "lang"), Some(b"de".to_vec()));
    set(&first, "theme", b"dark");
    assert_eq!(get(&second, "theme"), Some(b"dark".to_vec()));
    assert_eq!(
        device.entries(),
        BTreeMap::from([
            ("rayx/prefs/lang".into(), "tde".into()),
            ("rayx/prefs/theme".into(), "tdark".into()),
        ]),
        "nothing of RayX's own beside the values"
    );
}

fn set_of_classes() -> Registry {
    testing::set(|set| {
        rayx_services_storage::register(set);
    })
}

const TSX_CLEARS: &str = r#"import { mount, signal, Storage, Text } from "nsh";
import { KeyValueStore, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines: string[] = [];
  try {
    const services = await new RayXAppBuilder().addHostFakes().addStorage().start();
    const storage = services.getRequiredService(KeyValueStore);
    await storage.set("prefs", "lang", "hr");
    await Storage.clear();
    lines.push(String(await storage.get("prefs", "lang")));
    await storage.set("prefs", "lang", "de");
    lines.push(String(await Storage.getItem("rayx/prefs/lang")));
    lines.push((await Storage.keys()).join(","));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn the_store_reads_what_the_app_s_tsx_cleared() {
    assert_eq!(
        testing::run(set_of_classes(), TSX_CLEARS),
        "done null|tde|rayx/prefs/lang"
    );
}
