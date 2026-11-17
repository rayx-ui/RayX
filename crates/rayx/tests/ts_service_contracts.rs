//! Every one of RayX's twelve service contracts implemented in TS, through NSH's engine,
//! headless, with every service feature on: a TS class implementing `<Name>Contract`,
//! registered under the service's class (`addSingletonAs(<Name>, () => new …)`, or the startup
//! form, which `start()` initializes), is what Rust resolves as `dyn Trait` (its published state
//! read synchronously, a command called on the engine thread and from another thread) and what
//! `services.getRequiredService(<Name>)` wraps in TS. RayX's own consumers work over it: fonts
//! over a TS asset service, history over TS operations, selection over a TS navigation service,
//! the application configuration over a TS platform service, the input map over a TS key-value
//! store, the i18n formatter over a TS i18n service; and TS implementations of the four the host
//! requires (platform, suspension, assets, fonts) start an app on their own, with RayX's
//! application configuration and i18n service over them.

#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "../../rayx_app_nsh/tests/headless/mod.rs"]
mod headless;

use std::sync::{Arc, Mutex};

use futures::executor::block_on;
use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx::app::services::application_configuration::{
    ApplicationConfiguration, ApplicationConfigurationService, LayoutDirection,
};
use rayx::app::services::assets::{AssetKind, AssetQuery, AssetService};
use rayx::app::services::fonts::FontService;
use rayx::app::services::history::{HistoryEvent, HistoryScopeId, HistoryService};
use rayx::app::services::i18n::{I18nService, MessageFormatter};
use rayx::app::services::input_map::{
    InputBinding, InputMapDocument, InputMapEntry, InputMapService, InputMapSource,
};
use rayx::app::services::navigation::NavigationService;
use rayx::app::services::operations::{
    DomainOperation, OperationApplyResult, OperationOrigin, OperationService,
    OperationTargetHandler, OperationTargetId,
};
use rayx::app::services::platform::{PlatformInfoService, PlatformSnapshot};
use rayx::app::services::selection::{SelectionAreaId, SelectionService, SelectionStatus};
use rayx::app::services::storage::KeyValueStore;
use rayx::app::services::suspension::SuspensionService;
use rayx::app_nsh::ts::ServiceProvider;
use rayx::prelude::Registry;
use serde::{Deserialize, Serialize};

/// The font the TS asset service serves.
const PACIFICO: &[u8] =
    include_bytes!("../../rayx_services_fonts/tests/fixtures/Pacifico-Regular.woff2");

#[rayx::native_module("test/app")]
fn native(registry: &mut Registry) {
    rayx_services_application_configuration::register(registry);
    rayx_services_assets::register(registry);
    rayx_services_fonts::register(registry);
    rayx_services_history::register(registry);
    rayx_services_i18n::register(registry);
    rayx_services_input_map::register(registry);
    rayx_services_navigation::register(registry);
    rayx_services_operations::register(registry);
    rayx_services_platform::register(registry);
    rayx_services_selection::register(registry);
    rayx_services_storage::register(registry);
    rayx_services_suspension::register(registry);
    registry
        .module::<StorageProbe>()
        .module::<PlatformProbe>()
        .module::<SuspensionProbe>()
        .module::<AssetsProbe>()
        .module::<FontsProbe>()
        .module::<NavigationProbe>()
        .module::<I18nProbe>()
        .module::<OperationsProbe>()
        .module::<HistoryProbe>()
        .module::<SelectionProbe>()
        .module::<InputMapProbe>()
        .module::<ConfigurationProbe>()
        // Fakes of the four the host requires, where a test registers no TS one.
        .class_extension::<headless::HostFakes>();
}

/// The app's Rust provider behind TS's.
fn provider(services: &Instance<ServiceProvider>) -> rayx::app::ServiceProvider {
    services.with(|services| services.services().clone())
}

/// The app's `T`.
fn service<T: ?Sized + Send + Sync + 'static>(services: &Instance<ServiceProvider>) -> Arc<T> {
    provider(services).required_service::<T>().unwrap()
}

/// What `call` gives with the app's provider on a thread of its own (not the engine's), awaited
/// without blocking the engine.
async fn elsewhere<T: Send + 'static>(
    services: &Instance<ServiceProvider>,
    call: impl FnOnce(rayx::app::ServiceProvider) -> T + Send + 'static,
) -> T {
    let provider = provider(services);
    let (send, receive) = futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        let _ = send.send(call(provider));
    });
    receive.await.expect("the thread answered")
}

/// The test's lines, split.
fn lines(shown: &str) -> Vec<&str> {
    shown.split('|').collect()
}

// ---------------------------------------------------------------------------------------------
// Storage, and RayX's input map over it.

/// What Rust does with a TS key-value store.
#[module]
struct StorageProbe;

#[module]
impl StorageProbe {
    /// Its constant.
    pub(crate) fn read(services: Instance<ServiceProvider>) -> String {
        let capabilities = service::<dyn KeyValueStore>(&services).capabilities();
        format!(
            "persistent {} secure {}",
            capabilities.persistent, capabilities.secure
        )
    }

    /// Sets `prefs/font` and reads `prefs/theme` on the engine thread.
    pub(crate) async fn here(services: Instance<ServiceProvider>) -> String {
        let store = service::<dyn KeyValueStore>(&services);
        store
            .set("prefs".into(), "font".into(), b"serif".to_vec())
            .await
            .unwrap();
        let theme = store.get("prefs".into(), "theme".into()).await.unwrap();
        String::from_utf8(theme.unwrap_or_default()).unwrap()
    }

    /// Lists `prefs` and sets into a locked bucket from another thread.
    pub(crate) async fn elsewhere(services: Instance<ServiceProvider>) -> String {
        elsewhere(&services, |provider| {
            let store = provider.required_service::<dyn KeyValueStore>().unwrap();
            let keys = block_on(store.keys("prefs".into())).unwrap().join(",");
            let locked = match block_on(store.set("locked".into(), "key".into(), Vec::new())) {
                Ok(()) => "set".to_owned(),
                Err(error) => format!("error {error}"),
            };
            format!("{keys} {locked}")
        })
        .await
    }

    /// RayX's input map, which read the user's document from the store when the app started:
    /// its revision and what `ctrl-s` runs in the editor.
    pub(crate) fn input_map(services: Instance<ServiceProvider>) -> String {
        let input_map = service::<dyn InputMapService>(&services);
        let resolved = input_map.resolve(&["Editor"], &InputBinding::Keyboard("ctrl-s".into()));
        format!(
            "{} {}",
            input_map.revision(),
            resolved.map_or_else(|| "none".to_owned(), |resolved| resolved.action)
        )
    }

    /// Rebinds `save` to `ctrl-shift-s` through RayX's input map, which writes the store: the
    /// new revision, or the error.
    pub(crate) async fn rebind(services: Instance<ServiceProvider>) -> String {
        let input_map = service::<dyn InputMapService>(&services);
        let document = InputMapDocument::new(
            InputMapSource::User,
            input_map.revision(),
            vec![InputMapEntry::keyboard(
                "Editor",
                "ctrl-shift-s",
                "save",
                None,
            )],
        );
        match input_map
            .apply_user_document(input_map.revision(), document)
            .await
        {
            Ok(applied) => applied.revision.to_string(),
            Err(error) => format!("error {error}"),
        }
    }
}

const STORAGE: &str = r#"import { mount, signal, Text } from "nsh";
import { KeyValueStore, RayXAppBuilder, StorageProbe } from "test/app";

const bytes = (text) => [...text].map((c) => c.charCodeAt(0));
const text = (bytes) => String.fromCharCode(...bytes);

