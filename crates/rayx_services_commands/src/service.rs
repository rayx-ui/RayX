//! RayX's command service: direct commands, contextual handler overrides resolved innermost-first
//! against [`ContextService`], per-command `IgnoreWhileRunning` execution guards, reactive
//! metadata publishing, and optional [`InputMapService`] shortcut / keybinding resolution.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, TryLockError, Weak};

use futures::future::BoxFuture;
use rayx_app::services::commands::{
    ActiveKeyBinding, CommandError, CommandEvent, CommandMetadata, CommandService, CommandState,
    ContextService,
};
use rayx_app::services::input_map::InputMapService;
use rayx_app::{ChangeSubscription, EventStream, StateCell};

use crate::keymap;

/// Dynamic or static title provider for a command.
pub type TitleProvider = Arc<dyn Fn() -> String + Send + Sync>;

/// Dynamic predicate reporting whether a command can currently execute.
pub type BoolProvider = Arc<dyn Fn() -> bool + Send + Sync>;

/// Dynamic predicate reporting a command's optional toggle/checked state.
pub type CheckedProvider = Arc<dyn Fn() -> Option<bool> + Send + Sync>;

/// Async command execution handler.
pub type CommandAction =
    Arc<dyn Fn(Option<serde_json::Value>) -> BoxFuture<'static, Result<(), String>> + Send + Sync>;

/// Definition of a direct command registered on [`RayxCommands`].
pub struct CommandDefinition {
    /// Unique command identifier (must not be blank).
    pub id: String,
    /// Human-readable title provider.
    pub title: TitleProvider,
    /// Optional grouping category.
    pub category: Option<String>,
    /// Optional classification kind.
    pub kind: Option<String>,
    /// Contexts in which this command is active (empty means global `""`).
    pub contexts: Vec<String>,
    /// Optional `can_execute` predicate (defaults to always `true` when in context).
    pub can_execute: Option<BoolProvider>,
    /// Optional checked/toggle predicate.
    pub checked: Option<CheckedProvider>,
    /// Handler invoked when the command executes.
    pub run: CommandAction,
}

impl CommandDefinition {
    /// Creates a direct command with a static title and synchronous Rust closure.
    pub fn sync(
        id: impl Into<String>,
        title: impl Into<String>,
        run: impl Fn(Option<serde_json::Value>) -> Result<(), String> + Send + Sync + 'static,
    ) -> Self {
        let title = title.into();
        Self {
            id: id.into(),
            title: Arc::new(move || title.clone()),
            category: None,
            kind: None,
            contexts: Vec::new(),
            can_execute: None,
            checked: None,
            run: Arc::new(move |args| Box::pin(futures::future::ready(run(args)))),
        }
    }

    /// Creates a direct command with a static title and async Rust handler.
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        run: impl Fn(Option<serde_json::Value>) -> BoxFuture<'static, Result<(), String>>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        let title = title.into();
        Self {
            id: id.into(),
            title: Arc::new(move || title.clone()),
            category: None,
            kind: None,
            contexts: Vec::new(),
            can_execute: None,
            checked: None,
            run: Arc::new(run),
        }
    }

    /// Sets the command's optional category.
    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    /// Sets the command's optional kind.
    pub fn with_kind(mut self, kind: impl Into<String>) -> Self {
        self.kind = Some(kind.into());
        self
    }

    /// Restricts this direct command to the given contexts (innermost check via `ContextService`).
    pub fn with_contexts(mut self, contexts: Vec<String>) -> Self {
        self.contexts = contexts;
        self
    }

    /// Replaces the title with a dynamic provider.
    pub fn with_dynamic_title(mut self, title: impl Fn() -> String + Send + Sync + 'static) -> Self {
        self.title = Arc::new(title);
        self
    }

    /// Sets the `can_execute` predicate.
    pub fn with_can_execute(mut self, can_execute: impl Fn() -> bool + Send + Sync + 'static) -> Self {
        self.can_execute = Some(Arc::new(can_execute));
        self
    }

    /// Sets the optional checked/toggle predicate.
    pub fn with_checked(mut self, checked: impl Fn() -> Option<bool> + Send + Sync + 'static) -> Self {
        self.checked = Some(Arc::new(checked));
        self
    }
}

