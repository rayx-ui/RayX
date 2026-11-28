//! The context and command services for TSX.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use nsh_core::JsonValue;
use nsh_core::element::{
    Arg, JsHost, JsIn, JsKind, JsOwned, PropError, Settles, TsTypes, start_task,
};
use nsh_core::prelude::*;
use rayx_app::ChangeSubscription;
use rayx_app::di::ThreadBound;
use rayx_app::services::commands::{CommandService as Commands, ContextService as Contexts};
use rayx_app::services::input_map::InputMapService;
use rayx_app_nsh::EngineRunner;
use rayx_app_nsh::ts::{Argument, Listeners, ServiceClass, json};

use crate::context::RayxContext;
use crate::keymap;
use crate::service::{
    BoolProvider, CheckedProvider, CommandAction, CommandDefinition,
    ContextCommandHandlerDefinition, RayxCommands, TitleProvider,
};

/// Returns `concrete` if it is the exact backing instance of `service` (safe pointer identity
/// check; returns `None` when the app replaced the trait registration with its own implementation).
fn backing<T: ?Sized, C>(service: &Arc<T>, concrete: Option<Arc<C>>) -> Option<Arc<C>> {
    concrete.filter(|candidate| {
        let dyn_ptr: *const () = Arc::as_ptr(service).cast();
        let concrete_ptr: *const () = Arc::as_ptr(candidate).cast();
        std::ptr::eq(dyn_ptr, concrete_ptr)
    })
}

/// Coordinates `ContextService.enter(ctx)` when backed by a custom async `dyn ContextService`.
enum EnterState {
    Pending,
    Pushed(u64),
    Cancelled,
}

/// The app's active context stack, for TSX: `services.getRequiredService(ContextService)`.
///
/// Tracks innermost-first active contexts (`activeContexts()`, `hasContext(ctx)`), scoped context
/// entry (`enter(ctx)` returning a `() => void` leave handle that also cleans up with its reactive
/// owner), declarative base contexts (`setContexts(ctxs)`), and change listeners (`onChange`).
///
/// Wraps whichever `dyn ContextService` is registered (RayX's via `addCommands()`, or a TS
/// implementation of `ContextServiceContract` registered under this class).
#[class]
pub struct ContextService {
    service: Arc<dyn Contexts>,
    rayx: Option<Arc<RayxContext>>,
    listeners: Listeners,
}

impl ServiceClass for ContextService {
    type Service = dyn Contexts;

    fn wrap(service: Arc<dyn Contexts>) -> Self {
        Self {
            service,
            rayx: None,
            listeners: Listeners::default(),
        }
    }

    fn wrap_in(service: Arc<dyn Contexts>, services: &rayx_app::ServiceProvider) -> Self {
        let rayx = backing(&service, services.service::<RayxContext>());
        Self {
            service,
            rayx,
            listeners: Listeners::default(),
        }
    }
}

fn context_service(this: &Instance<ContextService>) -> Arc<dyn Contexts> {
    this.with(|this| Arc::clone(&this.service))
}

#[class]
impl ContextService {
    /// The contract this class is the token of (TS types only: `null` at run time).
    #[getter]
    #[ts("(): ContextServiceContract | null")]
    pub fn contract(&self) -> Option<bool> {
        None
    }

    /// The current context state: `{ contexts, revision }`.
    #[ts("(): ContextState")]
    pub fn state(&self) -> JsonValue {
        json(&*self.service.context().get())
    }

    /// The active context names now, innermost first (never including `""`).
    pub fn active_contexts(&self) -> Vec<String> {
        self.service.active_contexts()
    }

    /// Whether `context` is currently active (`""` is always active as the global context).
    pub fn has_context(&self, context: String) -> bool {
        self.service.has_context(&context)
    }