class MyStore {
  capabilities = { persistent: true, secure: false };
  values = new Map();
  initialized = false;
  isInitialized() { return this.initialized; }
  async initialize() {
    await new Promise((resolve) => setTimeout(resolve, 10));
    this.values.set("prefs/theme", bytes("dark"));
    this.values.set("rayx.input-map/user-document", bytes(JSON.stringify({
      schema_version: 1, source: "user", revision: 3,
      entries: [{ context: "Editor", binding: { kind: "keyboard", value: "ctrl-s" }, action: "save" }],
    })));
    this.initialized = true;
  }
  async get(bucket, key) { return this.values.get(`${bucket}/${key}`) ?? null; }
  async set(bucket, key, value) {
    if (bucket === "locked") throw new Error("the bucket is locked");
    this.values.set(`${bucket}/${key}`, value);
  }
  async remove(bucket, key) { return this.values.delete(`${bucket}/${key}`); }
  async keys(bucket) {
    return [...this.values.keys()]
      .filter((key) => key.startsWith(`${bucket}/`))
      .map((key) => key.slice(bucket.length + 1))
      .sort();
  }
  async clear(bucket) {
    for (const key of await this.keys(bucket)) this.values.delete(`${bucket}/${key}`);
  }
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  let mine;
  try {
    const services = await new RayXAppBuilder()
      .addStartupSingletonAs(KeyValueStore, () => (mine = new MyStore()))
      .addInputMap()
      .addHostFakes()
      .start();
    lines.push(StorageProbe.read(services));
    lines.push(await StorageProbe.here(services));
    lines.push(await StorageProbe.elsewhere(services));
    // The class over it: RayX's class, the implementation behind.
    const store = services.getRequiredService(KeyValueStore);
    lines.push(`${await store.get("prefs", "font")} ${store.capabilities.persistent}`);
    // RayX's input map over it.
    lines.push(StorageProbe.inputMap(services));
    lines.push(await StorageProbe.rebind(services));
    lines.push(JSON.parse(text(mine.values.get("rayx.input-map/user-document"))).entries[0].binding.value);
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_key_value_store_written_in_ts_is_the_app_s_and_the_input_map_s() {
    let shown = headless::run(registry(), STORAGE);
    let parts = lines(&shown);
    assert_eq!(parts[0], "done persistent true secure false", "{shown}");
    assert_eq!(parts[1], "dark", "{shown}");
    assert!(parts[2].starts_with("font,theme error "), "{shown}");
    assert!(parts[2].contains("the bucket is locked"), "{shown}");
    assert_eq!(parts[3], "serif true", "{shown}");
    assert_eq!(parts[4], "3 save", "{shown}");
    assert_eq!(parts[5], "4", "{shown}");
    assert_eq!(parts[6], "ctrl-shift-s", "{shown}");
    assert_eq!(parts.len(), 7, "{shown}");
}

// ---------------------------------------------------------------------------------------------
// Platform, and RayX's application configuration over it.

/// What Rust reads of a TS platform service.
#[module]
struct PlatformProbe;

#[module]
impl PlatformProbe {
    /// A desktop's snapshot, for the TS implementation to start from.
    pub(crate) fn defaults() -> JsonValue {
        serde_json::to_value(PlatformSnapshot::default()).unwrap()
    }

    /// Its state on the engine thread.
    pub(crate) fn read(services: Instance<ServiceProvider>) -> String {
        let snapshot = service::<dyn PlatformInfoService>(&services)
            .platform()
            .get();
        format!(
            "{} {:?} {}",
            snapshot.platform_name, snapshot.color_scheme, snapshot.reduced_motion
        )
    }

    /// Its state from another thread.
    pub(crate) async fn elsewhere(services: Instance<ServiceProvider>) -> String {
        elsewhere(&services, |provider| {
            let platform = provider
                .required_service::<dyn PlatformInfoService>()
                .unwrap();
            let snapshot = platform.platform().get();
            format!("{} {:?}", snapshot.platform_name, snapshot.color_scheme)
        })
        .await
    }

    /// Whether RayX's application configuration follows its motion setting.
    pub(crate) fn configuration(services: Instance<ServiceProvider>) -> String {
        let configuration = service::<dyn ApplicationConfigurationService>(&services);
        format!(
            "platform reduced {}",
            configuration.effective_motion(None).platform_reduced
        )
    }
}

const PLATFORM: &str = r#"import { mount, signal, Text } from "nsh";
import { ContractState, PlatformInfoService, PlatformProbe, RayXAppBuilder } from "test/app";

class MyPlatform {
  platform = new ContractState(PlatformProbe.defaults());
  isInitialized() { return this.platform.value.platform_name === "ts"; }
  async initialize() {
    await new Promise((resolve) => setTimeout(resolve, 10));
    this.platform.publish({ ...this.platform.value, platform_name: "ts", color_scheme: "Dark" });
  }
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  let mine;
  try {
    const services = await new RayXAppBuilder()
      .addStartupSingletonAs(PlatformInfoService, () => (mine = new MyPlatform()))
      .addApplicationConfiguration()
      .addHostFakes()
      .start();
    lines.push(PlatformProbe.read(services));
    lines.push(await PlatformProbe.elsewhere(services));
    lines.push(PlatformProbe.configuration(services));
    // The class over it: RayX's class, the implementation behind.
    const platform = services.getRequiredService(PlatformInfoService);
    const heard = [];
    platform.onChange((snapshot) => heard.push(String(snapshot.reduced_motion)));
    mine.platform.publish({ ...mine.platform.value, reduced_motion: true });
    lines.push(`${platform.current().color_scheme} ${platform.current().reduced_motion}`);
    for (let i = 0; i < 50 && heard.length === 0; i++) {
      await new Promise((resolve) => setTimeout(resolve, 5));
    }
    lines.push(heard.join(","));
    lines.push(PlatformProbe.read(services));
    // RayX's application configuration over it follows it.
    lines.push(PlatformProbe.configuration(services));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_platform_service_written_in_ts_is_the_app_s_and_the_configuration_s() {
    let shown = headless::run(registry(), PLATFORM);
    assert_eq!(
        shown,
        "done ts Dark false|ts Dark|platform reduced false|Dark true|true|ts Dark true\
         |platform reduced true"
    );
}

// ---------------------------------------------------------------------------------------------
// Suspension.

/// What the suspension service's subscriber heard.
static SUSPENSION_HEARD: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// What Rust does with a TS suspension service.
#[module]
struct SuspensionProbe;

#[module]
impl SuspensionProbe {
    /// Its state.
    pub(crate) fn read(services: Instance<ServiceProvider>) -> String {
        let snapshot = service::<dyn SuspensionService>(&services)
            .suspension()
            .get();
        format!(
            "{:?} {:?} {}",
            snapshot.lifecycle, snapshot.visibility, snapshot.asks_before_quit
        )
    }

    /// Hears its events.
    pub(crate) fn follow(services: Instance<ServiceProvider>) {
        service::<dyn SuspensionService>(&services)
            .events()
            .subscribe(|event| {
                SUSPENSION_HEARD
                    .lock()
                    .unwrap()
                    .push(format!("{:?}", event.kind));
            })
            .detach();
    }

    /// What it heard.
    pub(crate) fn heard() -> String {
        SUSPENSION_HEARD.lock().unwrap().join(",")
    }

    /// Asks before quitting, on the engine thread.
    pub(crate) async fn here(services: Instance<ServiceProvider>) -> String {
        match service::<dyn SuspensionService>(&services)
            .ask_before_quit(true)
            .await
        {
            Ok(()) => "asking".to_owned(),
            Err(error) => format!("error {error}"),
        }
    }

    /// Quits twice from another thread.
    pub(crate) async fn elsewhere(services: Instance<ServiceProvider>) -> String {
        elsewhere(&services, |provider| {
            let suspension = provider
                .required_service::<dyn SuspensionService>()
                .unwrap();
            [block_on(suspension.quit()), block_on(suspension.quit())]
                .into_iter()
                .map(|quit| match quit {
                    Ok(()) => "quit".to_owned(),
                    Err(error) => format!("error {error}"),
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .await
    }
}

const SUSPENSION: &str = r#"import { mount, signal, Text } from "nsh";
import {
  ContractEvents, ContractState, RayXAppBuilder, SuspensionProbe, SuspensionService,
} from "test/app";

const event = (kind, reason) => ({ kind, reason, timestamp_unix_millis: null, window: null });

class MySuspension {
  suspension = new ContractState({
    lifecycle: "NotStarted", activation: "Unknown", visibility: "Unknown",
    updated_at_unix_millis: null, last_reason: null, window: null, wakes: 0, asks_before_quit: false,
  });
  events = new ContractEvents();
  asked = [];
  isInitialized() { return this.suspension.value.lifecycle === "Running"; }
  async initialize() {
    this.suspension.publish({ ...this.suspension.value, lifecycle: "Running", visibility: "Foreground", last_reason: "HostStarted" });
    this.events.publish(event("Started", "HostStarted"));
  }
  async quit() {
    this.asked.push("quit");
    if (this.asked.filter((asked) => asked === "quit").length > 1) throw new Error("quitting already");
  }
  async askBeforeQuit(ask) {
    this.asked.push(`ask ${ask}`);
    this.suspension.publish({ ...this.suspension.value, asks_before_quit: ask });
  }
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  let mine;
  try {
    const services = await new RayXAppBuilder()
      .addStartupSingletonAs(SuspensionService, () => (mine = new MySuspension()))
      .addHostFakes()
      .start();
    lines.push(SuspensionProbe.read(services));
    SuspensionProbe.follow(services);
    lines.push(await SuspensionProbe.here(services));
    lines.push(SuspensionProbe.read(services));
    lines.push(await SuspensionProbe.elsewhere(services));
    // The class over it: RayX's class, the implementation behind.
    const suspension = services.getRequiredService(SuspensionService);
    lines.push(suspension.lifecycle().lifecycle);
    const requests = [];
    const stop = suspension.askBeforeQuit(() => requests.push("asked"));
    mine.events.publish(event("QuitRequested", "UserAskedToQuit"));
    for (let i = 0; i < 50 && requests.length === 0; i++) {
      await new Promise((resolve) => setTimeout(resolve, 5));
    }
    stop();
    for (let i = 0; i < 50 && !mine.asked.includes("ask false"); i++) {
      await new Promise((resolve) => setTimeout(resolve, 5));
    }
    lines.push(requests.join(","));
    lines.push(mine.asked.join(","));
    lines.push(SuspensionProbe.heard());
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_suspension_service_written_in_ts_is_the_app_s() {
    let shown = headless::run(registry(), SUSPENSION);
    let parts = lines(&shown);
    assert_eq!(parts[0], "done Running Foreground false", "{shown}");
    assert_eq!(parts[1], "asking", "{shown}");
    assert_eq!(parts[2], "Running Foreground true", "{shown}");
    assert!(parts[3].starts_with("quit error "), "{shown}");
    assert!(parts[3].contains("quitting already"), "{shown}");
    assert_eq!(parts[4], "Running", "{shown}");
    assert_eq!(parts[5], "asked", "{shown}");
    assert_eq!(parts[6], "ask true,quit,quit,ask true,ask false", "{shown}");
    assert_eq!(parts[7], "QuitRequested", "{shown}");
    assert_eq!(parts.len(), 8, "{shown}");
}

// ---------------------------------------------------------------------------------------------
// Assets, and RayX's fonts over them.

/// What Rust does with a TS asset service.
#[module]
struct AssetsProbe;

#[module]
impl AssetsProbe {
    /// The font the TS asset service serves.
    pub(crate) fn pacifico() -> Vec<u8> {
        PACIFICO.to_vec()
    }

    /// Lookups in its catalog.
    pub(crate) fn read(services: Instance<ServiceProvider>) -> String {
        let assets = service::<dyn AssetService>(&services);
        format!(
            "{} {}",
            assets.uri("data/hello.txt").unwrap(),
            assets
                .query(&AssetQuery::kind(AssetKind::Font))
                .unwrap()
                .len()
        )
    }

    /// Reads a text and clears the cache on the engine thread.
    pub(crate) async fn here(services: Instance<ServiceProvider>) -> String {
        let assets = service::<dyn AssetService>(&services);
        let text = assets.load_text("data/hello.txt".into()).await.unwrap();
        assets.clear_cache();
        text
    }

    /// Reads the font's bytes and a missing asset from another thread.
    pub(crate) async fn elsewhere(services: Instance<ServiceProvider>) -> String {
        elsewhere(&services, |provider| {
            let assets = provider.required_service::<dyn AssetService>().unwrap();
            let font = block_on(assets.load_bytes("@app/fonts/Pacifico-Regular.woff2".into()))
                .unwrap()
                .len();
            let missing = match block_on(assets.load_text("nowhere.txt".into())) {
                Ok(text) => text,
                Err(error) => format!("error {error}"),
            };
            format!("{} {missing}", font == PACIFICO.len())
        })
        .await
    }

    /// The fonts RayX's font service registered from it when the app started.
    pub(crate) fn fonts(services: Instance<ServiceProvider>) -> String {
        let fonts = service::<dyn FontService>(&services);
        let faces: Vec<String> = fonts
            .registered_fonts()
            .into_iter()
            .map(|face| format!("{} {}", face.family, face.logical_path.unwrap_or_default()))
            .collect();
        format!("{} {:?}", faces.join(","), fonts.diagnostics())
    }
}

const ASSETS: &str = r#"import { mount, signal, Text } from "nsh";
import { AssetService, AssetsProbe, ContractState, RayXAppBuilder } from "test/app";

const entry = (path, kind, mime_type) => ({
  package: "app", path, kind, mime_type, scales: [], tags: [], locale: null, theme: null,
  platform: null, title: null, description: null, width: null, height: null, custom: null,
});

class MyAssets {
  catalog = new ContractState(null);
  cleared = 0;
  isInitialized() { return this.catalog.value !== null; }
  async initialize() { await this.loadCatalog(); }
  async loadCatalog() {
    await new Promise((resolve) => setTimeout(resolve, 10));
    this.catalog.publish({
      packages: ["app"],
      entries: {
        "@app/data/hello.txt": entry("data/hello.txt", "text", "text/plain"),
        "@app/fonts/Pacifico-Regular.woff2": entry("fonts/Pacifico-Regular.woff2", "font", "font/woff2"),
      },
      diagnostics: [],
    });
  }
  async loadBytes(path) {
    // The probe's `Uint8Array`, as is (the contract takes it, or a `number[]`).
    if (path === "@app/fonts/Pacifico-Regular.woff2") return AssetsProbe.pacifico();
    throw new Error(`no asset ${path}`);
  }
  async loadText(path) {
    if (path === "data/hello.txt" || path === "@app/data/hello.txt") return "hello from TS";
    throw new Error(`no asset ${path}`);
  }
  clearCache() { this.cleared += 1; }
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  let mine;
  try {
    const services = await new RayXAppBuilder()
      .addStartupSingletonAs(AssetService, () => (mine = new MyAssets()))
      .addFonts()
      .addHostFakes()
      .start();
    lines.push(AssetsProbe.read(services));
    lines.push(await AssetsProbe.here(services));
    lines.push(await AssetsProbe.elsewhere(services));
    lines.push(String(mine.cleared));
    // The class over it: RayX's class, the implementation behind.
    lines.push(services.getRequiredService(AssetService).url("data/hello.txt"));
    // RayX's font service registered the catalog's font from it when the app started.
    lines.push(AssetsProbe.fonts(services));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn an_asset_service_written_in_ts_is_the_app_s_and_the_fonts() {
    let shown = headless::run(registry(), ASSETS);
    let parts = lines(&shown);
    assert_eq!(parts[0], "done asset:/@app/data/hello.txt 1", "{shown}");
    assert_eq!(parts[1], "hello from TS", "{shown}");
    assert!(parts[2].starts_with("true error "), "{shown}");
    assert!(parts[2].contains("no asset nowhere.txt"), "{shown}");
    assert_eq!(parts[3], "1", "{shown}");
    assert_eq!(parts[4], "asset:/@app/data/hello.txt", "{shown}");
    assert_eq!(
        parts[5], "Pacifico @app/fonts/Pacifico-Regular.woff2 []",
        "{shown}"
    );
    assert_eq!(parts.len(), 6, "{shown}");
}

// ---------------------------------------------------------------------------------------------
// Fonts.

/// What Rust does with a TS font service.
#[module]
struct FontsProbe;

#[module]
impl FontsProbe {
    /// Its state: the alias `script` and the faces.
    pub(crate) fn read(services: Instance<ServiceProvider>) -> String {
        let fonts = service::<dyn FontService>(&services);
        let faces: Vec<String> = fonts
            .registered_fonts()
            .into_iter()
            .map(|face| face.family)
            .collect();
        format!("{} [{}]", fonts.font_family("script"), faces.join(","))
    }

    /// Registers three bytes on the engine thread.
    pub(crate) async fn here(services: Instance<ServiceProvider>) -> String {
        match service::<dyn FontService>(&services)
            .register_font(vec![1, 2, 3])
            .await
        {
            Ok(family) => family,
            Err(error) => format!("error {error}"),
        }
    }

    /// Registers five bytes, then none, from another thread.
    pub(crate) async fn elsewhere(services: Instance<ServiceProvider>) -> String {
        elsewhere(&services, |provider| {
            let fonts = provider.required_service::<dyn FontService>().unwrap();
            [vec![1; 5], Vec::new()]
                .into_iter()
                .map(|data| match block_on(fonts.register_font(data)) {
                    Ok(family) => family,
                    Err(error) => format!("error {error}"),
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .await
    }
}

const FONTS: &str = r#"import { mount, signal, Text } from "nsh";
import { ContractState, FontService, FontsProbe, RayXAppBuilder } from "test/app";

class MyFonts {
  fonts = new ContractState({
    faces: [],
    aliases: [{ alias: "script", requested_family: "Pacifico", resolved_family: "Pacifico", glyph_fallback_families: ["Noto Color Emoji"] }],
    default_alias: "script",
    diagnostics: [],
  });
  isInitialized() { return this.fonts.value.faces.length > 0; }
  async initialize() { await this.registerFontAssets(); }
  add(face) { this.fonts.publish({ ...this.fonts.value, faces: [...this.fonts.value.faces, face] }); }
  async registerFont(data) {
    if (data.length === 0) throw new Error("no font data");
    const family = `Font${data.length}`;
    this.add({ logical_path: null, family });
    return family;
  }
  async registerFontAssets() {
    this.add({ logical_path: "@app/fonts/Pacifico-Regular.woff2", family: "Pacifico" });
  }
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder()
      .addStartupSingletonAs(FontService, () => new MyFonts())
      .addHostFakes()
      .start();
    lines.push(FontsProbe.read(services));
    lines.push(await FontsProbe.here(services));
    lines.push(await FontsProbe.elsewhere(services));
    lines.push(FontsProbe.read(services));
    // The class over it: RayX's class, the implementation behind.
    const fonts = services.getRequiredService(FontService);
    lines.push(`${fonts.family("script")} ${fonts.aliases().length}`);
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_font_service_written_in_ts_is_the_app_s() {
    let shown = headless::run(registry(), FONTS);
    let parts = lines(&shown);
    assert_eq!(
        parts[0], "done Pacifico, Noto Color Emoji [Pacifico]",
        "{shown}"
    );
    assert_eq!(parts[1], "Font3", "{shown}");
    assert!(parts[2].starts_with("Font5 error "), "{shown}");
    assert!(parts[2].contains("no font data"), "{shown}");
    assert_eq!(
        parts[3], "Pacifico, Noto Color Emoji [Pacifico,Font3,Font5]",
        "{shown}"
    );
    assert_eq!(parts[4], "Pacifico, Noto Color Emoji 1", "{shown}");
    assert_eq!(parts.len(), 5, "{shown}");
}

// ---------------------------------------------------------------------------------------------
// Navigation, and RayX's selection over it.

/// What Rust does with a TS navigation service.
#[module]
struct NavigationProbe;

#[module]
impl NavigationProbe {
    /// Its constants and its state's location.
    pub(crate) fn read(services: Instance<ServiceProvider>) -> String {
        let navigation = service::<dyn NavigationService>(&services);
        format!(
            "{} {} {:?} {}",
            navigation.root_region_name(),
            navigation.url_region(),
            navigation.default_history_mode(),
            navigation.location()
        )
    }

    /// Navigates to `uri` on the engine thread: the route's path, or the error.
    pub(crate) async fn here(services: Instance<ServiceProvider>, uri: String) -> String {
        match service::<dyn NavigationService>(&services)
            .navigate_to(uri)
            .await
        {
            Ok(result) => format!("{:?} {}", result.status, result.route.path),
            Err(error) => format!("error {error}"),
        }
    }

    /// Navigates to `uri` from another thread: the route's path, or the error.
    pub(crate) async fn elsewhere(services: Instance<ServiceProvider>, uri: String) -> String {
        elsewhere(&services, move |provider| {
            let navigation = provider
                .required_service::<dyn NavigationService>()
                .unwrap();
            match block_on(navigation.navigate_to(uri)) {
                Ok(result) => format!("{:?} {}", result.status, result.route.path),
                Err(error) => format!("error {error}"),
            }
        })
        .await
    }

    /// Activates the selection area `area` in RayX's selection service.
    pub(crate) async fn select(services: Instance<ServiceProvider>, area: u64) {
        let status = SelectionStatus {
            participant_count: 1,
            selected_character_count: 4,
            is_collapsed: false,
            is_selecting: false,
        };
        service::<dyn SelectionService>(&services)
            .activate(SelectionAreaId::from_raw(area), status)
            .await
            .unwrap();
    }

    /// RayX's selection service's active area.
    pub(crate) fn selected(services: Instance<ServiceProvider>) -> String {
        format!(
            "{:?}",
            service::<dyn SelectionService>(&services)
                .active_area()
                .map(SelectionAreaId::get)
        )
    }
}

const NAVIGATION: &str = r#"import { mount, signal, Text } from "nsh";
import {
  ContractEvents, ContractState, NavigationProbe, NavigationService, RayXAppBuilder,
} from "test/app";

const route = (path) => ({
  uri: path, base_uri: "app://", path, query: "", fragment: "", location: path,
  segments: path.split("/"), params: {}, query_params: {}, passed_parameters: null,
  component: null, pattern: path, prewarm: false,
});

class MyNavigation {
  navigation = new ContractState({ root_region: "main", regions: {}, location: "/start" });
  events = new ContractEvents();
  rootRegionName = "main";
  urlRegion = "main";
  defaultHistoryMode = "Replace";
  asked = [];
  async navigate(request) {
    if (request.args.uri === "/nowhere") throw new Error("no such route");
    this.asked.push(request.args.uri);
    this.navigation.publish({ ...this.navigation.value, location: request.args.uri });
    this.events.publish({ kind: "RouteChanged", region: request.region, from: null, to: route(request.args.uri) });
    return { region: request.region, direction: "New", status: "Success", route: route(request.args.uri) };
  }
  async back(region) { throw new Error("no history"); }
  async forward(region) { throw new Error("no history"); }
  async refresh(region) { throw new Error("no history"); }
  async goToEntry(region, index) { throw new Error("no history"); }
  async ensureRegion(region) {}
  async closeRegion(region) { return false; }
  async removeRegion(region) { return false; }
  async applyLocation(location) { return []; }
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  let mine;
  try {
    const services = await new RayXAppBuilder()
      .addSingletonAs(NavigationService, () => (mine = new MyNavigation()))
      .addSelection()
      .addHostFakes()
      .start();
    lines.push(NavigationProbe.read(services));
    // RayX's selection over it: a route change clears it.
    await NavigationProbe.select(services, 2);
    lines.push(NavigationProbe.selected(services));
    lines.push(await NavigationProbe.here(services, "/projects/1"));
    lines.push(NavigationProbe.selected(services));
    lines.push(await NavigationProbe.elsewhere(services, "/projects/2"));
    lines.push(await NavigationProbe.elsewhere(services, "/nowhere"));
    lines.push(NavigationProbe.read(services));
    // The class over it: RayX's class, the implementation behind.
    lines.push(services.getRequiredService(NavigationService).current() === null ? "null" : "a route");
    lines.push(mine.asked.join(","));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_navigation_service_written_in_ts_is_the_app_s_and_the_selection_s() {
    let shown = headless::run(registry(), NAVIGATION);
    let parts = lines(&shown);
    assert_eq!(parts[0], "done main main Replace /start", "{shown}");
    assert_eq!(parts[1], "Some(2)", "{shown}");
    assert_eq!(parts[2], "Success /projects/1", "{shown}");
    assert_eq!(parts[3], "None", "{shown}");
    assert_eq!(parts[4], "Success /projects/2", "{shown}");
    assert!(parts[5].starts_with("error "), "{shown}");
    assert!(parts[5].contains("no such route"), "{shown}");
    assert_eq!(parts[6], "main main Replace /projects/2", "{shown}");
    assert_eq!(parts[7], "null", "{shown}");
    assert_eq!(parts[8], "/projects/1,/projects/2", "{shown}");
    assert_eq!(parts.len(), 9, "{shown}");
}

// ---------------------------------------------------------------------------------------------
// I18n, and RayX's message formatter over it.

/// What Rust does with a TS i18n service.
#[module]
struct I18nProbe;

#[module]
impl I18nProbe {
    /// Its state, and what RayX's formatter over it formats.
    pub(crate) fn read(services: Instance<ServiceProvider>) -> String {
        let i18n = service::<dyn I18nService>(&services);
        let formatter = service::<dyn MessageFormatter>(&services);
        format!(
            "{} {} {}",
            i18n.language(),
            i18n.time_zone(),
            formatter.t("title")
        )
    }

    /// Sets the language to `language` on the engine thread.
    pub(crate) async fn here(services: Instance<ServiceProvider>, language: String) -> String {
        match service::<dyn I18nService>(&services)
            .set_language(language)
            .await
        {
            Ok(()) => "set".to_owned(),
            Err(error) => format!("error {error}"),
        }
    }

    /// Sets the time zone, then an unknown language, from another thread.
    pub(crate) async fn elsewhere(services: Instance<ServiceProvider>) -> String {
        elsewhere(&services, |provider| {
            let i18n = provider.required_service::<dyn I18nService>().unwrap();
            [
                block_on(i18n.set_time_zone("Europe/Zagreb".into())),
                block_on(i18n.set_language("xx".into())),
            ]
            .into_iter()
            .map(|set| match set {
                Ok(()) => "set".to_owned(),
                Err(error) => format!("error {error}"),
            })
            .collect::<Vec<_>>()
            .join(" ")
        })
        .await
    }
}

const I18N: &str = r#"import { mount, signal, Text } from "nsh";
import { ContractState, I18nProbe, I18nService, RayXAppBuilder } from "test/app";

class MyI18n {
  i18n = new ContractState({ language: "en", locale: "en", default_language: "en", time_zone: "UTC", version: 1 });
  isInitialized() { return (this.i18n.value.messages ?? []).length > 0; }
  async initialize() { await this.loadMessages(); }
  change(change) {
    const now = this.i18n.value;
    this.i18n.publish({ ...now, ...change, version: now.version + 1 });
  }
  async loadMessages() {
    await new Promise((resolve) => setTimeout(resolve, 10));
    this.change({
      available_languages: ["en", "hr"],
      messages: [
        { language: "en", messages: { title: "Projects", hello: "Hello, {$name}!" } },
        { language: "hr", messages: { title: "Projekti", hello: "Bok, {$name}!" } },
      ],
    });
  }
  async setLanguage(language) {
    if (!this.i18n.value.available_languages.includes(language)) throw new Error(`no language ${language}`);
    this.change({ language, locale: language, chosen_language: language });
  }
  async followPlatform() {}
  async addResource(language, namespace, json) {}
  async setTimeZone(timeZone) { this.change({ time_zone: timeZone }); }
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder()
      .addStartupSingletonAs(I18nService, () => new MyI18n())
      .addMessageFormatter()
      .addHostFakes()
      .start();
    lines.push(I18nProbe.read(services));
    lines.push(await I18nProbe.here(services, "hr"));
    lines.push(await I18nProbe.elsewhere(services));
    lines.push(I18nProbe.read(services));
    // The class over it: RayX's class, the implementation behind, formatting with RayX's
    // formatter over it.
    const i18n = services.getRequiredService(I18nService);
    lines.push(`${i18n.locale()} ${i18n.timeZone()} ${i18n.t("hello", { name: "Ana" })}`);
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn an_i18n_service_written_in_ts_is_the_app_s_and_the_formatter_s() {
    let shown = headless::run(registry(), I18N);
    let parts = lines(&shown);
    assert_eq!(parts[0], "done en UTC Projects", "{shown}");
    assert_eq!(parts[1], "set", "{shown}");
    assert!(parts[2].starts_with("set error "), "{shown}");
    assert!(parts[2].contains("no language xx"), "{shown}");
    assert_eq!(parts[3], "hr Europe/Zagreb Projekti", "{shown}");
    assert_eq!(parts[4], "hr Europe/Zagreb Bok, Ana!", "{shown}");
    assert_eq!(parts.len(), 5, "{shown}");
}

// ---------------------------------------------------------------------------------------------
// Operations, and RayX's history over them.

/// The value the TS operation service changes through the Rust handler.
static VALUE: Mutex<u32> = Mutex::new(0);

#[derive(Clone, Deserialize, Serialize)]
struct Set(u32);

impl DomainOperation for Set {
    const KIND: &'static str = "contracts.set";
    const VERSION: u32 = 1;
}

/// The Rust target the TS operation service applies operations to.
struct Target;

impl OperationTargetHandler<Set> for Target {
    fn apply_atomically(
        &self,
        operation: &Set,
        _origin: &OperationOrigin,
    ) -> Result<OperationApplyResult<Set>, String> {
        let mut value = VALUE.lock().unwrap();
        let before = std::mem::replace(&mut *value, operation.0);
        Ok(OperationApplyResult::applied(
            Set(before),
            ["value"],
            u64::from(before),
            u64::from(operation.0),
        ))
    }
}

fn doc() -> OperationTargetId {
    OperationTargetId::new("doc").unwrap()
}

fn doc_scope() -> HistoryScopeId {
    HistoryScopeId::new("doc-scope").unwrap()
}

/// What Rust does with a TS operation service.
#[module]
struct OperationsProbe;

#[module]
impl OperationsProbe {
    /// Its constant and state.
    pub(crate) fn read(services: Instance<ServiceProvider>) -> String {
        let operations = service::<dyn OperationService>(&services);
        let targets: Vec<String> = operations
            .targets()
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect();
        format!("{} [{}]", operations.instance_id(), targets.join(","))
    }

    /// On the engine thread: registers the Rust target `doc`, binds it to RayX's history and
    /// sets it to `value`: the receipt's status and the value, or the error.
    pub(crate) async fn here(services: Instance<ServiceProvider>, value: u32) -> String {
        let operations = service::<dyn OperationService>(&services);
        let history = service::<dyn HistoryService>(&services);
        operations
            .register_handler::<Set, _>(doc(), Arc::new(Target))
            .await
            .unwrap();
        history.bind_target_scope(doc(), doc_scope()).await.unwrap();
        match operations
            .submit(&doc(), Set(value), OperationOrigin::LocalUser)
            .await
        {
            Ok(receipt) => format!("{:?} {}", receipt.status, *VALUE.lock().unwrap()),
            Err(error) => format!("error {error}"),
        }
    }

    /// From another thread: a transaction begun and committed, and an operation to a target it
    /// does not have.
    pub(crate) async fn elsewhere(services: Instance<ServiceProvider>) -> String {
        elsewhere(&services, |provider| {
            let operations = provider.required_service::<dyn OperationService>().unwrap();
            let transaction = block_on(operations.begin()).unwrap();
            block_on(operations.commit(transaction)).unwrap();
            let missing = OperationTargetId::new("missing").unwrap();
            let submitted =
                match block_on(operations.submit(&missing, Set(1), OperationOrigin::System)) {
                    Ok(_) => "applied".to_owned(),
                    Err(error) => format!("error {error}"),
                };
            format!("began {transaction} {submitted}")
        })
        .await
    }

    /// RayX's history over it: whether it can undo, then undoes (back through the TS service)
    /// and says whether it can redo, and the value.
    pub(crate) async fn undo(services: Instance<ServiceProvider>) -> String {
        let history = service::<dyn HistoryService>(&services);
        let could = history.can_undo(&doc_scope());
        let undone = match history.undo(doc_scope()).await {
            Ok(undone) => undone.to_string(),
            Err(error) => format!("error {error}"),
        };
        format!(
            "{could} {undone} {} {}",
            history.can_redo(&doc_scope()),
            *VALUE.lock().unwrap()
        )
    }
}

const OPERATIONS: &str = r#"import { mount, signal, Text } from "nsh";
import {
  ContractEvents, ContractState, OperationService, OperationTarget, OperationsProbe, RayXAppBuilder,
} from "test/app";

class MyOperations {
  operations = new ContractState({ targets: [], recent_transactions: [], recent_failure_codes: [] });
  events = new ContractEvents();
  instanceId = 77;
  targets = new Map();
  next = 0;
  asked = [];
  publishTargets() {
    this.operations.publish({ targets: [...this.targets.keys()], recent_transactions: [], recent_failure_codes: [] });
  }
  async registerTarget(targetId, handle) {
    this.targets.set(targetId, OperationTarget.of(handle));
    this.publishTargets();
  }
  async releaseTarget(targetId) {
    const released = this.targets.delete(targetId);
    this.publishTargets();
    return released;
  }
  async begin() { this.asked.push("begin"); return ++this.next; }
  async commit(transactionId) { this.asked.push(`commit ${transactionId}`); }
  async abort(transactionId) { this.asked.push(`abort ${transactionId}`); }
  async submitRecord(record, origin, transactionId) {
    const target = this.targets.get(record.target_id);
    if (!target) throw new Error(`no target ${record.target_id}`);
    this.asked.push(`submit ${typeof origin === "string" ? origin : "remote"}`);
    const result = target.apply(record, origin);
    const receipt = { transaction_id: transactionId ?? ++this.next, target_id: record.target_id, forward: record, origin, ...result };
    this.events.publish({ Committed: receipt });
    return receipt;
  }
  async publishRemoteRecord(record, peerId, beforeRevision, afterRevision, affectedIds) {
    throw new Error("no peers");
  }
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  let mine;
  try {
    // RayX's history first: `start()` makes the TS service before the history's factory runs.
    const services = await new RayXAppBuilder()
      .addHistory()
      .addSingletonAs(OperationService, () => (mine = new MyOperations()))
      .addHostFakes()
      .start();
    lines.push(OperationsProbe.read(services));
    lines.push(await OperationsProbe.here(services, 4));
    lines.push(await OperationsProbe.elsewhere(services));
    lines.push(OperationsProbe.read(services));
    // RayX's history over it recorded the user's operation and undoes it through it.
    lines.push(await OperationsProbe.undo(services));
    // The class over it: RayX's class, the implementation behind.
    const operations = services.getRequiredService(OperationService);
    lines.push(`${operations.instanceId} ${operations.targets.join(",")}`);
    lines.push(mine.asked.join(","));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn an_operation_service_written_in_ts_is_the_app_s_and_the_history_s() {
    let shown = headless::run(registry(), OPERATIONS);
    let parts = lines(&shown);
    assert_eq!(parts[0], "done 77 []", "{shown}");
    assert_eq!(parts[1], "Applied 4", "{shown}");
    assert!(parts[2].starts_with("began 2 error "), "{shown}");
    assert!(parts[2].contains("no target missing"), "{shown}");
    assert_eq!(parts[3], "77 [doc]", "{shown}");
    assert_eq!(parts[4], "true true true 0", "{shown}");
    assert_eq!(parts[5], "77 doc", "{shown}");
    assert_eq!(
        parts[6], "submit LocalUser,begin,commit 2,begin,submit HistoryUndo,commit 3",
        "{shown}"
    );
    assert_eq!(parts.len(), 7, "{shown}");
}

// ---------------------------------------------------------------------------------------------
// History.

/// What the history's subscriber heard.
static HISTORY_HEARD: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// What Rust does with a TS history service.
#[module]
struct HistoryProbe;

fn scope(id: &str) -> HistoryScopeId {
    HistoryScopeId::new(id).unwrap()
}

#[module]
impl HistoryProbe {
    /// Its state for the scope `doc`.
    pub(crate) fn read(services: Instance<ServiceProvider>) -> String {
        let history = service::<dyn HistoryService>(&services);
        format!(
            "{} {} {:?}",
            history.can_undo(&scope("doc")),
            history.can_redo(&scope("doc")),
            history.stack_depths(&scope("doc"))
        )
    }

    /// Hears its events.
    pub(crate) fn follow(services: Instance<ServiceProvider>) {
        service::<dyn HistoryService>(&services)
            .events()
            .subscribe(|event| {
                let heard = match event {
                    HistoryEvent::UndoCompleted(scope) => format!("undone {}", scope.as_str()),
                    HistoryEvent::RedoCompleted(scope) => format!("redone {}", scope.as_str()),
                    other => format!("{other:?}"),
                };
                HISTORY_HEARD.lock().unwrap().push(heard);
            })
            .detach();
    }

    /// What it heard.
    pub(crate) fn heard() -> String {
        HISTORY_HEARD.lock().unwrap().join(",")
    }

    /// Binds a target and undoes `doc` on the engine thread.
    pub(crate) async fn here(services: Instance<ServiceProvider>) -> String {
        let history = service::<dyn HistoryService>(&services);
        history
            .bind_target_scope(OperationTargetId::new("editor").unwrap(), scope("doc"))
            .await
            .unwrap();
        match history.undo(scope("doc")).await {
            Ok(undone) => undone.to_string(),
            Err(error) => format!("error {error}"),
        }
    }

    /// Redoes `doc`, then `nothing`, from another thread.
    pub(crate) async fn elsewhere(services: Instance<ServiceProvider>) -> String {
        elsewhere(&services, |provider| {
            let history = provider.required_service::<dyn HistoryService>().unwrap();
            [scope("doc"), scope("nothing")]
                .into_iter()
                .map(|scope| match block_on(history.redo(scope)) {
                    Ok(redone) => redone.to_string(),
                    Err(error) => format!("error {error}"),
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .await
    }
}

const HISTORY: &str = r#"import { mount, signal, Text } from "nsh";
import {
  ContractEvents, ContractState, HistoryProbe, HistoryService, RayXAppBuilder,
} from "test/app";

class MyHistory {
  history = new ContractState({ scopes: {}, unavailable_entry_count: 0, recent_failure_codes: [] });
  events = new ContractEvents();
  stacks = { doc: { undo: 2, redo: 0 } };
  asked = [];
  constructor() { this.publish(); }
  publish() {
    const scopes = {};
    for (const [id, { undo, redo }] of Object.entries(this.stacks)) {
      scopes[id] = { undo_depth: undo, redo_depth: redo, revision: undo * 10 + redo, entry_limit: null };
    }
    this.history.publish({ scopes, unavailable_entry_count: 0, recent_failure_codes: [] });
  }
  async bindTargetScope(targetId, scopeId) { this.asked.push(`bind ${targetId} ${scopeId}`); }
  async undo(scopeId) {
    const stack = this.stacks[scopeId];
    if (!stack) throw new Error(`no scope ${scopeId}`);
    stack.undo -= 1; stack.redo += 1;
    this.publish();
    this.events.publish({ UndoCompleted: scopeId });
    return true;
  }
  async redo(scopeId) {
    const stack = this.stacks[scopeId];
    if (!stack) throw new Error(`no scope ${scopeId}`);
    stack.undo += 1; stack.redo -= 1;
    this.publish();
    this.events.publish({ RedoCompleted: scopeId });
    return true;
  }
  async clearScope(scopeId) { delete this.stacks[scopeId]; this.publish(); }
  async setScopeEntryLimit(scopeId, limit) {}
  async retainedRecords(scopeId) { return []; }
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  let mine;
  try {
    const services = await new RayXAppBuilder()
      .addSingletonAs(HistoryService, () => (mine = new MyHistory()))
      .addHostFakes()
      .start();
    lines.push(HistoryProbe.read(services));
    HistoryProbe.follow(services);
    lines.push(await HistoryProbe.here(services));
    lines.push(HistoryProbe.read(services));
    lines.push(await HistoryProbe.elsewhere(services));
    lines.push(HistoryProbe.read(services));
    // The class over it: RayX's class, the implementation behind.
    const history = services.getRequiredService(HistoryService);
    lines.push(`${history.canUndo("doc")} ${history.canRedo("doc")} ${history.depths("doc").join(",")}`);
    lines.push(mine.asked.join(","));
    lines.push(HistoryProbe.heard());
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_history_service_written_in_ts_is_the_app_s() {
    let shown = headless::run(registry(), HISTORY);
    let parts = lines(&shown);
    assert_eq!(parts[0], "done true false (2, 0)", "{shown}");
    assert_eq!(parts[1], "true", "{shown}");
    assert_eq!(parts[2], "true true (1, 1)", "{shown}");
    assert!(parts[3].starts_with("true error "), "{shown}");
    assert!(parts[3].contains("no scope nothing"), "{shown}");
    assert_eq!(parts[4], "true false (2, 0)", "{shown}");
    assert_eq!(parts[5], "true false 2,0", "{shown}");
    assert_eq!(parts[6], "bind editor doc", "{shown}");
    assert_eq!(parts[7], "undone doc,redone doc", "{shown}");
    assert_eq!(parts.len(), 8, "{shown}");
}

// ---------------------------------------------------------------------------------------------
// Selection.

/// What Rust does with a TS selection service.
#[module]
struct SelectionProbe;

fn status(characters: usize) -> SelectionStatus {
    SelectionStatus {
        participant_count: 1,
        selected_character_count: characters,
        is_collapsed: false,
        is_selecting: false,
    }
}

#[module]
impl SelectionProbe {
    /// Its active area and status.
    pub(crate) fn read(services: Instance<ServiceProvider>) -> String {
        let selection = service::<dyn SelectionService>(&services);
        format!(
            "{:?} {}",
            selection.active_area().map(SelectionAreaId::get),
            selection.status().selected_character_count
        )
    }

    /// Activates the area 2 with 6 characters on the engine thread.
    pub(crate) async fn here(services: Instance<ServiceProvider>) -> String {
        match service::<dyn SelectionService>(&services)
            .activate(SelectionAreaId::from_raw(2), status(6))
            .await
        {
            Ok(()) => "ok".to_owned(),
            Err(error) => format!("error {error}"),
        }
    }

    /// Activates the area 0, then 3, from another thread.
    pub(crate) async fn elsewhere(services: Instance<ServiceProvider>) -> String {
        elsewhere(&services, |provider| {
            let selection = provider.required_service::<dyn SelectionService>().unwrap();
            [0, 3]
                .into_iter()
                .map(|area| {
                    match block_on(selection.activate(SelectionAreaId::from_raw(area), status(9))) {
                        Ok(()) => "ok".to_owned(),
                        Err(error) => format!("error {error}"),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .await
    }
}

const SELECTION: &str = r#"import { mount, signal, Text } from "nsh";
import { ContractState, RayXAppBuilder, SelectionProbe, SelectionService } from "test/app";

const empty = { participant_count: 0, selected_character_count: 0, is_collapsed: false, is_selecting: false };
const state = (area, status) => ({
  active_area: area, status, navigation_revision: 0, active_area_revision: 0, status_revision: 0,
});

class MySelection {
  selection = new ContractState(state(null, empty));
  asked = [];
  async publishStatus(activeArea, status) { this.asked.push(`publish ${activeArea}`); }
  async activate(activeArea, status) {
    if (activeArea === 0) throw new Error("no area 0");
    this.asked.push(`activate ${activeArea}`);
    this.selection.publish(state(activeArea, status));
  }
  async deactivate(activeArea) { this.asked.push(`deactivate ${activeArea}`); }
  async clear() { this.selection.publish(state(null, empty)); }
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  let mine;
  try {
    const services = await new RayXAppBuilder()
      .addSingletonAs(SelectionService, () => (mine = new MySelection()))
      .addHostFakes()
      .start();
    lines.push(SelectionProbe.read(services));
    lines.push(await SelectionProbe.here(services));
    lines.push(SelectionProbe.read(services));
    lines.push(await SelectionProbe.elsewhere(services));
    lines.push(SelectionProbe.read(services));
    // The class over it: RayX's class, the implementation behind.
    lines.push(String(services.getRequiredService(SelectionService).activeArea()));
    lines.push(mine.asked.join(","));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_selection_service_written_in_ts_is_the_app_s() {
    let shown = headless::run(registry(), SELECTION);
    let parts = lines(&shown);
    assert_eq!(parts[0], "done None 0", "{shown}");
    assert_eq!(parts[1], "ok", "{shown}");
    assert_eq!(parts[2], "Some(2) 6", "{shown}");
    assert!(parts[3].starts_with("error "), "{shown}");
    assert!(parts[3].contains("no area 0"), "{shown}");
    assert!(parts[3].ends_with(" ok"), "{shown}");
    assert_eq!(parts[4], "Some(3) 9", "{shown}");
    assert_eq!(parts[5], "3", "{shown}");
    assert_eq!(parts[6], "activate 2,activate 3", "{shown}");
    assert_eq!(parts.len(), 7, "{shown}");
}

// ---------------------------------------------------------------------------------------------
// Input map.

/// What Rust does with a TS input map.
#[module]
struct InputMapProbe;

#[module]
impl InputMapProbe {
    /// Its state: the revision and what `ctrl-s` runs in the editor.
    pub(crate) fn read(services: Instance<ServiceProvider>) -> String {
        let input_map = service::<dyn InputMapService>(&services);
        let resolved = input_map.resolve(&["Editor"], &InputBinding::Keyboard("ctrl-s".into()));
        format!(
            "{} {} {}",
            input_map.revision(),
            resolved.map_or_else(|| "none".to_owned(), |resolved| resolved.action),
            input_map.bindings_for("save").len()
        )
    }

    /// Loads the user's document on the engine thread: its revision.
    pub(crate) async fn here(services: Instance<ServiceProvider>) -> String {
        match service::<dyn InputMapService>(&services).load().await {
            Ok(document) => format!("loaded {}", document.revision),
            Err(error) => format!("error {error}"),
        }
    }

    /// Unbinds `ctrl-s` for the user from another thread, then applies at a stale revision.
    pub(crate) async fn elsewhere(services: Instance<ServiceProvider>) -> String {
        elsewhere(&services, |provider| {
            let input_map = provider.required_service::<dyn InputMapService>().unwrap();
            let mut entry = InputMapEntry::keyboard("Editor", "ctrl-s", "save", None);
            entry.unbind = true;
            let document = InputMapDocument::new(InputMapSource::User, 0, vec![entry]);
            [
                block_on(input_map.apply_user_document(0, document.clone())),
                block_on(input_map.apply_user_document(0, document)),
            ]
            .into_iter()
            .map(|applied| match applied {
                Ok(document) => format!("applied {}", document.revision),
                Err(error) => format!("error {error}"),
            })
            .collect::<Vec<_>>()
            .join(" ")
        })
        .await
    }
}

const INPUT_MAP: &str = r#"import { mount, signal, Text } from "nsh";
import { ContractState, InputMapProbe, InputMapService, RayXAppBuilder } from "test/app";

const document = (source, revision, entries) => ({ schema_version: 1, source, revision, entries });
const save = { context: "Editor", binding: { kind: "keyboard", value: "ctrl-s" }, action: "save" };

class MyInputMap {
  inputMap = new ContractState({});
  loaded = false;
  isInitialized() { return this.loaded; }
  async initialize() {
    this.inputMap.publish({ application: document("application", 0, [save]), user: document("user", 0, []), revision: 0 });
    this.loaded = true;
  }
  async load() { return this.inputMap.value.user; }
  async applyUserDocument(expectedRevision, candidate) {
    if (expectedRevision !== this.inputMap.value.revision) {
      throw new Error(`expected revision ${expectedRevision}, the map is at ${this.inputMap.value.revision}`);
    }
    const user = { ...candidate, revision: expectedRevision + 1 };
    this.inputMap.publish({ ...this.inputMap.value, user, revision: user.revision });
    return user;
  }
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder()
      .addStartupSingletonAs(InputMapService, () => new MyInputMap())
      .addHostFakes()
      .start();
    lines.push(InputMapProbe.read(services));
    lines.push(await InputMapProbe.here(services));
    lines.push(await InputMapProbe.elsewhere(services));
    lines.push(InputMapProbe.read(services));
    // The class over it: RayX's class, the implementation behind.
    const inputMap = services.getRequiredService(InputMapService);
    lines.push(`${inputMap.revision()} ${inputMap.bindingsFor("save").length}`);
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn an_input_map_written_in_ts_is_the_app_s() {
    let shown = headless::run(registry(), INPUT_MAP);
    let parts = lines(&shown);
    assert_eq!(parts[0], "done 0 save 1", "{shown}");
    assert_eq!(parts[1], "loaded 0", "{shown}");
    assert!(parts[2].starts_with("applied 1 error "), "{shown}");
    assert!(
        parts[2].contains("expected revision 0, the map is at 1"),
        "{shown}"
    );
    assert_eq!(parts[3], "1 none 0", "{shown}");
    assert_eq!(parts[4], "1 0", "{shown}");
    assert_eq!(parts.len(), 5, "{shown}");
}

// ---------------------------------------------------------------------------------------------
// Application configuration.

/// What Rust does with a TS application configuration service.
#[module]
struct ConfigurationProbe;

/// The options with `name`, laid out right to left.
fn named(name: &str) -> ApplicationConfiguration {
    let mut options = ApplicationConfiguration {
        name: name.to_owned(),
        ..ApplicationConfiguration::default()
    };
    options.layout.direction = LayoutDirection::Rtl;
    options
}

#[module]
impl ConfigurationProbe {
    /// Its state.
    pub(crate) fn read(services: Instance<ServiceProvider>) -> String {
        let configuration = service::<dyn ApplicationConfigurationService>(&services);
        format!(
            "{} {:?} {}",
            configuration.options().name,
            configuration.layout_direction(),
            configuration.snapshot().revision
        )
    }

    /// Renames the app on the engine thread: whether it changed, or the error.
    pub(crate) async fn here(services: Instance<ServiceProvider>) -> String {
        match service::<dyn ApplicationConfigurationService>(&services)
            .replace_options(named("Renamed"))
            .await
        {
            Ok(changed) => changed.to_string(),
            Err(error) => format!("error {error}"),
        }
    }

    /// Renames the app, then names it nothing, from another thread.
    pub(crate) async fn elsewhere(services: Instance<ServiceProvider>) -> String {
        elsewhere(&services, |provider| {
            let configuration = provider
                .required_service::<dyn ApplicationConfigurationService>()
                .unwrap();
            ["Elsewhere", ""]
                .into_iter()
                .map(
                    |name| match block_on(configuration.replace_options(named(name))) {
                        Ok(changed) => changed.to_string(),
                        Err(error) => format!("error {error}"),
                    },
                )
                .collect::<Vec<_>>()
                .join(" ")
        })
        .await
    }
}

const CONFIGURATION: &str = r#"import { mount, signal, Text } from "nsh";
import {
  ApplicationConfigurationService, ConfigurationProbe, ContractState, RayXAppBuilder,
} from "test/app";

class MyConfiguration {
  configuration = new ContractState({ options: { name: "TS App", layout: { direction: "rtl" } }, revision: 1 });
  async replaceOptions(options) {
    if (!options.name) throw new Error("the app needs a name");
    const now = this.configuration.value;
    if (options.name === now.options.name) return false;
    this.configuration.publish({ ...now, options, revision: now.revision + 1 });
    return true;
  }
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder()
      .addSingletonAs(ApplicationConfigurationService, () => new MyConfiguration())
      .addHostFakes()
      .start();
    lines.push(ConfigurationProbe.read(services));
    lines.push(await ConfigurationProbe.here(services));
    lines.push(await ConfigurationProbe.elsewhere(services));
    lines.push(ConfigurationProbe.read(services));
    // The class over it: RayX's class, the implementation behind.
    const configuration = services.getRequiredService(ApplicationConfigurationService);
    lines.push(`${configuration.current().name} ${configuration.layoutDirection()}`);
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn an_application_configuration_service_written_in_ts_is_the_app_s() {
    let shown = headless::run(registry(), CONFIGURATION);
    let parts = lines(&shown);
    assert_eq!(parts[0], "done TS App Rtl 1", "{shown}");
    assert_eq!(parts[1], "true", "{shown}");
    assert!(parts[2].starts_with("true error "), "{shown}");
    assert!(parts[2].contains("the app needs a name"), "{shown}");
    assert_eq!(parts[3], "Elsewhere Rtl 3", "{shown}");
    assert_eq!(parts[4], "Elsewhere rtl", "{shown}");
    assert_eq!(parts.len(), 5, "{shown}");
}

// ---------------------------------------------------------------------------------------------
// The host's four, all in TS.

const HOST: &str = r#"import { mount, signal, Text } from "nsh";
import {
  AssetService, ContractEvents, ContractState, FontService, PlatformInfoService,
  PlatformProbe, RayXAppBuilder, SuspensionService,
} from "test/app";

class MyPlatform { platform = new ContractState({ ...PlatformProbe.defaults(), platform_name: "ts" }); }

class MySuspension {
  suspension = new ContractState({
    lifecycle: "Running", activation: "Active", visibility: "Foreground",
    updated_at_unix_millis: null, last_reason: null, window: null, wakes: 0, asks_before_quit: false,
  });
  events = new ContractEvents();
  async quit() {}
  async askBeforeQuit(ask) {}
}

class MyAssets {
  catalog = new ContractState({ packages: ["app"], entries: {}, diagnostics: [] });
  async loadCatalog() {}
  async loadBytes(path) { throw new Error(`no asset ${path}`); }
  async loadText(path) { throw new Error(`no asset ${path}`); }
  clearCache() {}
}

class MyFonts {
  fonts = new ContractState({ faces: [], aliases: [], default_alias: null, diagnostics: [] });
  async registerFont(data) { return "Any"; }
  async registerFontAssets() {}
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder()
      .addSingletonAs(PlatformInfoService, () => new MyPlatform())
      .addSingletonAs(SuspensionService, () => new MySuspension())
      .addSingletonAs(AssetService, () => new MyAssets())
      .addSingletonAs(FontService, () => new MyFonts())
      // RayX's services over them.
      .addApplicationConfiguration()
      .addI18n((options) => options.defaultLanguage("en"))
      .start();
    lines.push("started");
    lines.push(services.getRequiredService(PlatformInfoService).current().platform_name);
    lines.push(services.getRequiredService(SuspensionService).lifecycle().lifecycle);
    lines.push(services.getRequiredService(FontService).family("Any"));
  } catch (error) {
    lines.push(`threw ${error.message ?? error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn the_four_the_host_requires_written_in_ts_start_the_app() {
    assert_eq!(
        headless::run(registry(), HOST),
        "done started|ts|Running|Any"
    );
}
