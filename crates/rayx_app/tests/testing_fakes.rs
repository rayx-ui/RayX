//! `rayx_app::testing`'s fakes, NSH-free: each registered as its trait in a plain
//! `ServiceProvider`, resolved by the trait and used, its state published through the trait's
//! cell (only real changes, never under a lock a subscriber could need), its commands recorded
//! or answered from what the test set.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use futures::executor::block_on;
use rayx_app::di::DiReady;
use rayx_app::services::application_configuration::{
    ApplicationConfigurationService, ApplicationConfigurationSnapshot, LayoutDirection,
};
use rayx_app::services::assets::{AssetCatalog, AssetError, AssetService};
use rayx_app::services::fonts::{FontService, FontState};
use rayx_app::services::history::{HistoryEvent, HistoryScopeId, HistoryService};
use rayx_app::services::i18n::{I18nService, I18nServiceError, MessageFormatter};
use rayx_app::services::input_map::{
    InputMapDocument, InputMapEntry, InputMapService, InputMapSource, InputMapState,
};
use rayx_app::services::navigation::{NavigationEvent, NavigationService};
use rayx_app::services::operations::{
    DomainOperation, OperationApplyResult, OperationEvent, OperationOrigin, OperationService,
    OperationTargetHandler, OperationTargetId,
};
use rayx_app::services::platform::{PlatformInfoService, PlatformSnapshot, TextDirection};
use rayx_app::services::selection::{SelectionAreaId, SelectionService, SelectionStatus};
use rayx_app::services::storage::KeyValueStore;
use rayx_app::services::suspension::{SuspensionEventReason, SuspensionService};
use rayx_app::testing::{
    FakeApplicationConfiguration, FakeAssets, FakeFonts, FakeHistory, FakeI18n, FakeInputMap,
    FakeKeyValueStore, FakeMessageFormatter, FakeNavigation, FakeOperations, FakePlatform,
    FakeSelection, FakeSuspension, I18nCall, SelectionCall,
};
use rayx_app::{ChangeSubscription, ServiceProvider, StateCell};
use serde::{Deserialize, Serialize};

/// `service` registered as `T` in a plain provider, and resolved by it.
fn resolved<T: DiReady + ?Sized>(service: Arc<T>) -> Arc<T> {
    let mut services = ServiceProvider::new();
    services.add_singleton_as::<T>(service);
    services.required_service::<T>().unwrap()
}

/// The values `cell` publishes from now on, while the subscription lives.
fn published<T: Clone + Send + Sync + 'static>(
    cell: &StateCell<T>,
) -> (Arc<Mutex<Vec<T>>>, ChangeSubscription) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let subscription = cell.subscribe(move |value| sink.lock().unwrap().push((**value).clone()));
    (seen, subscription)
}

#[test]
fn the_platform_publishes_what_the_test_sets() {
    let fake = FakePlatform::new(PlatformSnapshot::default());
    let platform = resolved::<dyn PlatformInfoService>(Arc::clone(&fake) as _);
    let (seen, _subscription) = published(platform.platform());
    fake.modify(|snapshot| snapshot.viewport.width = 500.0);
    // Only a real change is published.
    fake.publish((*platform.platform().get()).clone());
    assert_eq!(platform.platform().get().viewport.width, 500.0);
    assert_eq!(seen.lock().unwrap().len(), 1);
}

#[test]
fn suspension_publishes_its_events_and_records_quits() {
    let fake = FakeSuspension::new();
    let suspension = resolved::<dyn SuspensionService>(Arc::clone(&fake) as _);
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    let _events = suspension
        .events()
        .subscribe(move |event| sink.lock().unwrap().push(event.reason));
    fake.start();
    assert_eq!(
        *events.lock().unwrap(),
        [SuspensionEventReason::HostStarted]
    );
    assert_eq!(
        suspension.suspension().get().last_reason,
        Some(SuspensionEventReason::HostStarted)
    );
    block_on(suspension.ask_before_quit(true)).unwrap();
    assert!(suspension.suspension().get().asks_before_quit);
    block_on(suspension.quit()).unwrap();
    block_on(suspension.quit()).unwrap();
    assert_eq!(fake.quits(), 2);
}