/// Definition of a contextual handler registered for a command id in a specific context.
pub struct ContextCommandHandlerDefinition {
    /// Optional dynamic or static title override when this contextual handler is active.
    pub title: Option<TitleProvider>,
    /// Optional category override when this contextual handler is active.
    pub category: Option<String>,
    /// Optional kind override when this contextual handler is active.
    pub kind: Option<String>,
    /// Optional `can_execute` predicate (defaults to `true` when this handler is selected).
    pub can_execute: Option<BoolProvider>,
    /// Optional checked/toggle predicate override.
    pub checked: Option<CheckedProvider>,
    /// Handler invoked when the command executes in this context.
    pub run: CommandAction,
}

impl ContextCommandHandlerDefinition {
    /// Creates a contextual handler from a synchronous Rust closure.
    pub fn sync(
        run: impl Fn(Option<serde_json::Value>) -> Result<(), String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            title: None,
            category: None,
            kind: None,
            can_execute: None,
            checked: None,
            run: Arc::new(move |args| Box::pin(futures::future::ready(run(args)))),
        }
    }

    /// Creates a contextual handler from an async Rust handler.
    pub fn new(
        run: impl Fn(Option<serde_json::Value>) -> BoxFuture<'static, Result<(), String>>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self {
            title: None,
            category: None,
            kind: None,
            can_execute: None,
            checked: None,
            run: Arc::new(run),
        }
    }

    /// Overrides the command title while this handler is active.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        let title = title.into();
        self.title = Some(Arc::new(move || title.clone()));
        self
    }

    /// Overrides the command title dynamically while this handler is active.
    pub fn with_dynamic_title(mut self, title: impl Fn() -> String + Send + Sync + 'static) -> Self {
        self.title = Some(Arc::new(title));
        self
    }

    /// Sets the `can_execute` predicate for this contextual handler.
    pub fn with_can_execute(mut self, can_execute: impl Fn() -> bool + Send + Sync + 'static) -> Self {
        self.can_execute = Some(Arc::new(can_execute));
        self
    }

    /// Sets the optional checked/toggle predicate for this contextual handler.
    pub fn with_checked(mut self, checked: impl Fn() -> Option<bool> + Send + Sync + 'static) -> Self {
        self.checked = Some(Arc::new(checked));
        self
    }
}

/// RAII handle returned by [`RayxCommands::register`] and [`RayxCommands::register_handler`].
///
/// Dropping this handle unregisters the command or contextual handler unless
/// [`CommandRegistration::detach`] was called.
pub struct CommandRegistration {
    shared: Weak<Shared>,
    id: String,
    token: Option<u64>,
    context: Option<String>,
}

impl CommandRegistration {
    /// Unregisters immediately. Calling more than once is a no-op.
    pub fn unregister(mut self) {
        self.unregister_inner();
    }

    /// Keeps the command or handler registered for the lifetime of the service.
    pub fn detach(mut self) {
        self.token = None;
    }

    /// Consumes this guard without unregistering on drop, returning an explicit unregister closure.
    /// Used by JS/TS bindings where discarding the returned `() => void` handle must not cause
    /// garbage collection to silently unregister the command.
    pub fn into_unregister(mut self) -> impl FnOnce() + Send + Sync + 'static {
        let shared = self.shared.clone();
        let id = std::mem::take(&mut self.id);
        let token = self.token.take();
        let context = self.context.take();
        move || {
            let Some(token) = token else {
                return;
            };
            let Some(shared) = shared.upgrade() else {
                return;
            };
            let removed = match &context {
                None => shared.unregister_direct(&id, token),
                Some(ctx) => shared.unregister_contextual(&id, ctx, token),
            };
            if removed {
                shared.publish_latest();
                shared.events.publish(CommandEvent::Unregistered(id));
            }
        }
    }

    fn unregister_inner(&mut self) {
        let Some(token) = self.token.take() else {
            return;
        };
        let Some(shared) = self.shared.upgrade() else {
            return;
        };
        let removed = match &self.context {
            None => shared.unregister_direct(&self.id, token),
            Some(ctx) => shared.unregister_contextual(&self.id, ctx, token),
        };
        if removed {
            shared.publish_latest();
            shared.events.publish(CommandEvent::Unregistered(self.id.clone()));
        }
    }
}

impl Drop for CommandRegistration {
    fn drop(&mut self) {
        self.unregister_inner();
    }
}

struct DirectEntry {
    token: u64,
    title: TitleProvider,
    category: Option<String>,
    kind: Option<String>,
    contexts: Vec<String>,
    can_execute: Option<BoolProvider>,
    checked: Option<CheckedProvider>,
    run: CommandAction,
}

