//! Loading application-layer bindings from an asset JSON (`InputMapOptions::application_asset`)
//! through `dyn AssetService`: startup ordering after `AssetService`, full and concise JSON
//! documents, `mod-*` and `when: "always"` normalization, user-layer precedence, code-level
//! overrides, and failure handling.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures::FutureExt as _;
use futures::executor::block_on;
use futures::future::{BoxFuture, LocalBoxFuture};
use rayx_app::StateCell;
use rayx_app::di::{
    AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupInitializationState, StartupServiceType,
};
use rayx_app::services::assets::{AssetCatalog, AssetError, AssetManifest, AssetService};
use rayx_app::services::input_map::*;
use rayx_app::services::storage::KeyValueStore;
use rayx_app::testing::{FakeAssets, FakeKeyValueStore};
use rayx_app::{AppBuilder as _, RayXAppBuilder};
use rayx_services_input_map::{
    INPUT_MAP_STORAGE_BUCKET, INPUT_MAP_USER_DOCUMENT_KEY, InputMapAppBuilderExt as _,
};
use serde_json::json;

fn key(sequence: &str) -> InputBinding {
    InputBinding::Keyboard(sequence.into())
}

fn catalog_with(package: &str, path: &str) -> AssetCatalog {
    let mut builder = AssetCatalog::builder();
    builder.add_manifest(
        package,
        &AssetManifest {
            files: vec![path.to_owned()],
            ..AssetManifest::default()
        },
    );
    builder.build()
}

/// An `AssetService` whose catalog only becomes ready during its own startup initialization,
/// proving `InputMapService` waits for `dyn AssetService` before reading `application_asset`.
struct DelayedStartupAssets {
    catalog: StateCell<Option<AssetCatalog>>,
    ready: AtomicBool,
    path: String,
    content: String,
    initialization: StartupInitializationState,
}

impl DelayedStartupAssets {
    fn new(path: &str, content: impl Into<String>) -> Arc<Self> {
        Arc::new(Self {
            catalog: StateCell::new(None),
            ready: AtomicBool::new(false),
            path: path.to_owned(),
            content: content.into(),
            initialization: StartupInitializationState::default(),
        })
    }
}

impl AssetService for DelayedStartupAssets {
    fn catalog(&self) -> &StateCell<Option<AssetCatalog>> {
        &self.catalog
    }

    fn load_catalog(&self) -> BoxFuture<'_, Result<(), AssetError>> {
        self.catalog.publish(Some(catalog_with("app", &self.path)));
        self.ready.store(true, Ordering::SeqCst);
        futures::future::ready(Ok(())).boxed()
    }

    fn load_bytes(&self, path: String) -> BoxFuture<'_, Result<Vec<u8>, AssetError>> {
        if !self.ready.load(Ordering::SeqCst) {
            return futures::future::ready(Err(AssetError::NotLoaded)).boxed();
        }
        let result = if path == self.path || path == format!("@app/{}", self.path) {
            Ok(self.content.as_bytes().to_vec())
        } else {
            Err(AssetError::NotFound(path))
        };
        futures::future::ready(result).boxed()
    }

    fn load_text(&self, path: String) -> BoxFuture<'_, Result<String, AssetError>> {
        if !self.ready.load(Ordering::SeqCst) {
            return futures::future::ready(Err(AssetError::NotLoaded)).boxed();
        }
        let result = if path == self.path || path == format!("@app/{}", self.path) {
            Ok(self.content.clone())
        } else {
            Err(AssetError::NotFound(path))
        };
        futures::future::ready(result).boxed()
    }

    fn clear_cache(&self) {}
}

impl AsyncInitializable for DelayedStartupAssets {
    fn is_initialized(&self) -> bool {
        self.initialization.is_initialized()
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        self.catalog.publish(Some(catalog_with("app", &self.path)));
        self.ready.store(true, Ordering::SeqCst);
        self.initialization.mark_initialized();
        futures::future::ready(Ok(())).boxed_local()
    }
}

impl StartupInitializable for DelayedStartupAssets {
    fn startup_service_type(&self) -> Option<StartupServiceType> {
        Some(StartupServiceType::of::<dyn AssetService>())
    }
}

