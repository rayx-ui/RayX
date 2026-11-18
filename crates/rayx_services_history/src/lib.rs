//! RayX's history: RayX's implementation of [`rayx_app::services::history::HistoryService`].
//! It records the operations the local user commits through the app's `dyn OperationService`,
//! per scope (a window, a document; a transaction is one entry), undoes and redoes them by
//! resubmitting the inverses (or the operations) with fresh inverses, and keeps each scope's
//! journal in memory, in a [`HistoryStore`] of the app's, or in the app's `dyn KeyValueStore`
//! ([`KeyValueHistoryStore`]). It publishes each scope's depths and each step.
//!
//! - Rust: `RayXAppBuilder::new().add_operations().add_history()` ([`HistoryAppBuilderExt`];
//!   `add_operations()` is `rayx_services_operations`', or the app registers its own
//!   `dyn OperationService`), then `services.required_service::<dyn HistoryService>()`.
//! - TS: `new RayXAppBuilder().addOperations().addHistory()` (or `addPersistentHistory()`), with
//!   the class [`HistoryService`] (the contract's token).
//! - The app's set: [`register`].
//!
//! [`HistoryStore`] is the implementation's extension point: where it keeps each scope's
//! journal. The `diagnostics` feature keeps the replays that failed in the published state.

#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

mod builder;
mod detached;
mod history_ts;
mod journal;
mod key_value_store;
mod lifecycle;
mod limits;
mod options_ts;
mod persistence;
mod retained_records;
mod service;
mod store;
mod timer;

use nsh_core::element::{Registry, TsAlias};
use rayx_app::services::history::*;
use rayx_app::services::operations::{OperationRecord, OperationTargetId};
use rayx_app::{EventStream, StateCell};
use rayx_app_nsh::ts::ContractRegistryExt as _;

pub use builder::{HistoryAppBuilderExt, HistoryBuilder};
pub use history_ts::HistoryService;
pub use journal::{HISTORY_JOURNAL_VERSION, HistoryJournalError, HistoryJournalV2};
pub use key_value_store::{HISTORY_BUCKET, KeyValueHistoryStore};
pub use limits::{HistoryLimitError, HistoryLimits, encoded_size, enforce};
pub use options_ts::HistoryOptions as TsHistoryOptions;
pub use store::{HistoryStore, HistoryStoreError, InMemoryHistoryStore};
pub use timer::{HISTORY_CONFIG_PATH, HistoryOptions, HistoryTimer};

rayx_app_nsh::contract_nsh!(rayx_app::services::history::HistoryService as HistoryServiceContract);

/// The history's state in TS.
pub const HISTORY_STATE: TsAlias = TsAlias {
    name: "HistoryState",
    ts: "{ scopes: Record<string, { undo_depth: number; redo_depth: number; revision: number; entry_limit: number | null }>; unavailable_entry_count: number; recent_failure_codes: string[] }",
    doc: "Each scope's depths (and diagnostics): the JSON of Rust's `HistoryState`.",
};

/// A history event in TS.
pub const HISTORY_EVENT: TsAlias = TsAlias {
    name: "HistoryEvent",
    ts: "{ Recorded: Record<string, unknown> } | { ScopeCleared: string } | { AvailabilityChanged: string } | { UndoCompleted: string } | { RedoCompleted: string } | { NavigationFailed: { scope_id: string; error_code: string } }",
    doc: "A step of the history: the JSON of Rust's `HistoryEvent`.",
};

/// An operation record in TS: the alias `rayx_services_operations`' set declares, the same
/// (NSH takes an alias declared twice alike).
pub const OPERATION_RECORD: TsAlias = TsAlias {
    name: "OperationRecord",
    ts: "{ target_id: string; kind: string; version: number; payload: number[] }",
    doc: "An operation as it crosses: its target, kind, version and the operation's JSON as bytes (the JSON of Rust's `OperationRecord`).",
};

/// Adds the history's TS side to an app's set: its class [`HistoryService`], the contract a TS
/// implementation follows (the TS interface `HistoryServiceContract`, [`HistoryServiceContract`]
/// in Rust), the options class [`TsHistoryOptions`] (`HistoryOptions` in TS) and
/// `builder.addHistory(..)` and `builder.addPersistentHistory(..)`.
pub fn register(registry: &mut Registry) {
    registry
        .ts_alias(OPERATION_RECORD)
        .ts_alias(HISTORY_STATE)
        .ts_alias(HISTORY_EVENT)
        .contract_class::<HistoryServiceContract, HistoryService>()
        .class::<TsHistoryOptions>()
        .class_extension::<HistoryBuilder>();
}

/// RayX's history, made directly over an operation service, for the crate's tests and an
/// app's.
#[doc(hidden)]
pub mod testing {
    use std::sync::Arc;

    use rayx_app::services::history::HistoryService;
    use rayx_app::services::operations::OperationService;

    use crate::HistoryStore;

    /// RayX's history of `operations`' operations, kept in memory.
    pub fn history(operations: Arc<dyn OperationService>) -> Arc<dyn HistoryService> {
        crate::service::RayxHistory::new(operations, None, false, None, None)
    }

    /// RayX's history of `operations`' operations, kept in `store`.
    pub fn history_with_store(
        operations: Arc<dyn OperationService>,
        store: Arc<dyn HistoryStore>,
    ) -> Arc<dyn HistoryService> {
        crate::service::RayxHistory::new(operations, Some(store), true, None, None)
    }

    /// RayX's history of `operations`' operations, kept in `store` when there is one, and a
    /// function poisoning its state's lock (a thread panics holding it), for tests of what
    /// follows a panic.
    pub fn history_with_poison(
        operations: Arc<dyn OperationService>,
        store: Option<Arc<dyn HistoryStore>>,
    ) -> (Arc<dyn HistoryService>, Box<dyn Fn() + Send + Sync>) {
        let stored = store.is_some();
        let history = crate::service::RayxHistory::new(operations, store, stored, None, None);
        let poisoned = Arc::clone(&history);
        let poison = move || {
            let history = Arc::clone(&poisoned);
            let _ = std::thread::spawn(move || {
                let _held = history.scopes.lock();
                panic!("a thread panicked holding the history's state (a test's poison)");
            })
            .join();
        };
        (history, Box::new(poison))
    }

    /// RayX's history of `operations`' operations, kept in `store` when there is one, its calls
    /// bounded by `timeout` on `timer`.
    pub fn history_with_timer(
        operations: Arc<dyn OperationService>,
        store: Option<Arc<dyn HistoryStore>>,
        timer: crate::HistoryTimer,
        timeout: std::time::Duration,
    ) -> Arc<dyn HistoryService> {
        let stored = store.is_some();
        crate::service::RayxHistory::new(operations, store, stored, Some(timer), Some(timeout))
    }
}