struct ContextHandlerEntry {
    token: u64,
    title: Option<TitleProvider>,
    category: Option<String>,
    kind: Option<String>,
    can_execute: Option<BoolProvider>,
    checked: Option<CheckedProvider>,
    run: CommandAction,
}

#[derive(Default)]
struct CommandSlot {
    /// Direct command registrations, newest last.
    direct: Vec<DirectEntry>,
    /// Contextual handlers keyed by context name (`""` for global fallback), newest last.
    contextual: BTreeMap<String, Vec<ContextHandlerEntry>>,
}

impl CommandSlot {
    fn is_empty(&self) -> bool {
        self.direct.is_empty() && self.contextual.values().all(Vec::is_empty)
    }

    fn all_contexts(&self) -> Vec<String> {
        let mut set = BTreeSet::new();
        if let Some(direct) = self.direct.last() {
            for ctx in &direct.contexts {
                if !ctx.is_empty() {
                    set.insert(ctx.clone());
                }
            }
        }
        for (ctx, entries) in &self.contextual {
            if !ctx.is_empty() && !entries.is_empty() {
                set.insert(ctx.clone());
            }
        }
        set.into_iter().collect()
    }
}

struct ResolvedEval {
    id: String,
    title: Option<TitleProvider>,
    category: Option<String>,
    kind: Option<String>,
    contexts: Vec<String>,
    in_context: bool,
    can_execute: Option<BoolProvider>,
    checked: Option<CheckedProvider>,
    is_executing: bool,
}

impl ResolvedEval {
    fn evaluate(self) -> CommandMetadata {
        let title = self
            .title
            .map_or_else(|| self.id.clone(), |provider| provider());
        let can_execute = if !self.in_context {
            false
        } else {
            self.can_execute.as_ref().is_none_or(|pred| pred())
        };
        let checked = self.checked.as_ref().and_then(|pred| pred());
        CommandMetadata {
            id: self.id,
            title,
            category: self.category,
            kind: self.kind,
            contexts: self.contexts,
            can_execute,
            is_executing: self.is_executing,
            checked,
        }
    }
}

struct ResolvedRun {
    can_execute: Option<BoolProvider>,
    run: CommandAction,
}

#[derive(Default)]
struct RegistryInner {
    next_token: u64,
    slots: BTreeMap<String, CommandSlot>,
    executing: HashSet<String>,
}

impl RegistryInner {
    fn next_token(&mut self) -> u64 {
        self.next_token += 1;
        self.next_token
    }

    fn resolve_contextual<'a>(
        slot: &'a CommandSlot,
        active_contexts: &[String],
    ) -> Option<&'a ContextHandlerEntry> {
        for ctx in active_contexts {
            if let Some(entry) = slot.contextual.get(ctx).and_then(|stack| stack.last()) {
                return Some(entry);
            }
        }
        slot.contextual.get("").and_then(|stack| stack.last())
    }

    fn direct_in_context(direct: &DirectEntry, active_contexts: &[String]) -> bool {
        if direct.contexts.is_empty() {
            return true;
        }
        direct
            .contexts
            .iter()
            .any(|ctx| ctx.is_empty() || active_contexts.iter().any(|active| active == ctx))
    }

    fn eval_spec(&self, id: &str, slot: &CommandSlot, active_contexts: &[String]) -> ResolvedEval {
        let direct = slot.direct.last();
        let contextual = Self::resolve_contextual(slot, active_contexts);
        let is_executing = self.executing.contains(id);
        let contexts = slot.all_contexts();

        if let Some(handler) = contextual {
            ResolvedEval {
                id: id.to_owned(),
                title: handler
                    .title
                    .clone()
                    .or_else(|| direct.map(|d| Arc::clone(&d.title))),
                category: handler
                    .category
                    .clone()
                    .or_else(|| direct.and_then(|d| d.category.clone())),
                kind: handler
                    .kind
                    .clone()
                    .or_else(|| direct.and_then(|d| d.kind.clone())),
                contexts,
                in_context: true,
                can_execute: handler.can_execute.clone(),
                checked: handler
                    .checked
                    .clone()
                    .or_else(|| direct.and_then(|d| d.checked.clone())),
                is_executing,
            }
        } else if let Some(direct) = direct {
            let in_context = Self::direct_in_context(direct, active_contexts);
            ResolvedEval {
                id: id.to_owned(),
                title: Some(Arc::clone(&direct.title)),
                category: direct.category.clone(),
                kind: direct.kind.clone(),
                contexts,
                in_context,
                can_execute: direct.can_execute.clone(),
                checked: direct.checked.clone(),
                is_executing,
            }
        } else {
            // Only contextual handlers exist, and none match the active contexts.
            ResolvedEval {
                id: id.to_owned(),
                title: None,
                category: None,
                kind: None,
                contexts,
                in_context: false,
                can_execute: None,
                checked: None,
                is_executing,
            }
        }
    }

    fn resolve_run(&self, id: &str, active_contexts: &[String]) -> Result<Option<ResolvedRun>, CommandError> {
        let Some(slot) = self.slots.get(id) else {
            return Err(CommandError::UnknownCommand(id.to_owned()));
        };
        if self.executing.contains(id) {
            return Err(CommandError::AlreadyExecuting(id.to_owned()));
        }
        if let Some(handler) = Self::resolve_contextual(slot, active_contexts) {
            return Ok(Some(ResolvedRun {
                can_execute: handler.can_execute.clone(),
                run: Arc::clone(&handler.run),
            }));
        }
        if let Some(direct) = slot.direct.last()
            && Self::direct_in_context(direct, active_contexts)
        {
            return Ok(Some(ResolvedRun {
                can_execute: direct.can_execute.clone(),
                run: Arc::clone(&direct.run),
            }));
        }
        Ok(None)
    }
}

