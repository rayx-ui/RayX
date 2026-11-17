//! Options written in TS: JSON values keyed by an [`OptionsToken`] (a name and defaults), made as
//! Rust's typed options are (`add_options` / `configure_options`): the defaults, then the bound
//! configuration section, then the configure callbacks in order, then the validations.
//!
//! They live beside Rust's typed options, not inside their pipeline: the steps are kept in the
//! root provider as a thread-bound entry under the token's key. A resolve from TS
//! (`services.getOptions(Token)`) runs the callbacks on the spot ([`Callback::call`]);
//! `validateOnStart` runs them in the startup run, before any startup service initializes
//! ([`Callback::call_async`]).
//!
//! Here too: what the options objects of TS builder methods share
//! (`builder.addI18n(options => { options.defaultLanguage("en"); })`). Each records what its
//! methods set, in order, as plain data. The builder method replays the record in the Rust
//! `configure` closure it passes to the Rust extension method of its name, so what TS sets
//! applies after the options bound from configuration, exactly as a Rust closure's changes do. An
//! options object is valid during its callback only: it is disposed after it, and using it later
//! throws.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use futures::FutureExt as _;
use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx_app::di::{
    LocalService, RuntimeKey, StartupInitializationFailure, StartupService, StartupServiceFuture,
    StartupServiceType, ThreadBound,
};
use rayx_app::{Configuration, ServiceProvider as Provider};

/// A key for options written in TS: a name (for messages and errors) and the defaults the
/// options start from, typed `T` for TypeScript. Two tokens are two options, whatever their
/// names.
#[class(generic = "T")]
pub struct OptionsToken {
    name: String,
    defaults: JsonValue,
    key: RuntimeKey,
}

#[class]
impl OptionsToken {
    /// A new token named `name`, whose options start as `defaults` (a JSON-shaped value).
    #[ts("(name: string, defaults: T)")]
    pub fn new(name: String, defaults: JsonValue) -> Self {
        let key = RuntimeKey::new(format!("options `{name}`"));
        Self {
            name,
            defaults,
            key,
        }
    }

    /// The name it was made with.
    #[getter]
    pub fn name(&self) -> String {
        self.name.clone()
    }
}

/// The options a token names, as `addOptions`' callback sets them: each method adds a step
/// and returns the object, so calls chain (`options.bind("Feed").validateOnStart()`).
#[class(generic = "T")]
pub struct OptionsBuilder {
    steps: Vec<Step>,
    on_start: bool,
}

/// What a configure callback that gives nothing fails with.
const NO_VALUE: &str = "configure gave no value: it returns the configured value (the one it got, changed, or a new one)";

/// A step of TS options.
#[derive(Clone)]
enum Step {
    /// Binds the configuration section at this path over the value.
    Bind(String),
    /// Gives the value configured.
    Configure(Callback<JsonValue, Option<JsonValue>>),
    /// Whether the value is valid; the message says what is wrong when it is not.
    Validate(Callback<JsonValue, bool>, String),
}

#[class]
impl OptionsBuilder {
    /// Binds the configuration section at `path` (`"Feed"`, `"Feed:Paging"`) over the
    /// defaults: its values replace theirs key by key.
    #[ts("(path: string): this")]
    pub fn bind(this: Instance<Self>, path: String) -> Instance<Self> {
        this.with_mut(|options| options.steps.push(Step::Bind(path)));
        this
    }

    /// Configures the value: `configure` gets it and returns it configured (it may change the
    /// one it gets and return it). Runs after the binding, in the order added, and before every
    /// validation.
    #[ts("(configure: (value: T) => T): this")]
    pub fn configure(
        this: Instance<Self>,
        #[args(value)] configure: Callback<JsonValue, Option<JsonValue>>,
    ) -> Instance<Self> {
        this.with_mut(|options| options.steps.push(Step::Configure(configure)));
        this
    }

    /// Validates the configured value: it runs after every configure step, however late it was
    /// added (a later `configureOptions` too). When `validate` gives `false`, resolving the
    /// options throws with `message` (every failing validation's message).
    #[ts("(validate: (value: T) => boolean, message: string): this")]
    pub fn validate(
        this: Instance<Self>,
        #[args(value)] validate: Callback<JsonValue, bool>,
        message: String,
    ) -> Instance<Self> {
        this.with_mut(|options| options.steps.push(Step::Validate(validate, message)));
        this
    }

