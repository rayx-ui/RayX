//! `add_history`, `add_history_with_store` and `add_persistent_history`: in Rust
//! ([`HistoryAppBuilderExt`]) and in TS ([`HistoryBuilder`], a class extension of the host's
//! `RayXAppBuilder`).

use std::sync::Arc;

use nsh_core::prelude::*;
use rayx_app::di::{StartupInitializationFailure, StartupService, StartupServiceType};
use rayx_app::services::history::HistoryService;
use rayx_app::services::operations::OperationService;
use rayx_app::services::storage::KeyValueStore;
use rayx_app::{AppBuilder, ServiceProvider};
use rayx_app_nsh::ts::{ContractFactory, RayXAppBuilder, record};

use crate::options_ts::{HistoryOptions as TsHistoryOptions, HistoryStep};

use crate::journal_store::{HistoryJournalStore, JournalStore};
use crate::key_value_store::KeyValueHistoryStore;
use crate::service::{RayxHistory, Starting};
use crate::store::HistoryStore;
use crate::timer::{HISTORY_CONFIG_PATH, HistoryOptions, HistoryTimer};
use rayx_app_nsh::EngineRunner;

/// What an app with a history and no operation service is told.
const OPERATIONS_HINT: &str = "add_operations() (addOperations in TS) from \
                               rayx_services_operations, or register your own dyn \
                               OperationService";

/// What an app keeping its history in a store written in TS without one is told.
const JOURNAL_STORE_HINT: &str = "addHistoryWithStore(() => new MyStore()) in TS, or register your \
                                  own dyn HistoryJournalStore";

/// What an app keeping its history in its key-value store without one is told.
const STORAGE_HINT: &str = "add_storage() (addStorage in TS) from rayx_services_storage, or \
                            register your own dyn KeyValueStore";

/// Registers RayX's history on an app builder.
pub trait HistoryAppBuilderExt: AppBuilder {
    /// Registers RayX's history as the app's `dyn HistoryService`, replacing any other: a
    /// startup service observing the app's `dyn OperationService` (which it requires:
    /// `add_operations()` from `rayx_services_operations`, or the app's own), with history kept
    /// in memory only.
    fn add_history(self) -> Self {
        register(self, false, Vec::new())
    }

    /// Like [`add_history`](Self::add_history), with each scope's history persisted in
    /// `store`, which is registered as the app's `dyn HistoryStore` (a startup service the
    /// history waits for).
    fn add_history_with_store(mut self, store: impl HistoryStore) -> Self {
        let store: Arc<dyn HistoryStore> = Arc::new(store);
        self.services_mut()
            .replace_app_service::<dyn HistoryStore>(store);
        register(self, true, Vec::new())
    }

    /// Like [`add_history`](Self::add_history), with each scope's history kept in the app's
    /// `dyn KeyValueStore` (which it requires: `add_storage()` from `rayx_services_storage`, or
    /// the app's own) by a [`KeyValueHistoryStore`], so it persists wherever the store does: on
    /// NSH, in the device's storage.
    fn add_persistent_history(self) -> Self {
        add_persistent(self, Vec::new())
    }
}

impl<T> HistoryAppBuilderExt for T where T: AppBuilder {}

/// `add_persistent_history`, with TS's recorded option `steps` after configuration's.
fn add_persistent<B: AppBuilder>(mut builder: B, steps: Vec<HistoryStep>) -> B {
    let services = builder.services_mut();
    services.require_for::<dyn HistoryStore, dyn KeyValueStore>(STORAGE_HINT);
    services.replace_app_service_factory_as::<dyn HistoryStore>(|services| {
        let store = services
            .required_service::<dyn KeyValueStore>()
            .unwrap_or_else(|error| panic!("the history store needs a key-value store: {error}"));
        Arc::new(KeyValueHistoryStore::new(store))
    });
    register(builder, true, steps)
}

/// `add_history_with_store` over the app's `dyn HistoryJournalStore` (a store written in TS),
/// with TS's recorded option `steps` after configuration's.
fn add_with_journal_store<B: AppBuilder>(mut builder: B, steps: Vec<HistoryStep>) -> B {
    let services = builder.services_mut();
    services.require_for::<dyn HistoryStore, dyn HistoryJournalStore>(JOURNAL_STORE_HINT);
    services.replace_app_service_factory_as::<dyn HistoryStore>(|services| {
        let store = services
            .required_service::<dyn HistoryJournalStore>()
            .unwrap_or_else(|error| {
                panic!("the history store written in TS is registered: {error}")
            });
        Arc::new(JournalStore(store))
    });
    register(builder, true, steps)
}