    /// Enters `context` (pushing it as the innermost active context) and returns a `() => void`
    /// function that leaves it; also leaves automatically when the calling reactive owner cleans
    /// up.
    pub fn enter(&self, context: String, cx: CallCx) -> Result<Stop, String> {
        if let Some(rayx) = &self.rayx {
            let token = rayx.push_sync(context).map_err(|error| error.to_string())?;
            let weak = Arc::downgrade(rayx);
            return Ok(Stop::new(move || {
                if let Some(rayx) = weak.upgrade() {
                    rayx.pop_sync(token);
                }
            }));
        }

        if context.trim().is_empty() {
            return Err("context name must not be blank".to_owned());
        }

        let state = Arc::new(Mutex::new(EnterState::Pending));
        let push_state = Arc::clone(&state);
        let push_service = Arc::clone(&self.service);
        cx.start(async move {
            if let Ok(token) = push_service.push_context(context).await {
                let cancelled = {
                    let mut guard = push_state.lock().unwrap_or_else(PoisonError::into_inner);
                    match *guard {
                        EnterState::Cancelled => true,
                        _ => {
                            *guard = EnterState::Pushed(token);
                            false
                        }
                    }
                };
                if cancelled {
                    let _ = push_service.pop_context(token).await;
                }
            }
        });

        let pop_service = Arc::clone(&self.service);
        Ok(Stop::new(move || {
            let token = {
                let mut guard = state.lock().unwrap_or_else(PoisonError::into_inner);
                match *guard {
                    EnterState::Pushed(token) => {
                        *guard = EnterState::Cancelled;
                        Some(token)
                    }
                    EnterState::Pending => {
                        *guard = EnterState::Cancelled;
                        None
                    }
                    EnterState::Cancelled => None,
                }
            };
            if let Some(token) = token {
                let _ = start_task(async move {
                    let _ = pop_service.pop_context(token).await;
                });
            }
        }))
    }

    /// Replaces the declarative base contexts (innermost first). Pushed contexts remain ahead of
    /// the declarative base contexts.
    pub fn set_contexts(&self, contexts: Vec<String>, cx: CallCx) {
        if let Some(rayx) = &self.rayx {
            rayx.set_contexts_sync(contexts);
            return;
        }
        let service = Arc::clone(&self.service);
        cx.start(async move {
            let _ = service.set_contexts(contexts).await;
        });
    }

    /// Pushes `context` onto the context stack and resolves with its numeric token id.
    pub async fn push_context(this: Instance<Self>, context: String) -> Result<u64, String> {
        context_service(&this)
            .push_context(context)
            .await
            .map_err(|error| error.to_string())
    }

    /// Pops the pushed context identified by `token`; resolves with whether it was present.
    pub async fn pop_context(this: Instance<Self>, token: u64) -> Result<bool, String> {
        context_service(&this)
            .pop_context(token)
            .await
            .map_err(|error| error.to_string())
    }

    /// Calls `listener` with `{ contexts, revision }` after every change, until the returned
    /// function is called (or the calling reactive owner cleans up), or this object is disposed.
    #[ts("(listener: (state: ContextState) => void): () => void")]
    pub fn on_change(
        &mut self,
        #[args(state)] listener: Callback<JsonValue>,
    ) -> Result<Stop, String> {
        let service = Arc::clone(&self.service);
        self.listeners.listen(listener, move |post| {
            Ok::<ChangeSubscription, String>(
                service
                    .context()
                    .subscribe(move |state| post(json(&**state))),
            )
        })
    }
}

impl ContextService {
    /// The underlying Rust service, for bridges taking `Instance<ContextService>`.
    pub fn service(&self) -> &Arc<dyn Contexts> {
        &self.service
    }
}

// --- JS descriptor parsing ---------------------------------------------------------------

enum TsTitle {
    Static(String),
    Dynamic(Callback<(), String>),
}

/// Parsed JS descriptor passed to `CommandService.register(descriptor)`.
pub struct CommandDescriptorArg {
    id: String,
    title: TsTitle,
    category: Option<String>,
    kind: Option<String>,
    contexts: Vec<String>,
    can_execute: Option<Callback<(), bool>>,
    checked: Option<Callback<(), Option<bool>>>,
    run: Callback<Argument, Settles>,
}

/// Parsed JS handler object passed to `CommandService.registerHandler(id, context, handler)`.
pub struct ContextHandlerArg {
    title: Option<TsTitle>,
    category: Option<String>,
    kind: Option<String>,
    can_execute: Option<Callback<(), bool>>,
    checked: Option<Callback<(), Option<bool>>>,
    run: Callback<Argument, Settles>,
}

fn read_optional_string(obj: JsIn<'_>, field: &str) -> Result<Option<String>, PropError> {
    let raw = obj.field(field)?;
    let value = raw.get();
    if value.is_nullish() {
        return Ok(None);
    }
    <String as JsValue>::from_js(value)
        .map(Some)
        .map_err(|error| PropError(format!("`{field}`: {}", error.0)))
}