    /// Validates the options when the app starts too, before any startup service initializes:
    /// a failure fails `start()` with its message.
    #[ts("(): this")]
    pub fn validate_on_start(this: Instance<Self>) -> Instance<Self> {
        this.with_mut(|options| options.on_start = true);
        this
    }
}

impl OptionsBuilder {
    fn new() -> Self {
        Self {
            steps: Vec::new(),
            on_start: false,
        }
    }
}

/// A token's options as the provider keeps them, on the engine thread.
pub(crate) struct TsOptions {
    name: String,
    defaults: JsonValue,
    key: RuntimeKey,
    steps: RefCell<Vec<Step>>,
    /// Validated on start: registered in the startup run already.
    on_start: Cell<bool>,
}

/// A token's options as the builder and the provider's factory share them.
pub(crate) type SharedOptions = Arc<ThreadBound<TsOptions>>;

/// What `addOptions` or `configureOptions` adds for `token`: its steps (`configure` called now
/// with an `OptionsBuilder`) and whether it validates on start.
pub(crate) struct Added {
    steps: Vec<Step>,
    on_start: bool,
}

impl Added {
    /// The steps `configure` records.
    pub(crate) fn recorded(
        configure: Option<Callback<Instance<OptionsBuilder>>>,
    ) -> Result<Self, String> {
        let mut on_start = false;
        let steps = crate::configuration::options_ts::record(
            OptionsBuilder::new(),
            configure,
            |options| {
                on_start = options.on_start;
                std::mem::take(&mut options.steps)
            },
        )?;
        Ok(Self { steps, on_start })
    }

    /// One configure step.
    pub(crate) fn configure(configure: Callback<JsonValue, Option<JsonValue>>) -> Self {
        Self {
            steps: vec![Step::Configure(configure)],
            on_start: false,
        }
    }
}

/// The runtime key `token`'s options are kept under.
pub(crate) fn key_of(token: &Instance<OptionsToken>) -> RuntimeKey {
    token.with(|token| token.key.clone())
}

/// Adds `added` to `token`'s options: in `kept` (the builder's, registered in `provider` the
/// first time).
pub(crate) fn add(
    provider: &mut Provider,
    kept: Option<SharedOptions>,
    token: &Instance<OptionsToken>,
    added: Added,
) -> Result<SharedOptions, String> {
    let shared = match kept {
        Some(kept) => kept,
        None => {
            let (name, defaults, key) = token.with(|token| {
                (
                    token.name.clone(),
                    token.defaults.clone(),
                    token.key.clone(),
                )
            });
            let options = TsOptions {
                name: name.clone(),
                defaults,
                key: key.clone(),
                steps: RefCell::new(Vec::new()),
                on_start: Cell::new(false),
            };
            let shared = Arc::new(
                ThreadBound::new(options, format!("options `{name}`"))
                    .map_err(|error| error.to_string())?,
            );
            let factory = Arc::clone(&shared);
            provider.add_singleton_thread_bound_keyed(key, move |_| {
                let options: LocalService = factory.get()?;
                Ok(options)
            });
            shared
        }
    };
    let options = shared.get().map_err(|error| error.to_string())?;
    options.steps.borrow_mut().extend(added.steps);
    if added.on_start && !options.on_start.replace(true) {
        add_on_start(provider, Arc::clone(&shared), &options);
    }
    Ok(shared)
}

/// Validates `options` in the startup run, before any startup service initializes.
fn add_on_start(provider: &mut Provider, shared: SharedOptions, options: &TsOptions) {
    let service = StartupServiceType::runtime(&options.key);
    provider.add_startup_resolved_async(service.clone(), move |context| {
        let shared = Arc::clone(&shared);
        let services = context.services().clone();
        let service = service.clone();
        async move {
            let failed = |message: String| StartupInitializationFailure {
                service: service.clone(),
                message,
                timed_out: false,
            };
            let options = shared.get().map_err(|error| failed(error.to_string()))?;
            let configuration = services.service::<Configuration>();
            let mut value = options.bound(configuration.as_deref()).map_err(failed)?;
            let steps = options.run_order();
            let mut failures = Vec::new();
            for step in steps {
                match step {
                    Step::Bind(_) => {}
                    Step::Configure(configure) => {
                        value = configure
                            .call_async(value)
                            .await
                            .map_err(|error| failed(format!("configure threw: {error}")))?
                            .ok_or_else(|| failed(NO_VALUE.to_owned()))?;
                    }
                    Step::Validate(validate, message) => {
                        let valid = validate
                            .call_async(value.clone())
                            .await
                            .map_err(|error| failed(format!("validate threw: {error}")))?;
                        if !valid {
                            failures.push(message);
                        }
                    }
                }
            }
            if !failures.is_empty() {
                return Err(failed(format!(
                    "failed validation: {}",
                    failures.join("; ")
                )));
            }
            Ok(Arc::new(crate::di::container_ts::Made) as StartupService)
        }
        .boxed_local() as StartupServiceFuture
    });
}

