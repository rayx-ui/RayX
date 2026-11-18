//! `add_startup_singleton_for::<dyn Trait, Impl>` and its factory, scoped and transient forms:
//! an implementation of a trait that is no `StartupInitializable` (a contract) registered as the
//! trait and as a startup service registered as the trait.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use futures::FutureExt;
use futures::future::LocalBoxFuture;
use rayx_app::prelude::*;
use rayx_app::services::platform::{PlatformInfoService, PlatformSnapshot};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

type Log = Arc<Mutex<Vec<&'static str>>>;

/// A plain trait of the app's: `implemented_by!` lets it be registered with the `*_for` forms.
trait Greeter: Send + Sync {
    fn greet(&self) -> String;
}

rayx_app::implemented_by!(Greeter);

/// An implementation that loads at startup and logs it.
struct Loaded {
    label: &'static str,
    log: Log,
    loads: AtomicUsize,
    key: Option<StartupServiceType>,
    platform: StateCell<PlatformSnapshot>,
}

impl Loaded {
    fn new(label: &'static str, log: &Log) -> Self {
        Self {
            label,
            log: Arc::clone(log),
            loads: AtomicUsize::new(0),
            key: None,
            platform: StateCell::new(PlatformSnapshot::default()),
        }
    }

    fn keyed(mut self, key: StartupServiceType) -> Self {
        self.key = Some(key);
        self
    }

    fn loads(&self) -> usize {
        self.loads.load(Ordering::SeqCst)
    }
}

impl Greeter for Loaded {
    fn greet(&self) -> String {
        format!("hello from {}", self.label)
    }
}

impl PlatformInfoService for Loaded {
    fn platform(&self) -> &StateCell<PlatformSnapshot> {
        &self.platform
    }
}

impl AsyncInitializable for Loaded {
    fn is_initialized(&self) -> bool {
        self.loads() > 0
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        self.log.lock().unwrap().push(self.label);
        futures::future::ready(Ok(())).boxed_local()
    }
}

impl StartupInitializable for Loaded {
    fn startup_service_type(&self) -> Option<StartupServiceType> {
        self.key.clone()
    }
}

/// A startup service that waits for `on` and logs.
struct Waiting {
    on: StartupServiceType,
    log: Log,
    initialized: StartupInitializationState,
}

impl AsyncInitializable for Waiting {
    fn is_initialized(&self) -> bool {
        self.initialized.is_initialized()
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        self.log.lock().unwrap().push("waiting");
        self.initialized.mark_initialized();
        futures::future::ready(Ok(())).boxed_local()
    }
}

impl StartupInitializable for Waiting {
    fn initialization_depends_on(&self) -> Vec<StartupServiceType> {
        vec![self.on.clone()]
    }
}

fn waiting_for<T: ?Sized + 'static>(log: &Log) -> Waiting {
    Waiting {
        on: StartupServiceType::of::<T>(),
        log: Arc::clone(log),
        initialized: StartupInitializationState::default(),
    }
}

fn run(provider: &ServiceProvider) -> Result<(), StartupInitializationError> {
    futures::executor::block_on(
        provider
            .startup_registry()
            .initialize(StartupInitializationContext::new(provider.clone())),
    )
}

#[test]
fn di_startup_for_a_contract_resolves_as_it_and_initializes_first() {
    let log = Log::default();
    let mut provider = ServiceProvider::new();
    // The dependent first: the order of registration does not matter.
    provider.add_startup_singleton(waiting_for::<dyn PlatformInfoService>(&log));
    let platform = provider.add_startup_singleton_for::<dyn PlatformInfoService, _>(Arc::new(
        Loaded::new("platform", &log),
    ));

    assert!(provider.has_startup_role::<dyn PlatformInfoService>());
    run(&provider).expect("startup runs");

    assert_eq!(*log.lock().unwrap(), ["platform", "waiting"]);
    assert_eq!(platform.loads(), 1);
    let resolved = provider
        .required_service::<dyn PlatformInfoService>()
        .unwrap();
    assert!(
        std::ptr::addr_eq(Arc::as_ptr(&resolved), Arc::as_ptr(&platform)),
        "the trait resolves to the registered implementation"
    );
}

