//! Assets read through NSH, headless: a Rust root on NSH's engine registers `add_assets`, the
//! catalog loads from the package's manifest when the app starts, and its files are read with
//! NSH's `fetch` of their `asset:/` URLs, on the engine thread and from another thread through
//! the surface's engine runner.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{
    cell::RefCell,
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

use crossbeam_channel::{Receiver, Sender};
use nsh_core::{ApproxTextEngine, MountBatch, MountItem, prelude::*};
use nsh_engine::{App, Assets, EngineConfig, LocalEngine, MountSink};
use rayx_app::services::assets::AssetService;
use rayx_app_nsh::{
    AppState, RayXAppBuilder, ServiceProvider, StartAppBuilderExt as _, app_services, app_state,
};
use rayx_services_assets::AssetAppBuilderExt as _;

thread_local! {
    /// The surface's root provider once the app is ready, for the test to read.
    static ROOT: RefCell<Option<ServiceProvider>> = const { RefCell::new(None) };
}

struct ChannelSink(Sender<MountBatch>);

impl MountSink for ChannelSink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.0.send(batch);
    }
}

/// The files of the app's crate `app`.
struct AppAssets;

impl Assets for AppAssets {
    fn read(&self, uri: &str) -> Result<Vec<u8>, String> {
        match uri {
            "asset:/@app/assets.toml" => Ok(br#"files = ["data/weather.json"]"#.to_vec()),
            "asset:/@app/data/weather.json" => Ok(br#"{ "temperature": 21.5 }"#.to_vec()),
            _ => Err(format!("{uri} is not bundled")),
        }
    }
}

/// The tests share the app (one composition root per process): one at a time.
fn serial() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A Rust root: starts the app with RayX's asset service, and shows the weather file's
/// temperature, read on the engine thread, once the app is ready.
#[component]
fn Root() -> impl IntoElement {
    rayx_app::testing::add_host_fakes(RayXAppBuilder::new().add_assets(|options| {
        options.package("app");
    }))
    .start();
    let temperature = signal(String::new());
    dynamic(move || match app_state() {
        AppState::Ready => {
            let services = app_services().unwrap();
            ROOT.with_borrow_mut(|root| *root = Some(services.clone()));
            let assets = services.required_service::<dyn AssetService>().unwrap();
            spawn(async move {
                let value: serde_json::Value = assets.load_json("data/weather.json").await.unwrap();
                temperature.set(value["temperature"].to_string());
            });
            text(move || format!("ready {}", temperature.get())).into_element()
        }
        AppState::Failed(message) => text(format!("failed: {message}")).into_element(),
        _ => text("starting").into_element(),
    })
}

fn pump_until(
    engine: &mut LocalEngine<ApproxTextEngine>,
    rx: &Receiver<MountBatch>,
    prefix: &str,
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
        if shown.iter().any(|text| text.starts_with(prefix)) {
            break;
        }
    }
    shown
}

fn start() -> (LocalEngine<ApproxTextEngine>, Receiver<MountBatch>) {
    let (tx, rx) = crossbeam_channel::unbounded();
    let engine = LocalEngine::start(
        EngineConfig {
            assets: Some(Arc::new(AppAssets) as Arc<dyn Assets>),
            ..EngineConfig::new(App::Rust(nsh_core::element::Root::of::<Root>()))
        },
        ApproxTextEngine,
        ChannelSink(tx),
    )
    .unwrap();
    (engine, rx)
}

#[test]
fn assets_load_through_nsh_once_the_app_is_ready() {
    let _serial = serial();
    let (mut engine, rx) = start();
    let shown = pump_until(&mut engine, &rx, "ready 21.5");
    assert!(
        shown.iter().any(|text| text == "ready 21.5"),
        "shown: {shown:?}"
    );

    let services = ROOT
        .with_borrow(|root| root.clone())
        .expect("the app started");
    let assets = services.required_service::<dyn AssetService>().unwrap();
    assert_eq!(
        assets.uri("data/weather.json").unwrap(),
        "asset:/@app/data/weather.json"
    );
    assert!(
        assets.diagnostics().is_empty(),
        "{:?}",
        assets.diagnostics()
    );
}

#[test]
fn another_thread_reads_assets_through_the_engine_runner() {
    let _serial = serial();
    let (mut engine, rx) = start();
    let shown = pump_until(&mut engine, &rx, "ready 21.5");
    assert!(
        shown.iter().any(|text| text == "ready 21.5"),
        "shown: {shown:?}"
    );
    let assets = ROOT
        .with_borrow(|root| root.clone())
        .expect("the app started")
        .required_service::<dyn AssetService>()
        .unwrap();
    // A fresh read (not the cache's): the engine thread runs it while it is pumped.
    assets.clear_cache();

    let (done, result) = crossbeam_channel::bounded(1);
    let reader = std::thread::spawn(move || {
        let value: serde_json::Value =
            futures::executor::block_on(assets.load_json("data/weather.json")).unwrap();
        done.send(value).unwrap();
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let value = loop {
        engine.pump();
        if let Ok(value) = result.recv_timeout(Duration::from_millis(5)) {
            break value;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the read did not finish"
        );
    };
    reader.join().unwrap();
    assert_eq!(value["temperature"], 21.5);
}