#[test]
fn loads_full_application_asset_after_asset_service_and_user_overrides_win() {
    let keybindings = json!({
        "schema_version": 1,
        "source": "application",
        "revision": 0,
        "entries": [
            {
                "context": "",
                "binding": { "kind": "keyboard", "value": "Mod-Shift-Z" },
                "action": "edit.redo"
            },
            {
                "context": "code-editor",
                "binding": { "kind": "keyboard", "value": "mod-s" },
                "action": "code.save",
                "when": "always"
            },
            {
                "context": "canvas",
                "binding": { "kind": "keyboard", "value": "v" },
                "action": "canvas.tool.move"
            }
        ]
    })
    .to_string();

    let store = FakeKeyValueStore::new();
    // Seed a persisted user override in storage before startup.
    let persisted_user = InputMapDocument::new(
        InputMapSource::User,
        2,
        vec![
            InputMapEntry::keyboard("canvas", "v", "canvas.customMove", None),
            InputMapEntry {
                unbind: true,
                ..InputMapEntry::keyboard("", "mod-shift-z", "edit.redo", None)
            },
        ],
    );
    block_on(store.set(
        INPUT_MAP_STORAGE_BUCKET.into(),
        INPUT_MAP_USER_DOCUMENT_KEY.into(),
        serde_json::to_vec(&persisted_user).unwrap(),
    ))
    .unwrap();

    // Register InputMap BEFORE the startup AssetService to verify dependency ordering, not
    // registration order.
    let assets = DelayedStartupAssets::new("keybindings.json", keybindings);
    let mut builder = RayXAppBuilder::new();
    builder
        .services_mut()
        .add_singleton_as::<dyn KeyValueStore>(store as Arc<dyn KeyValueStore>);
    let mut builder = builder.add_input_map(|options| {
        options.application_asset("keybindings.json");
    });
    builder
        .services_mut()
        .add_startup_singleton_for::<dyn AssetService, _>(assets);

    let services = builder.build().unwrap();
    services.validate_options_on_start().unwrap();
    block_on(
        services
            .startup_registry()
            .initialize(StartupInitializationContext::new(services.clone())),
    )
    .unwrap();

    let input_map = services.required_service::<dyn InputMapService>().unwrap();
    assert_eq!(input_map.revision(), 2);

    // Application layer loaded and normalized `Mod-Shift-Z` -> `mod-shift-z`.
    let app_doc = input_map.application_document();
    assert_eq!(app_doc.source, InputMapSource::Application);
    assert_eq!(app_doc.entries.len(), 3);
    assert_eq!(app_doc.entries[0].binding, key("mod-shift-z"));

    // `when: "always"` and `mod-*` platform resolution work from the asset.
    let save_win = input_map
        .resolve_for_platform(&["code-editor"], &key("ctrl-s"), false)
        .unwrap();
    assert_eq!(save_win.action, "code.save");
    assert_eq!(save_win.when, InputWhen::Always);
    assert_eq!(save_win.source, InputMapSource::Application);
    let save_mac = input_map
        .resolve_for_platform(&["code-editor"], &key("cmd-s"), true)
        .unwrap();
    assert_eq!(save_mac.action, "code.save");

    // User layer overrides and unbinds take precedence over asset-loaded defaults.
    let move_action = input_map.resolve(&["canvas"], &key("v")).unwrap();
    assert_eq!(move_action.action, "canvas.customMove");
    assert_eq!(move_action.source, InputMapSource::User);
    assert_eq!(input_map.resolve(&[], &key("mod-shift-z")), None);

    // Explicit `load()` keeps the application asset intact and reloads the user document.
    let reloaded = block_on(input_map.load()).unwrap();
    assert_eq!(reloaded.revision, 2);
    assert_eq!(input_map.application_document().entries.len(), 3);
}