#[test]
fn di_startup_for_a_plain_trait_with_implemented_by() {
    let log = Log::default();
    let provider = ServiceProvider::new()
        .with_startup_singleton_for::<dyn Greeter, _>(Arc::new(Loaded::new("greeter", &log)));

    run(&provider).expect("startup runs");

    assert_eq!(*log.lock().unwrap(), ["greeter"]);
    assert_eq!(
        provider.required_service::<dyn Greeter>().unwrap().greet(),
        "hello from greeter"
    );
}

#[test]
fn di_startup_for_keeps_a_key_the_implementation_names() {
    struct Elsewhere;
    let log = Log::default();
    let mut provider = ServiceProvider::new();
    provider.add_startup_singleton_for::<dyn Greeter, _>(Arc::new(
        Loaded::new("greeter", &log).keyed(StartupServiceType::of::<Elsewhere>()),
    ));
    provider.add_startup_singleton(waiting_for::<Elsewhere>(&log));

    run(&provider).expect("the dependent waits for the key the implementation names");

    assert_eq!(*log.lock().unwrap(), ["greeter", "waiting"]);
}

#[test]
fn di_startup_for_the_factory_form_makes_one_implementation() {
    let log = Log::default();
    let made = Arc::new(AtomicUsize::new(0));
    let mut provider = ServiceProvider::new();
    let (factory_log, counter) = (Arc::clone(&log), Arc::clone(&made));
    provider.add_startup_singleton_factory_for::<dyn Greeter, _>(move |_| {
        counter.fetch_add(1, Ordering::SeqCst);
        Arc::new(Loaded::new("made", &factory_log))
    });
    provider.add_startup_singleton(waiting_for::<dyn Greeter>(&log));

    run(&provider).expect("startup runs");

    assert_eq!(*log.lock().unwrap(), ["made", "waiting"]);
    assert_eq!(
        provider.required_service::<dyn Greeter>().unwrap().greet(),
        "hello from made"
    );
    assert_eq!(
        made.load(Ordering::SeqCst),
        1,
        "startup and resolving share the singleton"
    );
}

#[test]
fn di_startup_for_the_scoped_and_transient_forms() {
    let log = Log::default();
    let mut provider = ServiceProvider::new();
    let scoped_log = Arc::clone(&log);
    provider.add_startup_scoped_for::<dyn Greeter, _>(move |_| {
        Arc::new(Loaded::new("scoped", &scoped_log))
    });
    let transient_log = Arc::clone(&log);
    provider.add_startup_transient_for::<dyn PlatformInfoService, _>(move |_| {
        Arc::new(Loaded::new("transient", &transient_log))
    });

    run(&provider).expect("startup runs");

    let mut loaded = log.lock().unwrap().clone();
    loaded.sort_unstable();
    assert_eq!(loaded, ["scoped", "transient"]);
    let root = provider.required_service::<dyn Greeter>().unwrap();
    let again = provider.required_service::<dyn Greeter>().unwrap();
    assert!(Arc::ptr_eq(&root, &again), "one implementation per scope");
    let scope = provider.create_scope();
    assert!(!Arc::ptr_eq(
        &root,
        &scope.required_service::<dyn Greeter>().unwrap()
    ));
}

#[test]
fn di_startup_for_a_later_registration_of_the_trait_removes_it() {
    let log = Log::default();
    let mut provider = ServiceProvider::new();
    let first =
        provider.add_startup_singleton_for::<dyn Greeter, _>(Arc::new(Loaded::new("first", &log)));
    let second =
        provider.add_startup_singleton_for::<dyn Greeter, _>(Arc::new(Loaded::new("second", &log)));

    run(&provider).expect("startup runs");

    assert_eq!((first.loads(), second.loads()), (0, 1));
    assert_eq!(*log.lock().unwrap(), ["second"]);
}

#[test]
fn di_startup_for_the_startup_only_form_is_registered_as_the_trait() {
    let log = Log::default();
    let mut provider = ServiceProvider::new();
    provider.add_singleton_as::<dyn Greeter>(
        Arc::new(Loaded::new("resolved", &log)) as Arc<dyn Greeter>
    );
    let follower = Arc::new(Loaded::new("follower", &log));
    provider.add_startup_service_for::<dyn Greeter, _>(Arc::clone(&follower));
    assert!(provider.has_startup_role::<dyn Greeter>());

    // A later registration of the trait removes it.
    provider
        .add_singleton_as::<dyn Greeter>(Arc::new(Loaded::new("own", &log)) as Arc<dyn Greeter>);
    run(&provider).expect("startup runs");
    assert_eq!(follower.loads(), 0);
}