fn read_optional_contexts(obj: JsIn<'_>) -> Result<Vec<String>, PropError> {
    let raw = obj.field("contexts")?;
    let value = raw.get();
    if value.is_nullish() {
        return Ok(Vec::new());
    }
    <Vec<String> as JsValue>::from_js(value)
        .map_err(|error| PropError(format!("`contexts`: {}", error.0)))
}

fn read_title(obj: JsIn<'_>, required: bool) -> Result<Option<TsTitle>, PropError> {
    let raw = obj.field("title")?;
    let value = raw.get();
    if value.is_nullish() {
        if required {
            return Err(PropError(
                "`title` is required (string or () => string)".into(),
            ));
        }
        return Ok(None);
    }
    match value.kind() {
        JsKind::Function => <Callback<(), String> as Arg>::from_js(value)
            .map(|cb| Some(TsTitle::Dynamic(cb)))
            .map_err(|error| PropError(format!("`title`: {}", error.0))),
        JsKind::String => <String as JsValue>::from_js(value)
            .map(|s| Some(TsTitle::Static(s)))
            .map_err(|error| PropError(format!("`title`: {}", error.0))),
        _ => Err(PropError(format!(
            "`title`: {}",
            value.expected("a string or () => string").0
        ))),
    }
}

fn read_optional_can_execute(obj: JsIn<'_>) -> Result<Option<Callback<(), bool>>, PropError> {
    let raw = obj.field("canExecute")?;
    let value = raw.get();
    if value.is_nullish() {
        return Ok(None);
    }
    <Callback<(), bool> as Arg>::from_js(value)
        .map(Some)
        .map_err(|error| PropError(format!("`canExecute`: {}", error.0)))
}

fn read_optional_checked(obj: JsIn<'_>) -> Result<Option<Callback<(), Option<bool>>>, PropError> {
    let raw = obj.field("checked")?;
    let value = raw.get();
    if value.is_nullish() {
        return Ok(None);
    }
    <Callback<(), Option<bool>> as Arg>::from_js(value)
        .map(Some)
        .map_err(|error| PropError(format!("`checked`: {}", error.0)))
}

fn read_run(obj: JsIn<'_>) -> Result<Callback<Argument, Settles>, PropError> {
    let raw = obj.field("run")?;
    let value = raw.get();
    <Callback<Argument, Settles> as Arg>::from_js(value)
        .map_err(|error| PropError(format!("`run`: {}", error.0)))
}

impl JsValue for CommandDescriptorArg {
    fn ts_type(_types: &mut TsTypes) -> String {
        "CommandDescriptor".into()
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        if value.kind() != JsKind::Object {
            return Err(value.expected("a CommandDescriptor object"));
        }
        let id_raw = value.field("id")?;
        let id = <String as JsValue>::from_js(id_raw.get())
            .map_err(|error| PropError(format!("`id`: {}", error.0)))?;
        let title =
            read_title(value, true)?.ok_or_else(|| PropError("`title` is required".into()))?;
        Ok(Self {
            id,
            title,
            category: read_optional_string(value, "category")?,
            kind: read_optional_string(value, "kind")?,
            contexts: read_optional_contexts(value)?,
            can_execute: read_optional_can_execute(value)?,
            checked: read_optional_checked(value)?,
            run: read_run(value)?,
        })
    }

    fn to_js<'a>(&self, _host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        Err(PropError(
            "CommandDescriptor is an input argument only".into(),
        ))
    }
}

impl JsValue for ContextHandlerArg {
    fn ts_type(_types: &mut TsTypes) -> String {
        "ContextCommandHandler".into()
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        if value.kind() != JsKind::Object {
            return Err(value.expected("a ContextCommandHandler object"));
        }
        Ok(Self {
            title: read_title(value, false)?,
            category: read_optional_string(value, "category")?,
            kind: read_optional_string(value, "kind")?,
            can_execute: read_optional_can_execute(value)?,
            checked: read_optional_checked(value)?,
            run: read_run(value)?,
        })
    }

    fn to_js<'a>(&self, _host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        Err(PropError(
            "ContextCommandHandler is an input argument only".into(),
        ))
    }
}

// --- Bridges from JS callbacks to `Send + Sync` Rust command providers -------------------

