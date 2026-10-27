//! RayX's services on NSH, headless: the bundled `appsettings.json` as configuration, assets
//! read and fonts registered through NSH, and the platform and suspension services kept
//! current from NSH's environment.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{
    cell::RefCell,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use crossbeam_channel::{Receiver, Sender};
use nsh_core::{ApproxTextEngine, MountBatch, MountItem, prelude::*};
use nsh_engine::{App, Assets, EngineConfig, LocalEngine, MountSink};
use rayx_app::rayx_runtime::prelude::*;
use rayx_app::rayx_runtime::services::{
    application_configuration::ApplicationConfigurationService,
    assets::AssetService,
    fonts::FontService,
    platform::{FormFactor, PlatformInfoService},
    suspension::SuspensionService,
};
use rayx_app::{
    AppSettingsAppBuilderExt, AppState, RayXAppBuilder, ServiceProvider, app_services, app_state,
    set_app,
};

thread_local! {
    /// The surface's root provider once the app is ready, for the test to read.
    static ROOT: RefCell<Option<ServiceProvider>> = const { RefCell::new(None) };
}

const PACIFICO: &[u8] = include_bytes!("fixtures/Pacifico-Regular.woff2");

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
            "asset:/@app/appsettings.json" => Ok(br#"{
                "RayX": {
                    "Application": { "name": "Bundled", "tooltips": { "showDelayMs": 120 } },
                    "Services": { "Fonts": { "defaultAlias": "script", "aliases": [
                        { "alias": "script", "family": "Pacifico" }
                    ] } }
                }
            }"#
            .to_vec()),
            "asset:/@app/assets.toml" => Ok(br#"
files = ["fonts/Pacifico-Regular.woff2", "data/odds.json"]
"#
            .to_vec()),
            "asset:/@app/fonts/Pacifico-Regular.woff2" => Ok(PACIFICO.to_vec()),
            "asset:/@app/data/odds.json" => Ok(br#"{ "home": 1.85 }"#.to_vec()),
            _ => Err(format!("{uri} is not bundled")),
        }
    }
}

/// The tests share the app (one composition root per process): one at a time.
fn serial() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Which settings the app adds besides its own.
static SETTINGS: AtomicUsize = AtomicUsize::new(BUNDLED);
const BUNDLED: usize = 0;
const MISSING: usize = 1;
const MISSING_OPTIONAL: usize = 2;

fn build_app(builder: RayXAppBuilder) -> RayXAppBuilder {
    let builder = match SETTINGS.load(Ordering::SeqCst) {
        MISSING => builder.add_app_settings("missing"),
        MISSING_OPTIONAL => {
            builder.add_optional_app_settings("asset:/@app/appsettings.Development.json")
        }
        _ => builder,
    };
    builder
        .add_app_settings("app")
        .add_application_configuration(|_| {})
        .add_assets(|options| {
            options.package("app");
        })
        .add_fonts(|_| {})
        .add_suspension()
}

/// A Rust root: starts the app, keeps the platform services current, and shows what the
/// services hold once it is ready.
#[component]
fn Root() -> impl IntoElement {
    rayx_app::start();
    rayx_app::use_platform_sync();
    let odds = signal(String::new());
    dynamic(move || match app_state() {
        AppState::Ready => {
            let services = app_services().unwrap();
            ROOT.with_borrow_mut(|root| *root = Some(services.clone()));
            let assets = services.required_service::<AssetService>().unwrap();
            spawn(async move {
                let value: serde_json::Value = assets.load_json("data/odds.json").await.unwrap();
                odds.set(value["home"].to_string());
            });
            text(move || format!("ready {}", odds.get())).into_element()
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

fn start(settings: usize) -> (LocalEngine<ApproxTextEngine>, Receiver<MountBatch>) {
    set_app(build_app);
    SETTINGS.store(settings, Ordering::SeqCst);
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
fn missing_settings_fail_startup_unless_optional() {
    let _serial = serial();
    let (mut engine, rx) = start(MISSING);
    let shown = pump_until(&mut engine, &rx, "failed");
    assert!(
        shown
            .iter()
            .any(|text| text.starts_with("failed")
                && text.contains("asset:/@missing/appsettings.json")),
        "shown: {shown:?}"
    );
    drop(engine);
    let (mut engine, rx) = start(MISSING_OPTIONAL);
    let shown = pump_until(&mut engine, &rx, "ready 1.85");
    assert!(
        shown.iter().any(|text| text == "ready 1.85"),
        "shown: {shown:?}"
    );
}

#[test]
fn services_run_on_nsh() {
    let _serial = serial();
    let (mut engine, rx) = start(BUNDLED);
    let shown = pump_until(&mut engine, &rx, "ready 1.85");
    assert!(
        shown.iter().any(|text| text == "ready 1.85"),
        "shown: {shown:?}"
    );

    let services = ROOT
        .with_borrow(|root| root.clone())
        .expect("the app started");

    // The bundled appsettings.json is the configuration.
    let configuration = services
        .required_service::<ApplicationConfigurationService>()
        .unwrap();
    assert_eq!(configuration.options().name, "Bundled");
    assert_eq!(configuration.options().tooltips.show_delay_ms, 120);

    // The font asset was registered with NSH, which read its family.
    let fonts = services.required_service::<FontService>().unwrap();
    assert_eq!(fonts.registered_fonts()[0].family, "Pacifico");
    assert_eq!(fonts.font_family("script"), "Pacifico");
    assert!(fonts.diagnostics().is_empty(), "{:?}", fonts.diagnostics());

    // The platform service has NSH's environment; the suspension service the app's start.
    let platform = services
        .required_service::<PlatformInfoService>()
        .unwrap()
        .current();
    assert_eq!(
        (platform.viewport.width, platform.viewport.height),
        (390.0, 844.0)
    );
    assert_ne!(platform.form_factor, FormFactor::Unknown);
    assert!(!platform.locale.languages.is_empty());
    let suspension = services
        .required_service::<SuspensionService>()
        .unwrap()
        .current();
    assert!(suspension.is_running());
}