/// Registers RayX's history as `dyn HistoryService` (a startup service keyed by it), kept in
/// the app's `dyn HistoryStore` when `stored`, its options bound from configuration, then set
/// by `steps` (TS's options callback).
fn register<B: AppBuilder>(mut builder: B, stored: bool, steps: Vec<HistoryStep>) -> B {
    let services = builder.services_mut();
    services.require_for::<dyn HistoryService, dyn OperationService>(OPERATIONS_HINT);
    let options = services
        .add_options::<HistoryOptions>()
        .bind(HISTORY_CONFIG_PATH);
    if !steps.is_empty() {
        // TS's options callback, after configuration's.
        options.configure(move |options| {
            for step in &steps {
                step.apply(options);
            }
        });
    }
    services.replace_service_factory_as::<RayxHistory>(move |services| {
        let options = services
            .options::<HistoryOptions>()
            .unwrap_or_else(|error| panic!("the history options are invalid: {error}"));
        let timeout = options.call_timeout();
        let record_agent_operations = options.record_agent_operations;
        // The app's timer, else the engine's on NSH; with neither, calls are unbounded.
        let timer = services
            .service::<HistoryTimer>()
            .map(|timer| (*timer).clone())
            .or_else(|| {
                services
                    .service::<EngineRunner>()
                    .map(|runner| HistoryTimer::engine((*runner).clone()))
            });
        RayxHistory::new(
            operations(services),
            None,
            stored,
            timer,
            timeout,
            record_agent_operations,
        )
    });
    // Replaced before the startup service is added: replacing `dyn HistoryService` drops the
    // startup service keyed by it, which `Starting` is.
    services.replace_service_factory_as::<dyn HistoryService>(|services| {
        services
            .required_service::<RayxHistory>()
            .unwrap_or_else(|error| panic!("RayX's history is registered: {error}"))
    });
    services.add_startup_resolved_async(
        StartupServiceType::of::<dyn HistoryService>(),
        |context| {
            let history = context.services().required_service::<RayxHistory>();
            Box::pin(async move {
                let history = history.map_err(|error| StartupInitializationFailure {
                    service: StartupServiceType::of::<dyn HistoryService>(),
                    message: error.to_string(),
                    timed_out: false,
                })?;
                Ok(Arc::new(Starting(history)) as StartupService)
            })
        },
    );
    builder
}

fn operations(services: &ServiceProvider) -> Arc<dyn OperationService> {
    services
        .required_service::<dyn OperationService>()
        .unwrap_or_else(|error| panic!("the history needs an operation service: {error}"))
}

/// The methods `rayx_services_history` adds to TS's `RayXAppBuilder`.
pub struct HistoryBuilder;

#[class_extension(RayXAppBuilder)]
impl HistoryBuilder {
    /// Registers the app's history (undo and redo of operations, kept in memory); it needs an
    /// operation service (`addOperations`). `configure` sets its options
    /// (`options.callTimeoutMs(5000)`) after configuration's; one that throws makes this call
    /// throw.
    pub fn add_history(
        this: Instance<RayXAppBuilder>,
        #[args(options)] configure: Option<Callback<Instance<TsHistoryOptions>>>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        let steps = record(TsHistoryOptions::new(), configure, TsHistoryOptions::take)?;
        RayXAppBuilder::extend_with(this, move |builder| register(builder, false, steps))
    }

    /// Registers the app's history kept in its key-value store (so on NSH in the device's
    /// storage); it needs an operation service (`addOperations`) and a key-value store
    /// (`addStorage`). `configure` sets its options (`options.callTimeoutMs(5000)`) after
    /// configuration's; one that throws makes this call throw.
    pub fn add_persistent_history(
        this: Instance<RayXAppBuilder>,
        #[args(options)] configure: Option<Callback<Instance<TsHistoryOptions>>>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        let steps = record(TsHistoryOptions::new(), configure, TsHistoryOptions::take)?;
        RayXAppBuilder::extend_with(this, move |builder| add_persistent(builder, steps))
    }

    /// Registers the app's history kept in `store`'s store, written in TS: `store` makes a
    /// `HistoryStoreContract` (`() => new MyStore()`), which keeps each scope's journal as JSON
    /// text by revision; it needs an operation service (`addOperations`). `configure` sets its
    /// options (`options.callTimeoutMs(5000)`) after configuration's; one that throws makes this
    /// call throw.
    pub fn add_history_with_store(
        this: Instance<RayXAppBuilder>,
        store: ContractFactory<dyn HistoryJournalStore>,
        #[args(options)] configure: Option<Callback<Instance<TsHistoryOptions>>>,
        sys: System,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        let steps = record(TsHistoryOptions::new(), configure, TsHistoryOptions::take)?;
        let this = RayXAppBuilder::add_contract_singleton(this, store, &sys)?;
        RayXAppBuilder::extend_with(this, move |builder| add_with_journal_store(builder, steps))
    }
}
