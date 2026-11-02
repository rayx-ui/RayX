mod app_ext;
#[cfg(feature = "diagnostics")]
mod diagnostics;
mod entry;
mod error;
mod event;
mod journal;
mod key_value_store;
mod lifecycle;
mod limits;
mod persistence;
mod retained_records;
mod scope;
mod service;
mod store;

pub use app_ext::HistoryAppBuilderExt;
#[cfg(feature = "diagnostics")]
pub use diagnostics::HistoryDiagnostics;
pub use entry::HistoryEntry;
pub use error::{HistoryError, HistoryNavigationFailure};
pub use event::HistoryEvent;
pub use journal::{HISTORY_JOURNAL_VERSION, HistoryJournalError, HistoryJournalV2};
pub use key_value_store::{HISTORY_BUCKET, KeyValueHistoryStore};
pub use limits::{HistoryLimitError, HistoryLimits, encoded_size, enforce};
pub use scope::HistoryScopeId;
pub use service::{HistoryService, HistorySubscription};
pub use store::{HistoryStore, HistoryStoreError, InMemoryHistoryStore};