#[test]
fn assets_answer_their_files_and_record_the_loads() {
    let mut catalog = AssetCatalog::builder();
    catalog
        .add_manifest_text("app", "files = [\"notes/hello.txt\"]")
        .unwrap();
    let fake = FakeAssets::new(catalog.build()).file("@app/notes/hello.txt", "hello");
    let assets = resolved::<dyn AssetService>(Arc::clone(&fake) as _);
    assert!(assets.catalog().get().is_some());
    block_on(assets.load_catalog()).unwrap();
    assert_eq!(
        block_on(assets.load_text("@app/notes/hello.txt".into())).unwrap(),
        "hello"
    );
    assert_eq!(
        block_on(assets.load_bytes("@app/nowhere".into())),
        Err(AssetError::NotFound("@app/nowhere".into()))
    );
    assets.clear_cache();
    assert_eq!(fake.loads(), ["@app/notes/hello.txt", "@app/nowhere"]);
    assert_eq!((fake.catalog_loads(), fake.cleared()), (1, 1));

    let (seen, _subscription) = published(assets.catalog());
    fake.publish(AssetCatalog::default());
    assert_eq!(*seen.lock().unwrap(), [Some(AssetCatalog::default())]);
}

#[test]
fn fonts_answer_registrations_with_the_test_s_families() {
    let fake = FakeFonts::new(FontState::default()).family("Inter");
    let fonts = resolved::<dyn FontService>(Arc::clone(&fake) as _);
    let (seen, _subscription) = published(fonts.fonts());
    assert_eq!(
        block_on(fonts.register_font(b"inter".to_vec())).unwrap(),
        "Inter"
    );
    assert_eq!(
        block_on(fonts.register_font(b"other".to_vec())).unwrap(),
        "Font 2"
    );
    block_on(fonts.register_font_assets()).unwrap();
    let families: Vec<String> = fonts
        .registered_fonts()
        .into_iter()
        .map(|face| face.family)
        .collect();
    assert_eq!(families, ["Inter", "Font 2"]);
    assert_eq!(seen.lock().unwrap().len(), 2);
    assert_eq!(fake.registered(), [b"inter".to_vec(), b"other".to_vec()]);
    assert_eq!(fake.asset_registrations(), 1);
}

#[test]
fn the_store_answers_from_what_was_set() {
    let store = resolved::<dyn KeyValueStore>(FakeKeyValueStore::new() as _);
    block_on(async {
        store
            .set("prefs".into(), "lang".into(), b"hr".to_vec())
            .await
            .unwrap();
        assert_eq!(
            store.get("prefs".into(), "lang".into()).await.unwrap(),
            Some(b"hr".to_vec())
        );
        assert_eq!(store.keys("prefs".into()).await.unwrap(), ["lang"]);
        assert!(store.remove("prefs".into(), "lang".into()).await.unwrap());
        store.clear("prefs".into()).await.unwrap();
        assert!(store.keys("prefs".into()).await.unwrap().is_empty());
    });
}

