//! The services the host requires: a Rust root's and a TSX app's start fail, before anything is
//! made, when the platform, suspension, asset or font service is missing, naming each missing
//! one with how to register it in the app's language, all in one message; with the four
//! registered (by anyone) the app starts. Both builders are empty when new.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod headless;

use std::cell::RefCell;
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use nsh_core::element::Root;
use nsh_core::{ApproxTextEngine, MountBatch, MountItem, prelude::*};
use nsh_engine::{App, EngineConfig, LocalEngine, MountSink};
use rayx_app::services::assets::{AssetCatalog, AssetService};
use rayx_app::services::fonts::{FontService, FontState};
use rayx_app::services::platform::{PlatformInfoService, PlatformSnapshot};
use rayx_app::services::suspension::SuspensionService;
use rayx_app::testing::{FakeAssets, FakeFonts, FakePlatform, FakeSuspension};
use rayx_app_nsh::{AppBuilder as _, RayXAppBuilder, StartAppBuilderExt as _};

/// The four, by the names the messages give them.
const PLATFORM: &str = "dyn rayx_app::services::platform::PlatformInfoService";
const SUSPENSION: &str = "dyn rayx_app::services::suspension::SuspensionService";
const ASSETS: &str = "dyn rayx_app::services::assets::AssetService";
const FONTS: &str = "dyn rayx_app::services::fonts::FontService";

thread_local! {
    /// The services the Rust root leaves out.
    static MISSING: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
}

/// `RayXAppBuilder::new()` with fakes of the four but those in `MISSING`.
fn builder() -> RayXAppBuilder {
    let missing = MISSING.with_borrow(Clone::clone);
    let mut builder = RayXAppBuilder::new();
    let services = builder.services_mut();
    if !missing.contains(&PLATFORM) {
        services.add_singleton_as::<dyn PlatformInfoService>(FakePlatform::new(
            PlatformSnapshot::default(),
        )
            as Arc<dyn PlatformInfoService>);
    }
    if !missing.contains(&SUSPENSION) {
        services.add_singleton_as::<dyn SuspensionService>(
            FakeSuspension::new() as Arc<dyn SuspensionService>
        );
    }
    if !missing.contains(&ASSETS) {
        services.add_singleton_as::<dyn AssetService>(
            FakeAssets::new(AssetCatalog::default()) as Arc<dyn AssetService>
        );
    }
    if !missing.contains(&FONTS) {
        services.add_singleton_as::<dyn FontService>(
            FakeFonts::new(FontState::default()) as Arc<dyn FontService>
        );
    }
    builder
}

/// A Rust root: awaits `start_async()` and shows `ready`, or `failed` and the error.
#[component]
fn Start() -> impl IntoElement {
    let shown = signal("starting".to_owned());
    drop(spawn(async move {
        shown.set(match builder().start_async().await {
            Ok(_) => "ready".to_owned(),
            Err(error) => format!("failed {error}"),
        });
    }));
    text(move || shown.get())
}

struct ChannelSink(Sender<MountBatch>);

impl MountSink for ChannelSink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.0.send(batch);
    }
}

/// What a Rust root missing `missing` shows once it started or failed.
fn rust_start(missing: &[&'static str]) -> String {
    MISSING.with_borrow_mut(|kept| *kept = missing.to_vec());
    let (tx, rx): (Sender<MountBatch>, Receiver<MountBatch>) = crossbeam_channel::unbounded();
    let mut engine = LocalEngine::start(
        EngineConfig::new(App::Rust(Root::of::<Start>())),
        ApproxTextEngine,
        ChannelSink(tx),
    )
    .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        engine.pump();
        while let Ok(batch) = rx.recv_timeout(Duration::from_millis(5)) {
            for item in &batch.items {
                if let MountItem::Text { text, .. } = item
                    && (text.text == "ready" || text.text.starts_with("failed"))
                {
                    return text.text.clone();
                }
            }
        }
    }
    panic!("the app neither started nor failed");
}

