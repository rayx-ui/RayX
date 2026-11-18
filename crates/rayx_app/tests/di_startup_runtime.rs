//! Runtime-keyed startup services made by asynchronous factories, run by the same startup run as
//! typed ones: ordering both ways, concurrent layers, timeouts and failure reports.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use futures::FutureExt;
use futures::future::{LocalBoxFuture, pending, poll_fn, ready};
use rayx_app::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::Poll;
use std::time::Duration;

type Log = Arc<Mutex<Vec<String>>>;

/// Completes after being polled once more, so services of a layer interleave.
fn yield_once() -> impl std::future::Future<Output = ()> {
    let mut yielded = false;
    poll_fn(move |context| {
        if yielded {
            Poll::Ready(())
        } else {
            yielded = true;
            context.waker().wake_by_ref();
            Poll::Pending
        }
    })
}

#[derive(Clone, Copy, Default)]
enum Outcome {
    #[default]
    Succeed,
    Fail,
    Hang,
}

/// A configurable startup service that logs its initialization.
#[derive(Clone, Default)]
struct Probe {
    name: &'static str,
    log: Log,
    depends_on: Vec<StartupServiceType>,
    outcome: Outcome,
    timeout: Option<Duration>,
    /// Shared by the services of a layer: how many are initializing, and the most at once.
    running: Option<(Arc<AtomicUsize>, Arc<AtomicUsize>)>,
    initialized: StartupInitializationState,
}

impl Probe {
    fn new(name: &'static str, log: &Log) -> Self {
        Self {
            name,
            log: Arc::clone(log),
            ..Self::default()
        }
    }

    fn depends_on(mut self, dependency: StartupServiceType) -> Self {
        self.depends_on.push(dependency);
        self
    }

    fn outcome(mut self, outcome: Outcome) -> Self {
        self.outcome = outcome;
        self
    }

    fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
}

impl AsyncInitializable for Probe {
    fn is_initialized(&self) -> bool {
        self.initialized.is_initialized()
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let probe = self.clone();
        async move {
            if let Some((running, most)) = &probe.running {
                let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                most.fetch_max(now, Ordering::SeqCst);
                yield_once().await;
                running.fetch_sub(1, Ordering::SeqCst);
            }
            match probe.outcome {
                Outcome::Succeed => {
                    probe.log.lock().unwrap().push(probe.name.to_owned());
                    probe.initialized.mark_initialized();
                    Ok(())
                }
                Outcome::Fail => Err(StartupServiceError::new("refused")),
                Outcome::Hang => pending().await,
            }
        }
        .boxed_local()
    }

    fn initialization_timeout(&self) -> Option<Duration> {
        self.timeout
    }
}

impl StartupInitializable for Probe {
    fn initialization_depends_on(&self) -> Vec<StartupServiceType> {
        self.depends_on.clone()
    }
}

/// A typed startup service wrapping a probe.
macro_rules! typed {
    ($name:ident) => {
        #[derive(Clone, AppService)]
        struct $name(Probe);

        impl AsyncInitializable for $name {
            fn is_initialized(&self) -> bool {
                self.0.is_initialized()
            }

            fn initialize(
                &self,
                context: StartupInitializationContext,
            ) -> LocalBoxFuture<'static, StartupInitializationResult> {
                self.0.initialize(context)
            }
        }

        impl StartupInitializable for $name {
            fn initialization_depends_on(&self) -> Vec<StartupServiceType> {
                self.0.initialization_depends_on()
            }
        }
    };
}

typed!(TypedFirst);
typed!(TypedAfter);

/// Registers `probe` under the runtime key `key`, made by an asynchronous factory.
fn add_runtime(provider: &mut ServiceProvider, key: &RuntimeKey, probe: Probe) {
    provider.add_startup_resolved_async(key, move |_context| {
        let probe = probe.clone();
        async move {
            yield_once().await;
            let service: StartupService = Arc::new(probe);
            Ok(service)
        }
        .boxed_local()
    });
}