#[test]
fn navigation_publishes_its_journal_and_events_and_records_requests() {
    let fake = FakeNavigation::new();
    let navigation = resolved::<dyn NavigationService>(Arc::clone(&fake) as _);
    let (seen, _subscription) = published(navigation.navigation());
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    let _events = navigation.events().subscribe(move |event| {
        let name = match event {
            NavigationEvent::NavigatedFrom(_) => "from",
            NavigationEvent::NavigatedTo(_) => "to",
            NavigationEvent::RouteChanged(_) => "route",
            NavigationEvent::LocationChanged(_) => "location",
            NavigationEvent::RegionClosed { .. } => "closed",
        };
        sink.lock().unwrap().push(name);
    });
    block_on(navigation.navigate_to("/projects")).unwrap();
    block_on(navigation.navigate_to("/projects/1")).unwrap();
    assert_eq!(navigation.navigation().get().location, "/projects/1");
    block_on(navigation.back("root".into())).unwrap();
    assert_eq!(navigation.navigation().get().location, "/projects");
    assert_eq!(seen.lock().unwrap().len(), 3);
    assert_eq!(
        events.lock().unwrap()[..7],
        ["to", "route", "location", "from", "to", "route", "location"]
    );
    let requested: Vec<String> = fake
        .requests()
        .into_iter()
        .map(|request| request.args.uri)
        .collect();
    assert_eq!(requested, ["/projects", "/projects/1"]);
}

#[test]
fn i18n_applies_and_records_its_commands() {
    let fake = FakeI18n::in_language("en");
    let i18n = resolved::<dyn I18nService>(Arc::clone(&fake) as _);
    assert_eq!((i18n.language(), i18n.version()), ("en".into(), 0));
    let (seen, _subscription) = published(i18n.i18n());

    block_on(i18n.set_language("ar".into())).unwrap();
    assert_eq!(i18n.chosen_language().as_deref(), Some("ar"));
    assert_eq!(i18n.direction(), TextDirection::Rtl);
    assert_eq!(i18n.version(), 1);
    assert_eq!(
        block_on(i18n.set_language("not a tag!".into())),
        Err(I18nServiceError::InvalidLanguage("not a tag!".into()))
    );
    block_on(i18n.add_resource(
        "de".into(),
        Some("checkout".into()),
        r#"{ "title": "Kasse", "cart": { "empty": "Leer" } }"#.into(),
    ))
    .unwrap();
    assert_eq!(i18n.available_languages(), ["de"]);
    let messages = &i18n.i18n().get().messages[0].messages;
    assert_eq!(messages["checkout.title"], "Kasse");
    assert_eq!(messages["checkout.cart.empty"], "Leer");
    block_on(i18n.set_time_zone("Europe/Zagreb".into())).unwrap();
    block_on(i18n.follow_platform()).unwrap();
    assert_eq!(
        (i18n.language(), i18n.chosen_language()),
        ("en".into(), None)
    );
    // Loading changes nothing, so nothing is published.
    block_on(i18n.load_messages()).unwrap();
    assert_eq!(i18n.time_zone(), "Europe/Zagreb");
    assert_eq!(i18n.version(), 4);
    assert_eq!(seen.lock().unwrap().len(), 4);
    assert_eq!(
        fake.calls()[..2],
        [
            I18nCall::SetLanguage("ar".into()),
            I18nCall::SetLanguage("not a tag!".into())
        ]
    );
    assert_eq!(fake.calls().len(), 6);
    assert_eq!(fake.calls()[5], I18nCall::LoadMessages);
}