#[test]
fn loads_concise_entries_json_and_merges_code_bindings_on_top() {
    let concise = json!({
        "entries": [
            {
                "context": "",
                "binding": { "kind": "keyboard", "value": "mod-k" },
                "action": "search.open"
            },
            {
                "context": "editor",
                "binding": { "kind": "keyboard", "value": "mod-s" },
                "action": "asset.save"
            }
        ]
    })
    .to_string();

    let assets = FakeAssets::new(catalog_with("app", "keybindings.json"))
        .file("@app/keybindings.json", concise);
    let mut builder = RayXAppBuilder::new();
    builder
        .services_mut()
        .add_singleton_as::<dyn KeyValueStore>(FakeKeyValueStore::new() as Arc<dyn KeyValueStore>);
    builder
        .services_mut()
        .add_singleton_as::<dyn AssetService>(assets as Arc<dyn AssetService>);

    let services = builder
        .add_input_map(|options| {
            options
                .application_asset("keybindings.json")
                .bind_key("editor", "mod-s", "code.save")
                .bind_key("editor", "mod-p", "code.print");
        })
        .build()
        .unwrap();
    services.validate_options_on_start().unwrap();
    block_on(
        services
            .startup_registry()
            .initialize(StartupInitializationContext::new(services.clone())),
    )
    .unwrap();

    let input_map = services.required_service::<dyn InputMapService>().unwrap();
    assert_eq!(
        input_map.resolve(&[], &key("mod-k")).unwrap().action,
        "search.open"
    );
    assert_eq!(
        input_map
            .resolve(&["editor"], &key("mod-s"))
            .unwrap()
            .action,
        "code.save"
    );
    assert_eq!(
        input_map
            .resolve(&["editor"], &key("mod-p"))
            .unwrap()
            .action,
        "code.print"
    );
}

#[test]
fn rejects_missing_asset_service_bad_source_duplicates_and_blank_path() {
    // Blank path fails option validation on start.
    let mut builder = RayXAppBuilder::new();
    builder
        .services_mut()
        .add_singleton_as::<dyn KeyValueStore>(FakeKeyValueStore::new() as Arc<dyn KeyValueStore>);
    let services = builder
        .add_input_map(|options| {
            options.application_asset("   ");
        })
        .build()
        .unwrap();
    assert!(services.validate_options_on_start().is_err());

    // Missing AssetService fails startup.
    let mut builder = RayXAppBuilder::new();
    builder
        .services_mut()
        .add_singleton_as::<dyn KeyValueStore>(FakeKeyValueStore::new() as Arc<dyn KeyValueStore>);
    let services = builder
        .add_input_map(|options| {
            options.application_asset("keybindings.json");
        })
        .build()
        .unwrap();
    let result = block_on(
        services
            .startup_registry()
            .initialize(StartupInitializationContext::new(services.clone())),
    );
    assert!(result.is_err());

    // Wrong source ("user") in asset JSON fails startup.
    let wrong_source = json!({
        "schema_version": 1,
        "source": "user",
        "revision": 0,
        "entries": []
    })
    .to_string();
    let assets = FakeAssets::new(catalog_with("app", "keybindings.json"))
        .file("@app/keybindings.json", wrong_source);
    let mut builder = RayXAppBuilder::new();
    builder
        .services_mut()
        .add_singleton_as::<dyn KeyValueStore>(FakeKeyValueStore::new() as Arc<dyn KeyValueStore>);
    builder
        .services_mut()
        .add_singleton_as::<dyn AssetService>(assets as Arc<dyn AssetService>);
    let services = builder
        .add_input_map(|options| {
            options.application_asset("keybindings.json");
        })
        .build()
        .unwrap();
    assert!(
        block_on(
            services
                .startup_registry()
                .initialize(StartupInitializationContext::new(services.clone()))
        )
        .is_err()
    );

    // Duplicate bindings in asset JSON fail startup.
    let duplicate = json!({
        "entries": [
            { "context": "", "binding": { "kind": "keyboard", "value": "mod-z" }, "action": "a" },
            { "context": "", "binding": { "kind": "keyboard", "value": "Platform-Z" }, "action": "b" }
        ]
    })
    .to_string();
    let assets = FakeAssets::new(catalog_with("app", "keybindings.json"))
        .file("@app/keybindings.json", duplicate);
    let mut builder = RayXAppBuilder::new();
    builder
        .services_mut()
        .add_singleton_as::<dyn KeyValueStore>(FakeKeyValueStore::new() as Arc<dyn KeyValueStore>);
    builder
        .services_mut()
        .add_singleton_as::<dyn AssetService>(assets as Arc<dyn AssetService>);
    let services = builder
        .add_input_map(|options| {
            options.application_asset("keybindings.json");
        })
        .build()
        .unwrap();
    assert!(
        block_on(
            services
                .startup_registry()
                .initialize(StartupInitializationContext::new(services.clone()))
        )
        .is_err()
    );
}