fn run(provider: &ServiceProvider) -> Result<(), StartupInitializationError> {
    futures::executor::block_on(
        provider
            .startup_registry()
            .initialize(StartupInitializationContext::new(provider.clone())),
    )
}

fn failures(error: &StartupInitializationError) -> &[StartupInitializationFailure] {
    match error {
        StartupInitializationError::ServiceFailures { failures } => failures.as_slice(),
        other => panic!("expected service failures, got {other}"),
    }
}

fn position(log: &Log, name: &str) -> usize {
    log.lock()
        .unwrap()
        .iter()
        .position(|entry| entry == name)
        .unwrap_or_else(|| panic!("`{name}` was not initialized"))
}

#[test]
fn di_startup_runtime_async_made_service_initializes_in_the_run() {
    let log = Log::default();
    let key = RuntimeKey::new("ts:Settings");
    let mut provider = ServiceProvider::new();
    provider.add_startup_singleton(TypedFirst(Probe::new("typed", &log)));
    add_runtime(&mut provider, &key, Probe::new("ts:Settings", &log));

    run(&provider).expect("both startup services initialize");

    let mut entries = log.lock().unwrap().clone();
    entries.sort();
    assert_eq!(entries, ["ts:Settings", "typed"]);
    assert!(
        provider
            .required_service::<TypedFirst>()
            .unwrap()
            .is_initialized()
    );
}

#[test]
fn di_startup_runtime_service_may_wait_for_a_typed_one() {
    let log = Log::default();
    let key = RuntimeKey::new("ts:Feed");
    let mut provider = ServiceProvider::new();
    add_runtime(
        &mut provider,
        &key,
        Probe::new("ts:Feed", &log).depends_on(StartupServiceType::of::<TypedFirst>()),
    );
    provider.add_startup_singleton(TypedFirst(Probe::new("typed", &log)));

    run(&provider).expect("ordered by the dependency");

    assert!(position(&log, "typed") < position(&log, "ts:Feed"));
}

#[test]
fn di_startup_runtime_typed_service_may_wait_for_a_runtime_one() {
    let log = Log::default();
    let key = RuntimeKey::new("ts:Store");
    let mut provider = ServiceProvider::new();
    provider.add_startup_singleton(TypedAfter(
        Probe::new("typed", &log).depends_on(StartupServiceType::runtime(&key)),
    ));
    add_runtime(&mut provider, &key, Probe::new("ts:Store", &log));

    run(&provider).expect("ordered by the dependency");

    assert!(position(&log, "ts:Store") < position(&log, "typed"));
}

#[test]
fn di_startup_runtime_chain_crosses_both_kinds() {
    let log = Log::default();
    let first = RuntimeKey::new("ts:First");
    let last = RuntimeKey::new("ts:Last");
    let mut provider = ServiceProvider::new();
    add_runtime(
        &mut provider,
        &last,
        Probe::new("ts:Last", &log).depends_on(StartupServiceType::of::<TypedAfter>()),
    );
    provider.add_startup_singleton(TypedAfter(
        Probe::new("typed", &log).depends_on(StartupServiceType::runtime(&first)),
    ));
    add_runtime(&mut provider, &first, Probe::new("ts:First", &log));

    run(&provider).expect("a chain over both key kinds");

    assert_eq!(*log.lock().unwrap(), ["ts:First", "typed", "ts:Last"]);
}

#[test]
fn di_startup_runtime_layer_initializes_concurrently() {
    let log = Log::default();
    let running = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let mut provider = ServiceProvider::new();
    for name in ["ts:One", "ts:Two"] {
        let probe = Probe {
            running: Some(running.clone()),
            ..Probe::new(name, &log)
        };
        add_runtime(&mut provider, &RuntimeKey::new(name), probe);
    }
    provider.add_startup_singleton(TypedFirst(Probe {
        running: Some(running.clone()),
        ..Probe::new("typed", &log)
    }));

    run(&provider).expect("one layer");

    assert_eq!(
        running.1.load(Ordering::SeqCst),
        3,
        "the whole layer ran at once"
    );
    assert_eq!(log.lock().unwrap().len(), 3);
}