struct Shared {
    context: Arc<dyn ContextService>,
    input_map: Option<Arc<dyn InputMapService>>,
    inner: Mutex<RegistryInner>,
    commands: StateCell<CommandState>,
    events: EventStream<CommandEvent>,
    revision: AtomicU64,
    publishing: Mutex<()>,
    stale: AtomicBool,
}

impl Shared {
    fn snapshot_metadata(&self) -> Vec<CommandMetadata> {
        let active_contexts = self.context.active_contexts();
        let specs: Vec<ResolvedEval> = {
            let inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            inner
                .slots
                .iter()
                .map(|(id, slot)| inner.eval_spec(id, slot, &active_contexts))
                .collect()
        };
        specs.into_iter().map(ResolvedEval::evaluate).collect()
    }

    fn eval_one(&self, id: &str) -> Option<CommandMetadata> {
        let active_contexts = self.context.active_contexts();
        let spec = {
            let inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            inner
                .slots
                .get(id)
                .map(|slot| inner.eval_spec(id, slot, &active_contexts))
        }?;
        Some(spec.evaluate())
    }

    fn unregister_direct(&self, id: &str, token: u64) -> bool {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(slot) = inner.slots.get_mut(id) else {
            return false;
        };
        let before = slot.direct.len();
        slot.direct.retain(|entry| entry.token != token);
        let removed = slot.direct.len() != before;
        if slot.is_empty() {
            inner.slots.remove(id);
        }
        removed
    }

    fn unregister_contextual(&self, id: &str, context: &str, token: u64) -> bool {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(slot) = inner.slots.get_mut(id) else {
            return false;
        };
        let removed = if let Some(stack) = slot.contextual.get_mut(context) {
            let before = stack.len();
            stack.retain(|entry| entry.token != token);
            let removed = stack.len() != before;
            if stack.is_empty() {
                slot.contextual.remove(context);
            }
            removed
        } else {
            false
        };
        if slot.is_empty() {
            inner.slots.remove(id);
        }
        removed
    }

    /// Recomputes and publishes `CommandState` outside `self.inner`'s lock so dynamic predicates
    /// and subscribers can safely call back into the service.
    fn publish_latest(&self) {
        self.stale.store(true, Ordering::SeqCst);
        loop {
            let publishing = match self.publishing.try_lock() {
                Ok(guard) => guard,
                Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
                Err(TryLockError::WouldBlock) => return,
            };
            self.stale.store(false, Ordering::SeqCst);
            let commands = self.snapshot_metadata();
            if self.commands.get().commands != commands {
                let revision = self.revision.fetch_add(1, Ordering::SeqCst) + 1;
                self.commands.publish(CommandState { commands, revision });
            }
            drop(publishing);
            if !self.stale.load(Ordering::SeqCst) {
                return;
            }
        }
    }
}

/// Ensures `id` is cleared from `executing` even if the execution future is cancelled or panics.
struct ExecutingGuard {
    shared: Arc<Shared>,
    id: Option<String>,
}