impl TsOptions {
    /// The defaults with every bound section over them, in order.
    fn bound(&self, configuration: Option<&Configuration>) -> Result<JsonValue, String> {
        let mut value = self.defaults.clone();
        for step in self.steps.borrow().iter() {
            if let Step::Bind(path) = step {
                let configuration = configuration.ok_or_else(|| {
                    format!(
                        "{} binds `{path}`, but the app has no configuration",
                        self.named()
                    )
                })?;
                configuration
                    .bind(path, &mut value)
                    .map_err(|error| format!("{} did not bind: {error}", self.named()))?;
            }
        }
        Ok(value)
    }

    /// The configure and validate steps in the order they run, as Rust's options pipeline runs
    /// them: every configure step in the order added, then every validation, so each validation
    /// sees the configured value. The binds come before both ([`Self::bound`]).
    fn run_order(&self) -> Vec<Step> {
        let (validations, configures): (Vec<Step>, Vec<Step>) = self
            .steps
            .borrow()
            .iter()
            .filter(|step| !matches!(step, Step::Bind(_)))
            .cloned()
            .partition(|step| matches!(step, Step::Validate(..)));
        configures.into_iter().chain(validations).collect()
    }

    /// The options made now, on the spot: inside a call from JS.
    fn made(&self, configuration: Option<&Configuration>) -> Result<JsonValue, String> {
        let mut value = self.bound(configuration)?;
        let steps = self.run_order();
        let mut failures = Vec::new();
        for step in steps {
            match step {
                Step::Bind(_) => {}
                Step::Configure(configure) => {
                    value = configure
                        .call(value)
                        .map_err(|error| format!("{}: configure threw: {error}", self.named()))?
                        .ok_or_else(|| format!("{}: {NO_VALUE}", self.named()))?;
                }
                Step::Validate(validate, message) => {
                    let valid = validate
                        .call(value.clone())
                        .map_err(|error| format!("{}: validate threw: {error}", self.named()))?;
                    if !valid {
                        failures.push(message);
                    }
                }
            }
        }
        if failures.is_empty() {
            Ok(value)
        } else {
            Err(format!(
                "{} failed validation: {}",
                self.named(),
                failures.join("; ")
            ))
        }
    }

    fn named(&self) -> String {
        format!("options `{}`", self.name)
    }
}

/// `token`'s options in `services`, made now (inside a call from JS): `{ value }`.
pub(crate) fn resolve(
    services: &Provider,
    token: &Instance<OptionsToken>,
) -> Result<JsonValue, String> {
    let key = key_of(token);
    let configuration = services.service::<Configuration>();
    let value = match services.thread_bound_service_by_key(&key) {
        Some(options) => {
            let options: Rc<TsOptions> = options
                .downcast()
                .map_err(|_| format!("{key} holds no TS options"))?;
            options.made(configuration.as_deref())?
        }
        // Nothing registered: the defaults.
        None => token.with(|token| token.defaults.clone()),
    };
    Ok(serde_json::json!({ "value": value }))
}

/// Calls `configure` (when there is one) with a new recording options object and gives what it
/// recorded; the object is disposed after the call. A callback that throws fails with its
/// message.
pub fn record<C, S>(
    options: C,
    configure: Option<Callback<Instance<C>>>,
    steps: impl FnOnce(&mut C) -> Vec<S>,
) -> Result<Vec<S>, String>
where
    C: nsh_core::element::Class,
{
    let Some(configure) = configure else {
        return Ok(Vec::new());
    };
    let options = Instance::new(options);
    let called = configure.call(options.clone());
    let recorded = if options.is_disposed() {
        Vec::new()
    } else {
        let recorded = options.with_mut(steps);
        options.dispose();
        recorded
    };
    called.map(|()| recorded)
}

/// Records `step` on `this` and gives it back, for chaining.
pub fn push<C: nsh_core::element::Class>(
    this: Instance<C>,
    step: impl FnOnce(&mut C),
) -> Instance<C> {
    this.with_mut(step);
    this
}