#[test]
fn di_startup_runtime_timeouts_are_its_own_or_the_default() {
    let log = Log::default();
    let own = RuntimeKey::new("ts:Own");
    let defaulted = RuntimeKey::new("ts:Defaulted");
    let mut provider = ServiceProvider::new();
    provider.set_default_initialization_timeout(Duration::from_secs(9));
    add_runtime(
        &mut provider,
        &own,
        Probe::new("ts:Own", &log).timeout(Duration::from_millis(40)),
    );
    add_runtime(&mut provider, &defaulted, Probe::new("ts:Defaulted", &log));
    let asked = Arc::new(Mutex::new(Vec::new()));

    futures::executor::block_on(provider.startup_registry().initialize_with_timer(
        StartupInitializationContext::new(provider.clone()),
        {
            let asked = Arc::clone(&asked);
            move |duration| {
                asked.lock().unwrap().push(duration);
                pending::<()>().boxed_local()
            }
        },
    ))
    .expect("both finish in time");

    let mut asked = asked.lock().unwrap().clone();
    asked.sort();
    // Each factory is bounded by the default, then each initialization by its own timeout or
    // the default.
    assert_eq!(
        asked,
        [
            Duration::from_millis(40),
            Duration::from_secs(9),
            Duration::from_secs(9),
            Duration::from_secs(9)
        ]
    );
}

#[test]
fn di_startup_runtime_unsettled_factory_times_out_naming_the_service() {
    let log = Log::default();
    let key = RuntimeKey::new("ts:NeverMade");
    let mut provider = ServiceProvider::new();
    provider.set_default_initialization_timeout(Duration::from_millis(70));
    provider.add_startup_singleton(TypedFirst(Probe::new("typed", &log)));
    provider.add_startup_resolved_async(&key, |_context| pending().boxed_local());

    let error = futures::executor::block_on(provider.startup_registry().initialize_with_timer(
        StartupInitializationContext::new(provider.clone()),
        |duration| {
            // Only the default (the factory's bound) expires.
            if duration == Duration::from_millis(70) {
                ready(()).boxed_local()
            } else {
                pending().boxed_local()
            }
        },
    ))
    .expect_err("a factory that never settles fails startup");

    let failures = failures(&error);
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].service, StartupServiceType::runtime(&key));
    assert!(failures[0].timed_out);
    assert_eq!(failures[0].message, "was not made within 70 ms");
    assert!(error.to_string().contains("ts:NeverMade"), "{error}");
    assert!(log.lock().unwrap().is_empty(), "no service initialized");
}

#[test]
fn di_startup_runtime_timeout_is_reported_with_the_display_name() {
    let log = Log::default();
    let key = RuntimeKey::new("ts:Slow");
    let mut provider = ServiceProvider::new();
    add_runtime(
        &mut provider,
        &key,
        Probe::new("ts:Slow", &log)
            .outcome(Outcome::Hang)
            .timeout(Duration::from_millis(15)),
    );
    provider.add_startup_singleton(TypedFirst(Probe::new("typed", &log)));

    let error = futures::executor::block_on(provider.startup_registry().initialize_with_timer(
        StartupInitializationContext::new(provider.clone()),
        |duration| {
            if duration == Duration::from_millis(15) {
                ready(()).boxed_local()
            } else {
                pending().boxed_local()
            }
        },
    ))
    .expect_err("past its timeout");

    let failures = failures(&error);
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].service, StartupServiceType::runtime(&key));
    assert_eq!(failures[0].service.name(), "ts:Slow");
    assert!(failures[0].timed_out);
    assert!(
        error
            .to_string()
            .contains("ts:Slow: did not initialize within 15 ms"),
        "{error}"
    );
}