/// The line a missing service's problem is in Rust.
fn rust_line(needed: &str) -> String {
    let hint = match needed {
        PLATFORM => {
            "add_platform() from rayx_services_platform, or register your own dyn PlatformInfoService"
        }
        SUSPENSION => {
            "add_suspension() from rayx_services_suspension, or register your own dyn SuspensionService"
        }
        ASSETS => "add_assets(..) from rayx_services_assets, or register your own dyn AssetService",
        _ => "add_fonts(..) from rayx_services_fonts, or register your own dyn FontService",
    };
    format!("- the app needs `{needed}`, which is not registered: {hint}")
}

#[test]
fn a_rust_root_missing_one_service_fails_naming_it_and_how_to_add_it() {
    for needed in [PLATFORM, SUSPENSION, ASSETS, FONTS] {
        let shown = rust_start(&[needed]);
        assert_eq!(
            shown,
            format!(
                "failed the app's services are not complete:\n{}",
                rust_line(needed)
            ),
            "missing {needed}"
        );
    }
}

#[test]
fn a_rust_root_missing_all_four_fails_once_naming_them_all() {
    let shown = rust_start(&[PLATFORM, SUSPENSION, ASSETS, FONTS]);
    let lines: Vec<String> = [PLATFORM, SUSPENSION, ASSETS, FONTS]
        .into_iter()
        .map(rust_line)
        .collect();
    assert_eq!(
        shown,
        format!(
            "failed the app's services are not complete:\n{}",
            lines.join("\n")
        )
    );
}

#[test]
fn a_rust_root_with_the_four_from_anyone_starts() {
    assert_eq!(rust_start(&[]), "ready");
}

#[test]
fn a_new_rust_builder_is_empty() {
    let mut builder = RayXAppBuilder::new();
    let services = builder.services_mut();
    assert!(!services.contains_service::<dyn PlatformInfoService>());
    assert!(!services.contains_service::<dyn SuspensionService>());
    assert!(!services.contains_service::<dyn AssetService>());
    assert!(!services.contains_service::<dyn FontService>());
    assert!(
        !services
            .contains_service::<dyn rayx_app_nsh::rayx_runtime::services::storage::KeyValueStore>()
    );
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  try {
    await @BUILDER@.start();
    shown.set("done ready");
  } catch (error) {
    shown.set(`done failed ${error.message ?? error}`);
  }
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

/// The line a missing service's problem is in TS.
fn ts_line(needed: &str) -> String {
    let hint = match needed {
        PLATFORM => {
            "builder.addPlatform() from rayx_services_platform's set, or register your own \
             PlatformInfoServiceContract under PlatformInfoService"
        }
        SUSPENSION => {
            "builder.addSuspension() from rayx_services_suspension's set, or register your own \
             SuspensionServiceContract under SuspensionService"
        }
        ASSETS => {
            "builder.addAssets(..) from rayx_services_assets' set, or register your own \
             AssetServiceContract under AssetService"
        }
        _ => {
            "builder.addFonts(..) from rayx_services_fonts' set, or register your own \
             FontServiceContract under FontService"
        }
    };
    format!("- the app needs `{needed}`, which is not registered: {hint}")
}

#[test]
fn a_new_tsx_builder_is_empty_and_its_start_names_all_four_with_ts_hints() {
    let shown = headless::run(
        headless::set(|_| {}),
        &TSX.replace("@BUILDER@", "new RayXAppBuilder()"),
    );
    let lines: Vec<String> = [PLATFORM, SUSPENSION, ASSETS, FONTS]
        .into_iter()
        .map(ts_line)
        .collect();
    assert_eq!(
        shown,
        format!(
            "done failed the app's services are not complete:\n{}",
            lines.join("\n")
        )
    );
}

#[test]
fn a_tsx_app_with_the_four_starts() {
    let shown = headless::run(
        headless::set(|_| {}),
        &TSX.replace("@BUILDER@", "new RayXAppBuilder().addHostFakes()"),
    );
    assert_eq!(shown, "done ready");
}
