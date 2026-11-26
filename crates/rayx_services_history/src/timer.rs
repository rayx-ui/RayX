//! What bounds the history's calls in time: [`HistoryTimer`], and [`HistoryOptions`]' timeout.

use std::sync::Arc;
use std::time::Duration;

use futures::FutureExt as _;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};

/// The configuration section [`HistoryOptions`] bind from.
pub const HISTORY_CONFIG_PATH: &str = "RayX:Services:History";

/// How RayX's history bounds the calls it makes of the operation service and the store.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HistoryOptions {
    /// How long a call of the operation service (beginning a transaction, submitting a
    /// record) or of the store may take, in milliseconds, before the command making it fails
    /// with `HistoryError::TimedOut`, aborting its transaction and freeing its scope (`None`:
    /// unbounded). A TS implementation whose promise never settles would otherwise keep the
    /// scope busy for the app's life. 30 seconds by default: a TS service or store that
    /// legitimately takes longer has its calls aborted, so raise it for one (or set `None`). A
    /// commit is never bounded: once it is asked, its change lands, and the scope stays busy
    /// until it answers, so a commit that never answers keeps its scope busy (`ScopeBusy`) for
    /// good. That is the trade-off for never undoing a change twice; an operation service
    /// recovers its scope by answering the commit, or failing it (the replay then fails and
    /// frees the scope).
    pub call_timeout_ms: Option<u64>,
    /// Whether operations with [`rayx_app::services::operations::OperationOrigin::Agent`] are
    /// recorded as undoable history entries alongside
    /// [`rayx_app::services::operations::OperationOrigin::LocalUser`]. Enabled by default.
    pub record_agent_operations: bool,
}

impl Default for HistoryOptions {
    fn default() -> Self {
        Self {
            call_timeout_ms: Some(30_000),
            record_agent_operations: true,
        }
    }
}

impl HistoryOptions {
    /// The timeout, as a duration.
    pub fn call_timeout(&self) -> Option<Duration> {
        self.call_timeout_ms.map(Duration::from_millis)
    }
}

/// The timer RayX's history bounds its calls with: a future completing after the given
/// duration, from any thread. On NSH the history uses the engine's timers (through the host's
/// `EngineRunner`); an app without them (or a test) registers its own as a service, and with
/// neither the calls are unbounded.
#[derive(Clone)]
pub struct HistoryTimer(Arc<dyn Fn(Duration) -> BoxFuture<'static, ()> + Send + Sync>);

impl HistoryTimer {
    /// The timer whose futures `after` makes.
    pub fn new(after: impl Fn(Duration) -> BoxFuture<'static, ()> + Send + Sync + 'static) -> Self {
        Self(Arc::new(after))
    }

    /// The engine's timers, through `runner`: a future completing `duration` from now (never,
    /// once the engine stopped).
    pub fn engine(runner: rayx_app_nsh::EngineRunner) -> Self {
        Self::new(move |duration| {
            runner
                .run_cancellable(move || nsh_core::prelude::timer(duration))
                .then(|ran| async move {
                    if ran.is_err() {
                        futures::future::pending::<()>().await;
                    }
                })
                .boxed()
        })
    }

    /// A future completing after `duration`.
    pub fn after(&self, duration: Duration) -> BoxFuture<'static, ()> {
        (self.0)(duration)
    }
}

impl std::fmt::Debug for HistoryTimer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HistoryTimer")
            .finish_non_exhaustive()
    }
}