#[test]
fn di_startup_runtime_failing_initialize_is_reported_with_the_display_name() {
    let log = Log::default();
    let key = RuntimeKey::new("ts:Broken");
    let mut provider = ServiceProvider::new();
    add_runtime(
        &mut provider,
        &key,
        Probe::new("ts:Broken", &log).outcome(Outcome::Fail),
    );
    provider.add_startup_singleton(TypedFirst(Probe::new("typed", &log)));
    provider.add_startup_singleton(TypedAfter(
        Probe::new("after", &log).depends_on(StartupServiceType::runtime(&key)),
    ));

    let error = run(&provider).expect_err("a failing service fails startup");

    let failures = failures(&error);
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].service.name(), "ts:Broken");
    assert!(!failures[0].timed_out);
    assert!(error.to_string().contains("ts:Broken: refused"), "{error}");
    assert!(
        !provider
            .required_service::<TypedAfter>()
            .unwrap()
            .is_initialized(),
        "the run stops at the failing layer"
    );
}

#[test]
fn di_startup_runtime_failing_factory_fails_startup_naming_the_service() {
    let log = Log::default();
    let key = RuntimeKey::new("ts:Unmade");
    let mut provider = ServiceProvider::new();
    provider.add_startup_singleton(TypedFirst(Probe::new("typed", &log)));
    provider.add_startup_resolved_async(&key, |_context| {
        async move {
            yield_once().await;
            Err(StartupInitializationFailure {
                service: StartupServiceType::of::<()>(),
                message: "the constructor threw".to_owned(),
                timed_out: false,
            })
        }
        .boxed_local()
    });

    let error = run(&provider).expect_err("a failing factory fails startup");

    let failures = failures(&error);
    assert_eq!(failures.len(), 1);
    assert_eq!(
        failures[0].service,
        StartupServiceType::runtime(&key),
        "reported under its registration, whatever the factory named"
    );
    assert!(
        error
            .to_string()
            .contains("ts:Unmade: the constructor threw"),
        "{error}"
    );
    assert!(log.lock().unwrap().is_empty(), "no service initialized");
}

#[test]
fn di_startup_runtime_missing_dependency_names_the_runtime_key() {
    let log = Log::default();
    let missing = RuntimeKey::new("ts:Missing");
    let mut provider = ServiceProvider::new();
    add_runtime(
        &mut provider,
        &RuntimeKey::new("ts:Needy"),
        Probe::new("ts:Needy", &log).depends_on(StartupServiceType::runtime(&missing)),
    );

    let error = run(&provider).expect_err("an unknown dependency fails startup");

    assert!(matches!(
        &error,
        StartupInitializationError::MissingDependency { service, dependency, .. }
            if service.name() == "ts:Needy" && *dependency == StartupServiceType::runtime(&missing)
    ));
    assert!(error.to_string().contains("ts:Missing"), "{error}");
}

#[test]
fn di_startup_runtime_run_succeeds_only_when_every_service_initialized() {
    let log = Log::default();
    let mut provider = ServiceProvider::new();
    add_runtime(
        &mut provider,
        &RuntimeKey::new("ts:Good"),
        Probe::new("ts:Good", &log),
    );
    add_runtime(
        &mut provider,
        &RuntimeKey::new("ts:Bad"),
        Probe::new("ts:Bad", &log).outcome(Outcome::Fail),
    );
    provider.add_startup_singleton(TypedFirst(Probe::new("typed", &log)));

    let error = run(&provider).expect_err("one failure fails the run");

    assert_eq!(failures(&error).len(), 1);
    let mut entries = log.lock().unwrap().clone();
    entries.sort();
    assert_eq!(
        entries,
        ["ts:Good", "typed"],
        "the rest of the layer still ran"
    );
}

#[test]
fn di_startup_runtime_keys_of_the_same_name_are_distinct_services() {
    let log = Log::default();
    let first = RuntimeKey::new("ts:Twin");
    let second = RuntimeKey::new("ts:Twin");
    let mut provider = ServiceProvider::new();
    add_runtime(&mut provider, &first, Probe::new("ts:Twin#1", &log));
    add_runtime(
        &mut provider,
        &second,
        Probe::new("ts:Twin#2", &log).depends_on(StartupServiceType::runtime(&first)),
    );

    run(&provider).expect("two services");

    assert_eq!(*log.lock().unwrap(), ["ts:Twin#1", "ts:Twin#2"]);
}
