//! RayX's context service: a token-based push/pop stack layered ahead of declarative base
//! contexts, deduplicated innermost-first and published as [`ContextState`].

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, TryLockError, Weak};

use futures::FutureExt as _;
use futures::future::BoxFuture;
use rayx_app::StateCell;
use rayx_app::services::commands::{CommandError, ContextService, ContextState};

/// RAII handle returned by [`RayxContext::enter`]: pops the pushed context when dropped unless
/// [`ContextGuard::detach`] is called.
pub struct ContextGuard {
    shared: Weak<Shared>,
    token: Option<u64>,
}

impl ContextGuard {
    /// Pops the pushed context immediately. Calling more than once is a no-op.
    pub fn leave(mut self) -> bool {
        self.pop_inner()
    }

    /// Releases the guard without popping the context.
    pub fn detach(mut self) {
        self.token = None;
    }

    /// The underlying token id, if still active.
    pub fn token(&self) -> Option<u64> {
        self.token
    }

    fn pop_inner(&mut self) -> bool {
        let Some(token) = self.token.take() else {
            return false;
        };
        self.shared
            .upgrade()
            .is_some_and(|shared| shared.pop(token))
    }
}

impl Drop for ContextGuard {
    fn drop(&mut self) {
        let _ = self.pop_inner();
    }
}

/// RayX's implementation of [`ContextService`].
///
/// Combines a push/pop scope stack (newest pushed context is innermost) with declarative base
/// contexts (`set_contexts`), deduplicating while preserving innermost-first order. The empty
/// global context `""` is omitted from the published list and is always implicitly active last.
pub struct RayxContext {
    shared: Arc<Shared>,
}

impl RayxContext {
    /// Creates an empty context service (only the implicit global `""` context is active).
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Shared::default()),
        }
    }

    /// Synchronously pushes `context` onto the stack (innermost) and returns its token id.
    pub fn push_sync(&self, context: impl Into<String>) -> Result<u64, CommandError> {
        self.shared.push(context.into())
    }

    /// Synchronously removes the pushed context identified by `token`; returns whether it existed.
    pub fn pop_sync(&self, token: u64) -> bool {
        self.shared.pop(token)
    }

    /// Synchronously replaces the declarative base contexts (innermost first).
    pub fn set_contexts_sync(&self, contexts: Vec<String>) {
        self.shared.set_contexts(contexts);
    }

    /// Pushes `context` synchronously and returns an RAII [`ContextGuard`] that pops it on drop.
    pub fn enter(&self, context: impl Into<String>) -> Result<ContextGuard, CommandError> {
        let token = self.shared.push(context.into())?;
        Ok(ContextGuard {
            shared: Arc::downgrade(&self.shared),
            token: Some(token),
        })
    }
}

impl Default for RayxContext {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Default)]
struct ContextInner {
    next_token: u64,
    /// Pushed entries in push order (oldest first, newest/innermost last).
    pushed: Vec<(u64, String)>,
    /// Declarative base contexts (innermost first).
    base: Vec<String>,
    /// Current deduplicated state.
    state: ContextState,
}

impl ContextInner {
    /// Recomputes `state.contexts` from `pushed` (newest first) followed by `base` (in order),
    /// skipping empty entries and preserving first occurrence (innermost-first). Returns whether
    /// `state.contexts` changed.
    fn recompute(&mut self) -> bool {
        let mut next = Vec::with_capacity(self.pushed.len() + self.base.len());
        for (_, ctx) in self.pushed.iter().rev() {
            if !ctx.is_empty() && !next.iter().any(|existing| existing == ctx) {
                next.push(ctx.clone());
            }
        }
        for ctx in &self.base {
            let trimmed = ctx.trim();
            if !trimmed.is_empty() && !next.iter().any(|existing| existing == trimmed) {
                next.push(trimmed.to_owned());
            }
        }
        if self.state.contexts == next {
            return false;
        }
        self.state.contexts = next;
        self.state.revision += 1;
        true
    }
}

#[derive(Default)]
struct Shared {
    inner: Mutex<ContextInner>,
    cell: StateCell<ContextState>,
    publishing: Mutex<()>,
    stale: AtomicBool,
}

impl Shared {
    fn push(&self, context: String) -> Result<u64, CommandError> {
        let trimmed = context.trim();
        if trimmed.is_empty() {
            return Err(CommandError::InvalidArgument(
                "context name must not be blank".to_owned(),
            ));
        }
        let context = trimmed.to_owned();
        let (token, changed) = {
            let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            inner.next_token += 1;
            let token = inner.next_token;
            inner.pushed.push((token, context));
            let changed = inner.recompute();
            (token, changed)
        };
        if changed {
            self.publish_latest();
        }
        Ok(token)
    }

    fn pop(&self, token: u64) -> bool {
        let (removed, changed) = {
            let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            let before = inner.pushed.len();
            inner.pushed.retain(|(id, _)| *id != token);
            let removed = inner.pushed.len() != before;
            let changed = removed && inner.recompute();
            (removed, changed)
        };
        if changed {
            self.publish_latest();
        }
        removed
    }

    fn set_contexts(&self, contexts: Vec<String>) {
        let changed = {
            let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            inner.base = contexts;
            inner.recompute()
        };
        if changed {
            self.publish_latest();
        }
    }

    /// Publishes the latest state outside `self.inner`'s lock so subscribers may call back into
    /// the service without deadlocking.
    fn publish_latest(&self) {
        self.stale.store(true, Ordering::SeqCst);
        loop {
            let publishing = match self.publishing.try_lock() {
                Ok(guard) => guard,
                Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
                Err(TryLockError::WouldBlock) => return,
            };
            self.stale.store(false, Ordering::SeqCst);
            let snapshot = self
                .inner
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .state
                .clone();
            if *self.cell.get() != snapshot {
                self.cell.publish(snapshot);
            }
            drop(publishing);
            if !self.stale.load(Ordering::SeqCst) {
                return;
            }
        }
    }
}

impl ContextService for RayxContext {
    fn context(&self) -> &StateCell<ContextState> {
        &self.shared.cell
    }

    fn push_context(&self, context: String) -> BoxFuture<'_, Result<u64, CommandError>> {
        futures::future::ready(self.shared.push(context)).boxed()
    }

    fn pop_context(&self, token: u64) -> BoxFuture<'_, Result<bool, CommandError>> {
        futures::future::ready(Ok(self.shared.pop(token))).boxed()
    }

    fn set_contexts(&self, contexts: Vec<String>) -> BoxFuture<'_, Result<(), CommandError>> {
        self.shared.set_contexts(contexts);
        futures::future::ready(Ok(())).boxed()
    }
}
