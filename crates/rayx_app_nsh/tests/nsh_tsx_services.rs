//! RayX's services for TSX through NSH's engine, headless, in a TSX app with no Rust root:
//! `builder.start()` keeps the platform and suspension services current itself, and the
//! `NavigationService`, `KeyValueStore`, `ApplicationConfigurationService`, `AssetService`,
//! `FontService`, `PlatformInfoService` and `SuspensionService` classes reach their services.

// Loading the compiled TSX app takes `Bundle::bytecode`.
#![allow(unsafe_code, clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use nsh_core::{ApproxTextEngine, MountBatch, MountItem, element::Registry};
use nsh_engine::{App, Assets as EngineAssets, Bundle, Engine, EngineConfig, MountSink};

struct ChannelSink(Sender<MountBatch>);

impl MountSink for ChannelSink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.0.send(batch);
    }
}

/// The files of the app's crate `app`.
struct AppAssets;

impl EngineAssets for AppAssets {
    fn read(&self, uri: &str) -> Result<Vec<u8>, String> {
        match uri {
            "asset:/@app/assets.toml" => Ok(b"files = [\"images/logo.png\"]\n".to_vec()),
            "asset:/@app/images/logo.png" => Ok(b"png".to_vec()),
            _ => Err(format!("{uri} is not bundled")),
        }
    }
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import {
  ApplicationConfigurationService, AssetService, FontService, KeyValueStore, NavigationService,
  PlatformInfoService, RayXAppBuilder, SuspensionService,
} from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder()
      .addNavigation((options) => { options.addRoute("/").addRoute("/projects/{id}"); })
      .addStorage()
      .addApplicationConfiguration()
      .addAssets((options) => { options.package("app"); })
      .addFonts()
      .addPlatform()
      .addSuspension()
      .start();

    const navigation = services.getRequiredService(NavigationService);
    const changes = [];
    navigation.onChange((event) => changes.push(event.to.path));
    lines.push(navigation.navigate("/projects/12").status);
    lines.push(navigation.current().route.path);
    lines.push(navigation.back().route.path);
    // Listeners are called on a later turn.
    await new Promise((resolve) => setTimeout(resolve, 0));
    lines.push(`changes ${changes.join(",")}`);

    const storage = services.getRequiredService(KeyValueStore);
    storage.set("prefs", "lang", "hr");
    storage.setJson("prefs", "theme", { scale: 1.25 });
    lines.push(`${storage.get("prefs", "lang")} ${storage.getJson("prefs", "theme").scale}`);
    lines.push(storage.keys("prefs").join(","));
    lines.push(`${storage.remove("prefs", "lang")} ${storage.get("prefs", "lang")}`);

    const configuration = services.getRequiredService(ApplicationConfigurationService);
    lines.push(`${configuration.layoutDirection()} ${typeof configuration.current()}`);

    lines.push(services.getRequiredService(AssetService).url("images/logo.png"));
    lines.push(services.getRequiredService(FontService).family("serif"));

    const platform = services.getRequiredService(PlatformInfoService);
    lines.push(typeof platform.current().form_factor);
    // Kept current by `start()`: the suspension service heard the app start.
    const suspension = services.getRequiredService(SuspensionService);
    lines.push(suspension.lifecycle().last_reason === null ? "not synced" : "synced");
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

/// The last text shown once one starts with `prefix` (or after 10 s).
fn shown(rx: &Receiver<MountBatch>, prefix: &str) -> String {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut last = String::new();
    while let Ok(batch) = rx.recv_deadline(deadline) {
        for item in &batch.items {
            if let MountItem::Text { text, .. } = item {
                last = text.text.clone();
            }
        }
        if last.starts_with(prefix) {
            break;
        }
    }
    last
}

#[test]
fn nsh_tsx_reaches_the_services_and_start_keeps_the_platform_current() {
    let mut set = Registry::ui_set("test/app");
    rayx_app_nsh::ts::register(&mut set);
    let dir = std::env::temp_dir().join(format!("rayx-app-tsx-services-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("App.tsx");
    std::fs::write(&entry, TSX).unwrap();
    let ui_sets = vec![set];
    let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        development: false,
        app: nsh_bundler::AppInfo::default(),
        asset_root_from: None,
        entry,
        minify: false,
        split: false,
        ui_sets: ui_sets.iter().map(Into::into).collect(),
    })
    .unwrap();
    let described: Vec<_> = ui_sets.iter().map(Into::into).collect();
    let bytecode = nsh_js::compile_bytecode_with(&bundle.code, false, &described).unwrap();
    let (tx, rx) = crossbeam_channel::unbounded();
    // SAFETY: compiled just now by this runtime.
    let app = App::Js(unsafe { Bundle::bytecode(bytecode) });
    let config = EngineConfig {
        ui_sets,
        assets: Some(Arc::new(AppAssets) as Arc<dyn EngineAssets>),
        ..EngineConfig::new(app)
    };
    let _engine = Engine::start(config, || ApproxTextEngine, ChannelSink(tx)).unwrap();
    let last = shown(&rx, "done");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        last,
        "done Success|/projects/12|/|changes /projects/12,/|hr 1.25|lang,theme|true null|ltr object|asset:/@app/images/logo.png|serif|string|synced"
    );
}
