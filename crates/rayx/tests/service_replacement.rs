//! Replacing RayX's implementations from Rust, with every service feature on: an app that
//! registers its own implementation of a core trait in place of RayX's gets it used by every
//! consumer of the trait: RayX's other services (fonts and i18n read the app's own assets, i18n
//! and the application configuration follow its own platform, history undoes through its own
//! operations and keeps its journals in its own store, selection follows its own navigation, the
//! input map keeps the user's document in its own store, RayX's formatter formats its own i18n),
//! the host's requirement (its own platform, suspension, assets and fonts start the app) and the
//! service's TS class (which wraps the app's own implementation). The app's own implementations
//! are `rayx_app::testing`'s fakes.

#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "../../rayx_app_nsh/tests/headless/mod.rs"]
mod headless;

use std::sync::{Arc, Mutex, OnceLock};

use futures::FutureExt as _;
use futures::executor::block_on;
use futures::future::BoxFuture;
use nsh_core::element::Registry;
use nsh_core::prelude::*;
use rayx::app::services::application_configuration::ApplicationConfigurationService;
use rayx::app::services::assets::{AssetCatalog, AssetService};
use rayx::app::services::fonts::{FontService, FontState};
use rayx::app::services::history::{HistoryScopeId, HistoryService};
use rayx::app::services::input_map::{
    InputMapDocument, InputMapEntry, InputMapService, InputMapSource,
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
use rayx::app::testing::{
    FakeAssets, FakeFonts, FakeI18n, FakeKeyValueStore, FakeNavigation, FakeOperations,
    FakePlatform, FakeSuspension,
};
use rayx::app_nsh::ts::ServiceProvider as TsServiceProvider;
use rayx::prelude::*;
use rayx::services::fonts::testing::{FontRegistrar, add_fonts_with};
use rayx::services::input_map::INPUT_MAP_STORAGE_BUCKET;
use serde::{Deserialize, Serialize};

/// Whether `a` and `b` are the same object.
fn same<T: ?Sized, U: ?Sized>(a: &Arc<T>, b: &Arc<U>) -> bool {
    std::ptr::addr_eq(Arc::as_ptr(a), Arc::as_ptr(b))
}

/// `builder` built, its options validated and its startup services started.
fn start(builder: RayXAppBuilder) -> ServiceProvider {
    let services = builder.build().unwrap();
    services.validate_options_on_start().unwrap();
    block_on(
        services
            .startup_registry()
            .initialize(StartupInitializationContext::new(services.clone())),
    )
    .unwrap();
    services
}

/// The app's own assets: a font and the English and Croatian messages.
fn own_assets() -> Arc<FakeAssets> {
    let mut catalog = AssetCatalog::builder();
    catalog
        .add_manifest_text(
            "app",
            "files = [\"fonts/Inter.woff2\", \"locales/en.json\", \"locales/hr.json\"]",
        )
        .unwrap();
    FakeAssets::new(catalog.build())
        .file("@app/fonts/Inter.woff2", "family:Inter")
        .file("@app/locales/en.json", r#"{ "hello": "Hello, {$name}!" }"#)
        .file("@app/locales/hr.json", r#"{ "hello": "Bok, {$name}!" }"#)
}

/// Reads "family:<name>" data as a font of that family, in place of NSH.
struct Registrar;

impl FontRegistrar for Registrar {
    fn register(&self, data: Vec<u8>) -> BoxFuture<'static, Result<String, String>> {
        let text = String::from_utf8(data).unwrap_or_default();
        let family = text.strip_prefix("family:").map(str::to_owned);
        async move { family.ok_or_else(|| "not a font".to_owned()) }.boxed()
    }
}

#[test]
fn fonts_and_i18n_read_the_app_s_own_assets_and_follow_its_own_platform() {
    let assets = own_assets();
    let platform = FakePlatform::new(PlatformSnapshot::default());
    let mut builder = RayXAppBuilder::new().add_i18n(|options| {
        options.default_language("en");
    });
    builder = add_fonts_with(builder, |_| {}, Arc::new(Registrar));
    let services = builder.services_mut();
    services.add_singleton_as::<dyn AssetService>(Arc::clone(&assets) as Arc<dyn AssetService>);
    services.add_singleton_as::<dyn PlatformInfoService>(
        Arc::clone(&platform) as Arc<dyn PlatformInfoService>
    );
    let services = start(builder);

    assert!(same(
        &services.required_service::<dyn AssetService>().unwrap(),
        &assets
    ));
    let fonts = services.required_service::<dyn FontService>().unwrap();
    assert_eq!(fonts.registered_fonts()[0].family, "Inter");
    let i18n = services.required_service::<dyn I18nService>().unwrap();
    let mut languages = i18n.available_languages();
    languages.sort();
    assert_eq!(languages, ["en", "hr"]);
    let formatter = services.required_service::<dyn MessageFormatter>().unwrap();
    let ana = Args::new().with("name", "Ana");
    assert_eq!(formatter.format("hello", &ana), "Hello, Ana!");
    let mut loads = assets.loads();
    loads.sort();
    assert_eq!(
        loads,
        [
            "@app/fonts/Inter.woff2",
            "@app/locales/en.json",
            "@app/locales/hr.json"
        ]
    );

    platform.modify(|snapshot| snapshot.locale.languages = vec!["hr-HR".into()]);
    assert_eq!(i18n.language(), "hr-HR");
    assert_eq!(formatter.format("hello", &ana), "Bok, Ana!");
}

#[test]
fn the_configuration_follows_the_app_s_own_platform() {
    let platform = FakePlatform::new(PlatformSnapshot::default());
    let mut builder = RayXAppBuilder::new().add_application_configuration(|_| {});
    builder
        .services_mut()
        .add_singleton_as::<dyn PlatformInfoService>(
            Arc::clone(&platform) as Arc<dyn PlatformInfoService>
        );
    let services = start(builder);
    let configuration = services
        .required_service::<dyn ApplicationConfigurationService>()
        .unwrap();
    assert!(!configuration.snapshot().platform_reduced_motion);
    platform.modify(|snapshot| snapshot.reduced_motion = true);
    assert!(configuration.snapshot().platform_reduced_motion);
    assert!(configuration.effective_motion(None).is_reduced());
}

#[derive(Clone, Deserialize, Serialize)]
struct Set(u64);

impl DomainOperation for Set {
    const KIND: &'static str = "replacement.set";
    const VERSION: u32 = 1;
}

#[derive(Default)]
struct Value(Mutex<u64>);

impl OperationTargetHandler<Set> for Value {
    fn apply_atomically(
        &self,
        operation: &Set,
        _origin: &OperationOrigin,
    ) -> Result<OperationApplyResult<Set>, String> {
        let before = std::mem::replace(&mut *self.0.lock().unwrap(), operation.0);
        Ok(OperationApplyResult::applied(
            Set(before),
            ["value"],
            before,
            operation.0,
        ))
    }
}

#[test]
fn history_undoes_through_the_app_s_own_operations_into_its_own_store() {
    let operations = FakeOperations::new();
    let store = FakeKeyValueStore::new();
    let mut builder = RayXAppBuilder::new().add_persistent_history();
    let services = builder.services_mut();
    services.add_singleton_as::<dyn OperationService>(
        Arc::clone(&operations) as Arc<dyn OperationService>
    );
    services.add_singleton_as::<dyn KeyValueStore>(Arc::clone(&store) as Arc<dyn KeyValueStore>);
    let services = start(builder);
    let history = services.required_service::<dyn HistoryService>().unwrap();

    let operations = operations as Arc<dyn OperationService>;
    let value = Arc::new(Value::default());
    let target = OperationTargetId::new("doc").unwrap();
    let scope = HistoryScopeId::new("editor").unwrap();
    block_on(operations.register_handler(target.clone(), Arc::clone(&value))).unwrap();
    block_on(history.bind_target_scope(target.clone(), scope.clone())).unwrap();
    block_on(operations.submit(&target, Set(7), OperationOrigin::LocalUser)).unwrap();
    assert!(history.can_undo(&scope));
    assert!(block_on(history.undo(scope.clone())).unwrap());
    assert_eq!(*value.0.lock().unwrap(), 0);
    assert!(history.can_redo(&scope));
    let journals = block_on(store.keys(rayx::services::history::HISTORY_BUCKET.into())).unwrap();
    assert_eq!(journals, ["editor"]);
}

#[test]
fn selection_follows_the_app_s_own_navigation() {
    let mut builder = RayXAppBuilder::new().add_selection();
    builder
        .services_mut()
        .add_singleton_as::<dyn NavigationService>(
            FakeNavigation::new() as Arc<dyn NavigationService>
        );
    let services = start(builder);
    let navigation = services
        .required_service::<dyn NavigationService>()
        .unwrap();
    let selection = services.required_service::<dyn SelectionService>().unwrap();
    let status = SelectionStatus {
        participant_count: 1,
        selected_character_count: 3,
        ..SelectionStatus::default()
    };
    block_on(selection.activate(SelectionAreaId::from_raw(1), status)).unwrap();
    assert!(selection.status().has_selection());
    block_on(navigation.navigate_to("/elsewhere")).unwrap();
    assert!(!selection.status().has_selection());
}

#[test]
fn the_input_map_keeps_the_user_s_document_in_the_app_s_own_store() {
    let store = FakeKeyValueStore::new();
    let mut builder = RayXAppBuilder::new().add_input_map(|_| {});
    builder
        .services_mut()
        .add_singleton_as::<dyn KeyValueStore>(Arc::clone(&store) as Arc<dyn KeyValueStore>);
    let services = start(builder);
    let input_map = services.required_service::<dyn InputMapService>().unwrap();
    let document = InputMapDocument::new(
        InputMapSource::User,
        0,
        vec![InputMapEntry::keyboard("", "cmd-k", "app::search", None)],
    );
    block_on(input_map.apply_user_document(input_map.revision(), document)).unwrap();
    let kept = block_on(store.keys(INPUT_MAP_STORAGE_BUCKET.into())).unwrap();
    assert!(!kept.is_empty());
}

#[test]
fn rayx_s_formatter_formats_the_app_s_own_i18n() {
    let i18n = FakeI18n::in_language("en");
    block_on(i18n.add_resource("en".into(), None, r#"{ "hello": "Hi, {$name}!" }"#.into()))
        .unwrap();
    block_on(i18n.add_resource("hr".into(), None, r#"{ "hello": "Bok, {$name}!" }"#.into()))
        .unwrap();
    let mut builder = RayXAppBuilder::new().add_message_formatter();
    builder
        .services_mut()
        .add_singleton_as::<dyn I18nService>(Arc::clone(&i18n) as Arc<dyn I18nService>);
    let services = start(builder);
    let formatter = services.required_service::<dyn MessageFormatter>().unwrap();
    let ana = Args::new().with("name", "Ana");
    assert_eq!(formatter.format("hello", &ana), "Hi, Ana!");
    block_on(i18n.set_language("hr".into())).unwrap();
    assert_eq!(formatter.format("hello", &ana), "Bok, Ana!");
}

#[test]
fn the_app_s_own_i18n_registered_after_add_i18n_replaces_it_completely() {
    let assets = own_assets();
    let platform = FakePlatform::new(PlatformSnapshot::default());
    let i18n = FakeI18n::in_language("en");
    block_on(i18n.add_resource("en".into(), None, r#"{ "hello": "Hi, {$name}!" }"#.into()))
        .unwrap();
    let mut builder = RayXAppBuilder::new().add_i18n(|options| {
        options.default_language("en");
    });
    let services = builder.services_mut();
    services.add_singleton_as::<dyn AssetService>(Arc::clone(&assets) as Arc<dyn AssetService>);
    services.add_singleton_as::<dyn PlatformInfoService>(
        Arc::clone(&platform) as Arc<dyn PlatformInfoService>
    );
    services.add_singleton_as::<dyn I18nService>(Arc::clone(&i18n) as Arc<dyn I18nService>);
    assert!(
        !services.has_startup_role::<dyn I18nService>(),
        "registering the app's own removes RayX's startup service"
    );
    let services = start(builder);

    assert!(same(
        &services.required_service::<dyn I18nService>().unwrap(),
        &i18n
    ));
    assert!(
        assets.loads().is_empty(),
        "RayX's i18n never loads its messages: {:?}",
        assets.loads()
    );
    let formatter = services.required_service::<dyn MessageFormatter>().unwrap();
    let ana = Args::new().with("name", "Ana");
    assert_eq!(formatter.format("hello", &ana), "Hi, Ana!");
}

/// The app's own services, one of each for the process.
struct Own {
    platform: Arc<FakePlatform>,
    suspension: Arc<FakeSuspension>,
    assets: Arc<FakeAssets>,
    fonts: Arc<FakeFonts>,
    store: Arc<FakeKeyValueStore>,
    i18n: Arc<FakeI18n>,
}

fn own() -> &'static Own {
    static OWN: OnceLock<Own> = OnceLock::new();
    OWN.get_or_init(|| {
        let mut snapshot = PlatformSnapshot::default();
        snapshot.viewport.width = 321.0;
        let store = FakeKeyValueStore::new();
        block_on(store.set("prefs".into(), "lang".into(), b"hr".to_vec())).unwrap();
        let i18n = FakeI18n::in_language("hr");
        block_on(i18n.add_resource("hr".into(), None, r#"{ "hello": "Bok, {$name}!" }"#.into()))
            .unwrap();
        Own {
            platform: FakePlatform::new(snapshot),
            suspension: FakeSuspension::new(),
            assets: own_assets(),
            fonts: FakeFonts::new(FontState::default()),
            store,
            i18n,
        }
    })
}

/// Registers the app's own platform, suspension, assets, fonts, key-value store and i18n
/// service.
#[rayx::app_extension]
fn add_own_services(mut builder: RayXAppBuilder) -> RayXAppBuilder {
    let own = own();
    let services = builder.services_mut();
    services.add_singleton_as::<dyn PlatformInfoService>(
        Arc::clone(&own.platform) as Arc<dyn PlatformInfoService>
    );
    services.add_singleton_as::<dyn SuspensionService>(
        Arc::clone(&own.suspension) as Arc<dyn SuspensionService>
    );
    services.add_singleton_as::<dyn AssetService>(Arc::clone(&own.assets) as Arc<dyn AssetService>);
    services.add_singleton_as::<dyn FontService>(Arc::clone(&own.fonts) as Arc<dyn FontService>);
    services
        .add_singleton_as::<dyn KeyValueStore>(Arc::clone(&own.store) as Arc<dyn KeyValueStore>);
    services.add_singleton_as::<dyn I18nService>(Arc::clone(&own.i18n) as Arc<dyn I18nService>);
    builder
}

/// The test's Rust side.
#[module]
struct Probe;

#[module]
impl Probe {
    /// Whether the started app's platform, suspension, assets and fonts are the app's own.
    pub(crate) fn host_services_are_own(
        services: Instance<TsServiceProvider>,
    ) -> Result<bool, String> {
        services.with(|services| {
            let services = services.services();
            let own = own();
            let platform = services.required_service::<dyn PlatformInfoService>();
            let suspension = services.required_service::<dyn SuspensionService>();
            let assets = services.required_service::<dyn AssetService>();
            let fonts = services.required_service::<dyn FontService>();
            match (platform, suspension, assets, fonts) {
                (Ok(platform), Ok(suspension), Ok(assets), Ok(fonts)) => {
                    Ok(same(&platform, &own.platform)
                        && same(&suspension, &own.suspension)
                        && same(&assets, &own.assets)
                        && same(&fonts, &own.fonts))
                }
                _ => Err("a host service is missing".to_owned()),
            }
        })
    }

    /// The text the app's own store holds at `key` in `bucket`.
    pub(crate) fn stored(bucket: String, key: String) -> Option<String> {
        block_on(own().store.get(bucket, key))
            .unwrap()
            .map(|bytes| String::from_utf8(bytes).unwrap())
    }
}

/// The app's set: the services' sets, the extension and the probe, without the host fakes'
/// `addHostFakes`, which no TSX here calls.
fn set() -> Registry {
    headless::set(|set| {
        rayx::services::platform::register(set);
        rayx::services::storage::register(set);
        rayx::services::i18n::register(set);
        set.module::<Probe>().class_extension::<add_own_services>();
    })
}

const HOST_TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { Probe, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    try {
      await new RayXAppBuilder().start();
      lines.push("started without");
    } catch (error) {
      lines.push(String(error.message ?? error).includes("PlatformInfoService") ? "refused" : `${error}`);
    }
    const services = await new RayXAppBuilder().addOwnServices().start();
    lines.push(String(Probe.hostServicesAreOwn(services)));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn the_app_s_own_platform_suspension_assets_and_fonts_start_the_host() {
    assert_eq!(headless::run(set(), HOST_TSX), "done refused|true");
}

const CLASSES_TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { I18nService, KeyValueStore, PlatformInfoService, Probe, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder().addOwnServices().addMessageFormatter().start();
    const store = services.getRequiredService(KeyValueStore);
    lines.push(await store.get("prefs", "lang"));
    await store.set("prefs", "theme", "dark");
    lines.push(Probe.stored("prefs", "theme"));
    const platform = services.getRequiredService(PlatformInfoService);
    lines.push(String(platform.current().viewport.width));
    const i18n = services.getRequiredService(I18nService);
    lines.push(`${i18n.locale()} ${i18n.t("hello", { name: "Ana" })}`);
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn the_services_ts_classes_wrap_the_app_s_own_implementations() {
    assert_eq!(
        headless::run(set(), CLASSES_TSX),
        "done hr|dark|321|hr Bok, Ana!"
    );
}