fn make_title_provider(title: TsTitle) -> Result<TitleProvider, String> {
    match title {
        TsTitle::Static(text) => Ok(Arc::new(move || text.clone())),
        TsTitle::Dynamic(callback) => {
            let initial = if callback.can_run_now() {
                callback.call(()).unwrap_or_default()
            } else {
                String::new()
            };
            let kept = ThreadBound::new(callback, "a command title accessor")
                .map_err(|error| error.to_string())?;
            let cached = Mutex::new(initial);
            Ok(Arc::new(move || {
                if let Ok(cb) = kept.get()
                    && cb.can_run_now()
                    && let Ok(value) = cb.call(())
                {
                    *cached.lock().unwrap_or_else(PoisonError::into_inner) = value.clone();
                    return value;
                }
                cached
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .clone()
            }))
        }
    }
}

fn make_bool_provider(callback: Callback<(), bool>) -> Result<BoolProvider, String> {
    let initial = if callback.can_run_now() {
        callback.call(()).unwrap_or(true)
    } else {
        true
    };
    let kept = ThreadBound::new(callback, "a command canExecute accessor")
        .map_err(|error| error.to_string())?;
    let cached = AtomicBool::new(initial);
    Ok(Arc::new(move || {
        if let Ok(cb) = kept.get()
            && cb.can_run_now()
            && let Ok(value) = cb.call(())
        {
            cached.store(value, Ordering::Relaxed);
            return value;
        }
        cached.load(Ordering::Relaxed)
    }))
}

fn make_checked_provider(callback: Callback<(), Option<bool>>) -> Result<CheckedProvider, String> {
    let initial = if callback.can_run_now() {
        callback.call(()).ok().flatten()
    } else {
        None
    };
    let kept = ThreadBound::new(callback, "a command checked accessor")
        .map_err(|error| error.to_string())?;
    let cached = Mutex::new(initial);
    Ok(Arc::new(move || {
        if let Ok(cb) = kept.get()
            && cb.can_run_now()
            && let Ok(value) = cb.call(())
        {
            *cached.lock().unwrap_or_else(PoisonError::into_inner) = value;
            return value;
        }
        *cached.lock().unwrap_or_else(PoisonError::into_inner)
    }))
}

fn make_run_action(
    callback: Callback<Argument, Settles>,
    runner: Option<Arc<EngineRunner>>,
) -> Result<CommandAction, String> {
    let kept = Arc::new(
        ThreadBound::new(callback, "a command run handler").map_err(|error| error.to_string())?,
    );
    Ok(Arc::new(move |args: Option<JsonValue>| {
        let kept = Arc::clone(&kept);
        let runner = runner.clone();
        if let Some(runner) = runner {
            let fut = runner.run(move || async move {
                let cb = kept.get().map_err(|error| error.to_string())?;
                let arg = match args {
                    Some(value) => Argument::Value(value),
                    None => Argument::None,
                };
                cb.call_async(arg).await.map(|_| ())
            });
            Box::pin(async move {
                match fut.await {
                    Ok(result) => result,
                    Err(_) => Err("the surface's engine is gone".to_owned()),
                }
            })
        } else {
            // Fallback when running on the engine thread without a registered EngineRunner.
            let (send, receive) = futures::channel::oneshot::channel();
            let started = start_task(async move {
                let result = match kept.get() {
                    Ok(cb) => {
                        let arg = match args {
                            Some(value) => Argument::Value(value),
                            None => Argument::None,
                        };
                        cb.call_async(arg).await.map(|_| ())
                    }
                    Err(error) => Err(error.to_string()),
                };
                let _ = send.send(result);
            });
            if started.is_err() {
                return Box::pin(futures::future::ready(Err(
                    "the surface's engine is gone".to_owned()
                )));
            }
            Box::pin(async move {
                receive
                    .await
                    .unwrap_or_else(|_| Err("the surface's engine is gone".to_owned()))
            })
        }
    }))
}

/// The app's command registry and dispatcher, for TSX: `services.getRequiredService(CommandService)`.
///
/// Registers direct commands (`register(descriptor)`) and contextual handlers
/// (`registerHandler(id, context, handler)`), executes commands with single-flight protection
/// (`execute(id, args?)`), queries live metadata (`canExecute`, `isExecuting`, `title`, `checked`,
/// `all`, `has`), resolves shortcuts and active keybindings via `InputMapService` (`shortcutFor`,
/// `resolveActiveBindings`), and notifies listeners (`onChange`, `onEvent`).
///
/// Wraps whichever `dyn CommandService` is registered (RayX's via `addCommands()`, or a TS
/// implementation of `CommandServiceContract` registered under this class).
#[class]
pub struct CommandService {
    service: Arc<dyn Commands>,
    rayx: Option<Arc<RayxCommands>>,
    context: Option<Arc<dyn Contexts>>,
    input_map: Option<Arc<dyn InputMapService>>,
    runner: Option<Arc<EngineRunner>>,
    listeners: Listeners,
}