impl ExecutingGuard {
    fn new(shared: Arc<Shared>, id: String) -> Self {
        Self {
            shared,
            id: Some(id),
        }
    }

    fn finish(mut self) {
        if let Some(id) = self.id.take() {
            {
                let mut inner = self
                    .shared
                    .inner
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                inner.executing.remove(&id);
            }
            self.shared.publish_latest();
        }
    }
}

impl Drop for ExecutingGuard {
    fn drop(&mut self) {
        if let Some(id) = self.id.take() {
            {
                let mut inner = self
                    .shared
                    .inner
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                inner.executing.remove(&id);
            }
            self.shared.publish_latest();
        }
    }
}

/// RayX's implementation of [`CommandService`].
///
/// Observes the app's `dyn ContextService` to re-evaluate active handlers on context changes, and
/// optionally uses `dyn InputMapService` for shortcut lookup and active keybinding resolution.
pub struct RayxCommands {
    shared: Arc<Shared>,
    _context_subscription: ChangeSubscription,
}

impl RayxCommands {
    /// Creates a command service observing `context` and optionally querying `input_map`.
    pub fn new(
        context: Arc<dyn ContextService>,
        input_map: Option<Arc<dyn InputMapService>>,
    ) -> Self {
        let shared = Arc::new(Shared {
            context: Arc::clone(&context),
            input_map,
            inner: Mutex::default(),
            commands: StateCell::default(),
            events: EventStream::new(),
            revision: AtomicU64::new(0),
            publishing: Mutex::new(()),
            stale: AtomicBool::new(false),
        });
        let on_context = Arc::clone(&shared);
        let subscription = context.context().subscribe(move |_| {
            on_context.publish_latest();
        });
        Self {
            shared,
            _context_subscription: subscription,
        }
    }

    /// Registers a direct command. Dropping the returned [`CommandRegistration`] unregisters it.
    pub fn register(
        &self,
        definition: CommandDefinition,
    ) -> Result<CommandRegistration, CommandError> {
        let id = definition.id.trim().to_owned();
        if id.is_empty() {
            return Err(CommandError::InvalidArgument(
                "command id must not be blank".to_owned(),
            ));
        }
        let contexts: Vec<String> = definition
            .contexts
            .into_iter()
            .map(|ctx| ctx.trim().to_owned())
            .collect();

        let token = {
            let mut inner = self
                .shared
                .inner
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let token = inner.next_token();
            let slot = inner.slots.entry(id.clone()).or_default();
            slot.direct.push(DirectEntry {
                token,
                title: definition.title,
                category: definition.category,
                kind: definition.kind,
                contexts,
                can_execute: definition.can_execute,
                checked: definition.checked,
                run: definition.run,
            });
            token
        };

        self.shared.publish_latest();
        self.shared
            .events
            .publish(CommandEvent::Registered(id.clone()));

        Ok(CommandRegistration {
            shared: Arc::downgrade(&self.shared),
            id,
            token: Some(token),
            context: None,
        })
    }

    /// Registers a contextual handler for command `id` in `context` (`""` for global fallback).
    ///
    /// Contextual handlers are resolved innermost-context-first against [`ContextService`].
    /// Dropping the returned [`CommandRegistration`] unregisters this handler.
    pub fn register_handler(
        &self,
        id: impl Into<String>,
        context: impl Into<String>,
        handler: ContextCommandHandlerDefinition,
    ) -> Result<CommandRegistration, CommandError> {
        let id = id.into().trim().to_owned();
        if id.is_empty() {
            return Err(CommandError::InvalidArgument(
                "command id must not be blank".to_owned(),
            ));
        }
        let context = context.into().trim().to_owned();

        let token = {
            let mut inner = self
                .shared
                .inner
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let token = inner.next_token();
            let slot = inner.slots.entry(id.clone()).or_default();
            slot.contextual
                .entry(context.clone())
                .or_default()
                .push(ContextHandlerEntry {
                    token,
                    title: handler.title,
                    category: handler.category,
                    kind: handler.kind,
                    can_execute: handler.can_execute,
                    checked: handler.checked,
                    run: handler.run,
                });
            token
        };

        self.shared.publish_latest();
        self.shared
            .events
            .publish(CommandEvent::Registered(id.clone()));

        Ok(CommandRegistration {
            shared: Arc::downgrade(&self.shared),
            id,
            token: Some(token),
            context: Some(context),
        })
    }