#[test]
fn the_message_formatter_answers_the_test_s_messages() {
    let wall_time = dxn_intl::DateTime::new(2026, 11, 2, 10, 30, 0).unwrap();
    let fake = FakeMessageFormatter::new("hr")
        .message("hello", "Bok, {$name}!")
        .message(
            "cart",
            ".input {$n :number}\n.match $n\none {{{$n} artikl}}\nfew {{{$n} artikla}}\n* {{{$n} artikala}}",
        )
        .local_time_at("2026-11-02T09:30:00Z", wall_time);
    let formatter = resolved::<dyn MessageFormatter>(Arc::clone(&fake) as _);
    assert_eq!(formatter.locale().to_string(), "hr");
    assert_eq!(
        formatter.format("hello", &dxn_i18n::Args::new().with("name", "Ana")),
        "Bok, Ana!"
    );
    assert_eq!(
        formatter.format("cart", &dxn_i18n::Args::new().with("n", 3)),
        "3 artikla"
    );
    assert!(formatter.contains("hello"));
    assert_eq!(formatter.t("missing"), "missing");
    assert_eq!(
        formatter.plural_category(21.into()),
        dxn_intl::PluralCategory::One
    );
    assert_eq!(
        formatter
            .format_number(1234.into(), dxn_intl::NumberOptions::default())
            .unwrap(),
        "1.234"
    );
    assert_eq!(
        formatter.local_time("2026-11-02T09:30:00Z").unwrap(),
        wall_time
    );
    assert!(formatter.local_time_from_millis(0).is_err());
    assert_eq!(fake.asked(), ["hello", "cart", "hello", "missing"]);
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct Set(u32);

impl DomainOperation for Set {
    const KIND: &'static str = "fakes.set";
    const VERSION: u32 = 1;
}

#[derive(Default)]
struct Target(Mutex<u32>);

impl OperationTargetHandler<Set> for Target {
    fn apply_atomically(
        &self,
        operation: &Set,
        _origin: &OperationOrigin,
    ) -> Result<OperationApplyResult<Set>, String> {
        let before = std::mem::replace(&mut *self.0.lock().unwrap(), operation.0);
        Ok(OperationApplyResult::applied(
            Set(before),
            ["value"],
            u64::from(before),
            u64::from(operation.0),
        ))
    }
}

#[test]
fn operations_apply_through_their_targets_and_publish_them() {
    let operations = resolved::<dyn OperationService>(FakeOperations::new() as _);
    let (seen, _subscription) = published(operations.operations());
    let committed = Arc::new(Mutex::new(0));
    let sink = Arc::clone(&committed);
    let _events = operations.events().subscribe(move |event| {
        if matches!(event, OperationEvent::Committed(_)) {
            *sink.lock().unwrap() += 1;
        }
    });
    let target = Arc::new(Target::default());
    let id = OperationTargetId::new("doc").unwrap();
    block_on(operations.register_handler(id.clone(), Arc::clone(&target))).unwrap();
    assert_eq!(operations.targets(), std::slice::from_ref(&id));
    block_on(operations.submit(&id, Set(3), OperationOrigin::LocalUser)).unwrap();
    assert_eq!(*target.0.lock().unwrap(), 3);
    assert_eq!(*committed.lock().unwrap(), 1);
    assert!(block_on(operations.release_target(id)).unwrap());
    assert!(operations.targets().is_empty());
    assert_eq!(seen.lock().unwrap().len(), 2);
}

#[test]
fn history_publishes_the_depths_and_records_bindings() {
    let fake = FakeHistory::new();
    let history = resolved::<dyn HistoryService>(Arc::clone(&fake) as _);
    let scope = HistoryScopeId::new("editor").unwrap();
    let (seen, _subscription) = published(history.history());
    let undone = Arc::new(Mutex::new(0));
    let sink = Arc::clone(&undone);
    let _events = history.events().subscribe(move |event| {
        if matches!(event, HistoryEvent::UndoCompleted(_)) {
            *sink.lock().unwrap() += 1;
        }
    });
    fake.set_depths(&scope, 1, 0);
    assert!(history.can_undo(&scope));
    assert!(block_on(history.undo(scope.clone())).unwrap());
    assert!(!block_on(history.undo(scope.clone())).unwrap());
    assert!(history.can_redo(&scope));
    assert_eq!(*undone.lock().unwrap(), 1);
    assert_eq!(seen.lock().unwrap().len(), 3);
    let target = OperationTargetId::new("doc").unwrap();
    block_on(history.bind_target_scope(target.clone(), scope.clone())).unwrap();
    assert_eq!(fake.bindings(), [(target, scope)]);
}

#[test]
fn selection_applies_and_records_its_commands() {
    let fake = FakeSelection::new();
    let selection = resolved::<dyn SelectionService>(Arc::clone(&fake) as _);
    let (seen, _subscription) = published(selection.selection());
    let area = SelectionAreaId::from_raw(7);
    let status = SelectionStatus {
        selected_character_count: 4,
        ..SelectionStatus::default()
    };
    block_on(selection.activate(area, status)).unwrap();
    assert_eq!(selection.active_area(), Some(area));
    // Another area neither publishes nor deactivates.
    block_on(selection.deactivate(SelectionAreaId::from_raw(8))).unwrap();
    block_on(selection.clear()).unwrap();
    assert_eq!(selection.active_area(), None);
    assert_eq!(seen.lock().unwrap().len(), 2);
    assert_eq!(
        fake.calls(),
        [
            SelectionCall::Activate(area, status),
            SelectionCall::Deactivate(SelectionAreaId::from_raw(8)),
            SelectionCall::Clear,
        ]
    );
}

#[test]
fn the_input_map_applies_and_records_the_user_s_documents() {
    let fake = FakeInputMap::new(InputMapState::default());
    let input_map = resolved::<dyn InputMapService>(Arc::clone(&fake) as _);
    let (seen, _subscription) = published(input_map.input_map());
    let document = InputMapDocument::new(
        InputMapSource::User,
        0,
        vec![InputMapEntry::keyboard("", "cmd-k", "app::search", None)],
    );
    let applied = block_on(input_map.apply_user_document(0, document)).unwrap();
    assert_eq!((applied.revision, input_map.revision()), (1, 1));
    assert_eq!(block_on(input_map.load()).unwrap(), applied);
    assert!(block_on(input_map.apply_user_document(0, applied.clone())).is_err());
    assert_eq!(seen.lock().unwrap().len(), 1);
    assert_eq!((fake.applied(), fake.loads()), (vec![applied], 1));
}

#[test]
fn the_configuration_takes_and_records_options() {
    let fake = FakeApplicationConfiguration::new(ApplicationConfigurationSnapshot::default());
    let configuration = resolved::<dyn ApplicationConfigurationService>(Arc::clone(&fake) as _);
    let (seen, _subscription) = published(configuration.configuration());
    let mut options = configuration.options();
    options.layout.direction = LayoutDirection::Rtl;
    assert!(block_on(configuration.replace_options(options.clone())).unwrap());
    assert!(!block_on(configuration.replace_options(options.clone())).unwrap());
    assert!(configuration.layout_direction().is_rtl());
    assert_eq!(seen.lock().unwrap().len(), 1);
    assert_eq!(fake.replaced(), [options.clone(), options]);
}

#[test]
fn a_subscriber_may_call_the_fake_back() {
    let fake = FakeSelection::new();
    let selection: Arc<dyn SelectionService> = Arc::clone(&fake) as _;
    let inner = Arc::clone(&selection);
    // Clears the selection whenever one is activated, from inside the publish.
    let _subscription = selection.selection().subscribe(move |state| {
        if state.active_area.is_some() {
            block_on(inner.clear()).unwrap();
        }
    });
    block_on(selection.activate(SelectionAreaId::from_raw(1), SelectionStatus::default())).unwrap();
    assert_eq!(selection.active_area(), None);
}

#[test]
fn changes_from_many_threads_are_all_published() {
    let fake = FakeFonts::new(FontState::default());
    let fonts: Arc<dyn FontService> = Arc::clone(&fake) as _;
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let fonts = Arc::clone(&fonts);
            std::thread::spawn(move || {
                for _ in 0..25 {
                    block_on(fonts.register_font(Vec::new())).unwrap();
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(fonts.registered_fonts().len(), 200);

    let i18n = FakeI18n::in_language("en");
    let threads: Vec<_> = (0..8)
        .map(|thread| {
            let i18n = Arc::clone(&i18n);
            std::thread::spawn(move || {
                for message in 0..25 {
                    let json = format!(r#"{{ "m{thread}_{message}": "x" }}"#);
                    block_on(i18n.add_resource("en".into(), None, json)).unwrap();
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    let state = i18n.i18n().get();
    assert_eq!(state.messages[0].messages.len(), 200);
    assert_eq!(state.version, 200);
}