impl ServiceClass for CommandService {
    type Service = dyn Commands;

    fn wrap(service: Arc<dyn Commands>) -> Self {
        Self {
            service,
            rayx: None,
            context: None,
            input_map: None,
            runner: None,
            listeners: Listeners::default(),
        }
    }

    fn wrap_in(service: Arc<dyn Commands>, services: &rayx_app::ServiceProvider) -> Self {
        let rayx = backing(&service, services.service::<RayxCommands>());
        let context = services.service::<dyn Contexts>();
        let input_map = services.service::<dyn InputMapService>();
        let runner = services.service::<EngineRunner>();
        Self {
            service,
            rayx,
            context,
            input_map,
            runner,
            listeners: Listeners::default(),
        }
    }
}

fn command_service(this: &Instance<CommandService>) -> Arc<dyn Commands> {
    this.with(|this| Arc::clone(&this.service))
}

#[class]
impl CommandService {
    /// The contract this class is the token of (TS types only: `null` at run time).
    #[getter]
    #[ts("(): CommandServiceContract | null")]
    pub fn contract(&self) -> Option<bool> {
        None
    }

    /// The published command state: `{ commands, revision }`.
    #[ts("(): CommandState")]
    pub fn state(&self) -> JsonValue {
        json(&*self.service.commands().get())
    }

    /// Registers a direct command and returns a `() => void` unregister handle (also unregisters
    /// automatically when the calling reactive owner cleans up).
    #[ts("(descriptor: CommandDescriptor): () => void")]
    pub fn register(&self, descriptor: CommandDescriptorArg) -> Result<Stop, String> {
        let Some(rayx) = &self.rayx else {
            return Err(
                "command registration requires RayX's command service (`addCommands()`)".to_owned(),
            );
        };
        let definition = CommandDefinition {
            id: descriptor.id,
            title: make_title_provider(descriptor.title)?,
            category: descriptor.category,
            kind: descriptor.kind,
            contexts: descriptor.contexts,
            can_execute: descriptor.can_execute.map(make_bool_provider).transpose()?,
            checked: descriptor.checked.map(make_checked_provider).transpose()?,
            run: make_run_action(descriptor.run, self.runner.clone())?,
        };
        let registration = rayx
            .register(definition)
            .map_err(|error| error.to_string())?;
        Ok(Stop::new(registration.into_unregister()))
    }

    /// Registers a contextual handler for command `id` in `context` (`""` for global fallback) and
    /// returns a `() => void` unregister handle (also unregisters when the calling reactive owner
    /// cleans up).
    #[ts("(id: string, context: string, handler: ContextCommandHandler): () => void")]
    pub fn register_handler(
        &self,
        id: String,
        context: String,
        handler: ContextHandlerArg,
    ) -> Result<Stop, String> {
        let Some(rayx) = &self.rayx else {
            return Err(
                "command handler registration requires RayX's command service (`addCommands()`)"
                    .to_owned(),
            );
        };
        let definition = ContextCommandHandlerDefinition {
            title: handler.title.map(make_title_provider).transpose()?,
            category: handler.category,
            kind: handler.kind,
            can_execute: handler.can_execute.map(make_bool_provider).transpose()?,
            checked: handler.checked.map(make_checked_provider).transpose()?,
            run: make_run_action(handler.run, self.runner.clone())?,
        };
        let registration = rayx
            .register_handler(id, context, definition)
            .map_err(|error| error.to_string())?;
        Ok(Stop::new(registration.into_unregister()))
    }

    /// Executes command `id` with optional `args`. Resolves `true` if the command ran, `false` if
    /// `canExecute` was false or out of context, and rejects if unknown, already executing, or the
    /// handler threw.
    #[ts("(id: string, args?: unknown): boolean")]
    pub async fn execute(
        this: Instance<Self>,
        id: String,
        args: Option<JsonValue>,
    ) -> Result<bool, String> {
        command_service(&this)
            .execute(id, args)
            .await
            .map_err(|error| error.to_string())
    }

    /// Whether command `id` is registered and can currently execute.
    pub fn can_execute(&self, id: String) -> bool {
        match &self.rayx {
            Some(rayx) => rayx.can_execute_live(&id),
            None => self.service.can_execute(&id),
        }
    }