    /// Re-evaluates all dynamic metadata (`title`, `can_execute`, `checked`) and publishes
    /// [`CommandState`] if anything changed.
    pub fn refresh(&self) {
        self.shared.publish_latest();
    }

    /// Evaluates whether `id` can execute right now by invoking its active predicate live.
    pub fn can_execute_live(&self, id: &str) -> bool {
        self.shared
            .eval_one(id)
            .is_some_and(|meta| meta.can_execute)
    }

    /// Evaluates whether `id` is currently executing.
    pub fn is_executing_live(&self, id: &str) -> bool {
        self.shared
            .inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .executing
            .contains(id)
    }

    /// Evaluates the active title of `id` live, returning `None` if `id` is not registered.
    pub fn title_live(&self, id: &str) -> Option<String> {
        self.shared.eval_one(id).map(|meta| meta.title)
    }

    /// Evaluates the active checked state of `id` live.
    pub fn checked_live(&self, id: &str) -> Option<bool> {
        self.shared.eval_one(id).and_then(|meta| meta.checked)
    }

    /// Evaluates fresh metadata for all registered commands, sorted by `id`.
    pub fn all_live(&self) -> Vec<CommandMetadata> {
        self.shared.snapshot_metadata()
    }

    /// Returns whether `id` is registered (either direct or contextual).
    pub fn has_command(&self, id: &str) -> bool {
        self.shared
            .inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .slots
            .contains_key(id)
    }

    /// Returns the preferred NSH `+`-syntax shortcut for `id` using the active contexts and
    /// registered [`InputMapService`], or `None` if no input map or keyboard binding exists.
    pub fn shortcut_for(&self, id: &str) -> Option<String> {
        let input_map = self.shared.input_map.as_ref()?;
        let contexts = self.shared.context.active_contexts();
        keymap::shortcut_for(id, &contexts, &input_map.input_map().get())
    }

    /// Computes the deduplicated active NSH keymap bindings for the current active contexts and
    /// registered [`InputMapService`]. Returns an empty list if no input map is registered.
    pub fn resolve_active_bindings(&self) -> Vec<ActiveKeyBinding> {
        let Some(input_map) = self.shared.input_map.as_ref() else {
            return Vec::new();
        };
        let contexts = self.shared.context.active_contexts();
        keymap::resolve_active_bindings(&contexts, &input_map.input_map().get())
    }
}

impl CommandService for RayxCommands {
    fn commands(&self) -> &StateCell<CommandState> {
        &self.shared.commands
    }

    fn events(&self) -> &EventStream<CommandEvent> {
        &self.shared.events
    }

    fn execute(
        &self,
        id: String,
        arguments: Option<serde_json::Value>,
    ) -> BoxFuture<'_, Result<bool, CommandError>> {
        let shared = Arc::clone(&self.shared);
        let active_contexts = shared.context.active_contexts();
        let resolved = {
            let inner = shared.inner.lock().unwrap_or_else(PoisonError::into_inner);
            inner.resolve_run(&id, &active_contexts)
        };
        let resolved = match resolved {
            Ok(Some(resolved)) => resolved,
            Ok(None) => return Box::pin(futures::future::ready(Ok(false))),
            Err(error) => return Box::pin(futures::future::ready(Err(error))),
        };

        // Evaluate `can_execute` outside the registry lock.
        if let Some(predicate) = &resolved.can_execute
            && !predicate()
        {
            shared.publish_latest();
            return Box::pin(futures::future::ready(Ok(false)));
        }

        // Atomically claim the single-flight slot (`IgnoreWhileRunning`) synchronously on call.
        {
            let mut inner = shared.inner.lock().unwrap_or_else(PoisonError::into_inner);
            if !inner.executing.insert(id.clone()) {
                return Box::pin(futures::future::ready(Err(CommandError::AlreadyExecuting(
                    id,
                ))));
            }
        }
        shared.publish_latest();

        let guard = ExecutingGuard::new(Arc::clone(&shared), id.clone());
        let run_future = (resolved.run)(arguments);

        Box::pin(async move {
            let outcome = run_future.await;
            guard.finish();

            match outcome {
                Ok(()) => {
                    shared.events.publish(CommandEvent::Executed { id });
                    Ok(true)
                }
                Err(message) => {
                    shared.events.publish(CommandEvent::Failed {
                        id: id.clone(),
                        message: message.clone(),
                    });
                    Err(CommandError::ExecutionFailed { id, message })
                }
            }
        })
    }
}
