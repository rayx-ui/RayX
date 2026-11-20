//! A library's files in the catalog, headless on NSH: the app's set serves a library's set that
//! ships files (`Registry::assets`), the app names no package (`add_assets(|_| {})`), and once
//! the app starts the catalog has the library's files, listed by NSH's `served_asset_files()` on
//! the engine thread, with the metadata of the manifest the library ships, and reads them with
//! NSH's `fetch`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{cell::RefCell, sync::Arc, time::Duration};

use crossbeam_channel::{Receiver, Sender};
use nsh_core::element::{AssetFile, AssetKind as NshAssetKind, Registry};
use nsh_core::{ApproxTextEngine, MountBatch, MountItem, prelude::*};
use nsh_engine::{App, Assets, EngineConfig, LocalEngine, MountSink};
use rayx_app::services::assets::{AssetKind, AssetService};
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

/// The files the library `acme-ui` ships, as `assets!()` lists them.
static LIBRARY: [AssetFile; 3] = [
    AssetFile {
        uri: "asset:/@acme-ui/assets.toml",
        source: "/acme-ui/assets/assets.toml",
        kind: NshAssetKind::File,
    },
    AssetFile {
        uri: "asset:/@acme-ui/data/palette.json",
        source: "/acme-ui/assets/data/palette.json",
        kind: NshAssetKind::File,
    },
    AssetFile {
        uri: "asset:/@acme-ui/icons/star.svg",
        source: "/acme-ui/assets/icons/star.svg",
        kind: NshAssetKind::Image,
    },
];

/// The library's set, `acme/ui`, which ships its files.
fn library() -> Registry {
    let mut set = Registry::ui_set("acme/ui");
    set.assets(&LIBRARY);
    set
}

/// The app's set, which serves the library's.
fn app() -> Registry {
    let mut set = Registry::ui_set("app/native");
    set.serve(library());
    set
}

/// The files the host serves: the library's.
struct HostAssets;

impl Assets for HostAssets {
    fn read(&self, uri: &str) -> Result<Vec<u8>, String> {
        match uri {
            "asset:/@acme-ui/assets.toml" => {
                Ok(b"[[asset]]\npath = \"icons/star.svg\"\ntags = [\"brand\"]\n".to_vec())
            }
            "asset:/@acme-ui/data/palette.json" => Ok(br#"{ "accent": "teal" }"#.to_vec()),
            "asset:/@acme-ui/icons/star.svg" => Ok(b"<svg/>".to_vec()),
            _ => Err(format!("{uri} is not bundled")),
        }
    }
}

/// A Rust root: starts the app with RayX's asset service naming no package, and shows the
/// library's palette, read through the catalog, once the app is ready.
#[component]
fn Root() -> impl IntoElement {
    rayx_app::testing::add_host_fakes(RayXAppBuilder::new().add_assets(|_| {})).start();
    let accent = signal(String::new());
    dynamic(move || match app_state() {
        AppState::Ready => {
            let services = app_services().unwrap();
            ROOT.with_borrow_mut(|root| *root = Some(services.clone()));
            let assets = services.required_service::<dyn AssetService>().unwrap();
            spawn(async move {
                let value: serde_json::Value = assets.load_json("data/palette.json").await.unwrap();
                accent.set(value["accent"].as_str().unwrap_or_default().to_owned());
            });
            text(move || format!("ready {}", accent.get())).into_element()
        }
        AppState::Failed(message) => text(format!("failed: {message}")).into_element(),
        _ => text("starting").into_element(),
    })
}

fn pump_until(
    engine: &mut LocalEngine<ApproxTextEngine>,
    rx: &Receiver<MountBatch>,
    shown: &str,
) -> Vec<String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut texts = Vec::new();
    while std::time::Instant::now() < deadline {
        engine.pump();
        while let Ok(batch) = rx.recv_timeout(Duration::from_millis(5)) {
            for item in &batch.items {
                if let MountItem::Text { text, .. } = item {
                    texts.push(text.text.clone());
                }
            }
        }
        if texts.iter().any(|text| text == shown) {
            break;
        }
    }
    texts
}

#[test]
fn a_served_library_s_files_are_in_the_catalog_without_the_app_naming_them() {
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut engine = LocalEngine::start(
        EngineConfig {
            ui_sets: vec![app()],
            assets: Some(Arc::new(HostAssets) as Arc<dyn Assets>),
            ..EngineConfig::new(App::Rust(nsh_core::element::Root::of::<Root>()))
        },
        ApproxTextEngine,
        ChannelSink(tx),
    )
    .unwrap();
    let shown = pump_until(&mut engine, &rx, "ready teal");
    assert!(
        shown.iter().any(|text| text == "ready teal"),
        "shown: {shown:?}"
    );

    let assets = ROOT
        .with_borrow(|root| root.clone())
        .expect("the app started")
        .required_service::<dyn AssetService>()
        .unwrap();
    let catalog = assets.loaded_catalog().unwrap();
    assert_eq!(
        catalog.as_ref().as_ref().unwrap().packages().to_vec(),
        ["acme-ui"]
    );
    let star = assets.descriptor("@acme-ui/icons/star.svg").unwrap();
    assert_eq!(star.kind, AssetKind::Svg);
    assert_eq!(star.tags, ["brand"], "the library's manifest's metadata");
    assert_eq!(
        assets.uri("data/palette.json").unwrap(),
        "asset:/@acme-ui/data/palette.json"
    );
    assert!(
        assets.diagnostics().is_empty(),
        "{:?}",
        assets.diagnostics()
    );
}