    /// Whether command `id` is currently executing.
    pub fn is_executing(&self, id: String) -> bool {
        match &self.rayx {
            Some(rayx) => rayx.is_executing_live(&id),
            None => self.service.is_executing(&id),
        }
    }

    /// The current title of command `id`, or `null` if not registered.
    pub fn title(&self, id: String) -> Option<String> {
        match &self.rayx {
            Some(rayx) => rayx.title_live(&id),
            None => self.service.title(&id),
        }
    }

    /// The current checked state of command `id`, or `null` if not checkable or not registered.
    pub fn checked(&self, id: String) -> Option<bool> {
        match &self.rayx {
            Some(rayx) => rayx.checked_live(&id),
            None => self.service.checked(&id),
        }
    }

    /// All registered commands' current metadata, sorted by `id`.
    #[ts("(): CommandMetadata[]")]
    pub fn all(&self) -> JsonValue {
        match &self.rayx {
            Some(rayx) => json(&rayx.all_live()),
            None => json(&self.service.all()),
        }
    }

    /// Whether a command with `id` is registered.
    pub fn has(&self, id: String) -> bool {
        match &self.rayx {
            Some(rayx) => rayx.has_command(&id),
            None => self.service.has(&id),
        }
    }

    /// Re-evaluates dynamic command metadata (`title`, `canExecute`, `checked`) and notifies
    /// `onChange` listeners if anything changed.
    pub fn refresh(&self) {
        if let Some(rayx) = &self.rayx {
            rayx.refresh();
        }
    }

    /// Resolves the preferred NSH `+`-syntax keyboard shortcut for command `id` from the app's
    /// `InputMapService` (if registered) and active contexts, or `null` if none is bound.
    pub fn shortcut_for(&self, id: String) -> Option<String> {
        if let Some(rayx) = &self.rayx {
            return rayx.shortcut_for(&id);
        }
        let input_map = self.input_map.as_ref()?;
        let contexts = self
            .context
            .as_ref()
            .map(|ctx| ctx.active_contexts())
            .unwrap_or_default();
        keymap::shortcut_for(&id, &contexts, &input_map.input_map().get())
    }

    /// Computes the deduplicated active NSH keymap bindings for the current active contexts and
    /// `InputMapService` (empty array if no `InputMapService` is registered).
    #[ts("(): ActiveKeyBinding[]")]
    pub fn resolve_active_bindings(&self) -> JsonValue {
        if let Some(rayx) = &self.rayx {
            return json(&rayx.resolve_active_bindings());
        }
        let Some(input_map) = self.input_map.as_ref() else {
            return json(&Vec::<rayx_app::services::commands::ActiveKeyBinding>::new());
        };
        let contexts = self
            .context
            .as_ref()
            .map(|ctx| ctx.active_contexts())
            .unwrap_or_default();
        json(&keymap::resolve_active_bindings(
            &contexts,
            &input_map.input_map().get(),
        ))
    }

    /// Calls `listener` with `{ commands, revision }` after every state change, until the returned
    /// function is called (or the calling reactive owner cleans up), or this object is disposed.
    #[ts("(listener: (state: CommandState) => void): () => void")]
    pub fn on_change(
        &mut self,
        #[args(state)] listener: Callback<JsonValue>,
    ) -> Result<Stop, String> {
        let service = Arc::clone(&self.service);
        self.listeners.listen(listener, move |post| {
            Ok::<ChangeSubscription, String>(
                service
                    .commands()
                    .subscribe(move |state| post(json(&**state))),
            )
        })
    }

    /// Calls `listener` with every command event (`{ Registered }`, `{ Unregistered }`,
    /// `{ Executed }`, `{ Failed }`), until the returned function is called (or the calling
    /// reactive owner cleans up), or this object is disposed.
    #[ts("(listener: (event: CommandEvent) => void): () => void")]
    pub fn on_event(
        &mut self,
        #[args(event)] listener: Callback<JsonValue>,
    ) -> Result<Stop, String> {
        let service = Arc::clone(&self.service);
        self.listeners.listen(listener, move |post| {
            Ok::<ChangeSubscription, String>(
                service.events().subscribe(move |event| post(json(event))),
            )
        })
    }
}

impl CommandService {
    /// The underlying Rust service, for bridges taking `Instance<CommandService>`.
    pub fn service(&self) -> &Arc<dyn Commands> {
        &self.service
    }
}
