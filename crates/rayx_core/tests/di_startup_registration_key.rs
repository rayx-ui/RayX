//! `StartupInitializable::startup_service_type` is optional: a startup service that leaves it
//! out is keyed by what it was registered as, and one that implements it keeps its key.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use futures::FutureExt;
use futures::future::LocalBoxFuture;
use rayx_core::prelude::*;
use std::sync::{Arc, Mutex};

type Log = Arc<Mutex<Vec<&'static str>>>;

/// A startup service that logs its initialization and may wait for others. With `key` set it
/// names its own startup key; without, it leaves `startup_service_type` out.
macro_rules! service {
    ($name:ident) => {
        #[derive(Clone, AppService)]
        struct $name {
            label: &'static str,
            log: Log,
            depends_on: Vec<StartupServiceType>,
            key: Option<StartupServiceType>,
            initialized: StartupInitializationState,
        }

        #[allow(dead_code)]
        impl $name {
            fn new(label: &'static str, log: &Log) -> Self {
                Self {
                    label,
                    log: Arc::clone(log),
                    depends_on: Vec::new(),
                    key: None,
                    initialized: StartupInitializationState::default(),
                }
            }

            fn depends_on(mut self, dependency: StartupServiceType) -> Self {
                self.depends_on.push(dependency);
                self
            }

            fn keyed(mut self, key: StartupServiceType) -> Self {
                self.key = Some(key);
                self
            }
        }

        impl AsyncInitializable for $name {
            fn is_initialized(&self) -> bool {
                self.initialized.is_initialized()
            }

            fn initialize(
                &self,
                _context: StartupInitializationContext,
            ) -> LocalBoxFuture<'static, StartupInitializationResult> {
                let (label, log, initialized) =
                    (self.label, Arc::clone(&self.log), self.initialized.clone());
                async move {
                    log.lock().unwrap().push(label);
                    initialized.mark_initialized();
                    Ok(())
                }
                .boxed_local()
            }
        }

        impl StartupInitializable for $name {
            fn startup_service_type(&self) -> Option<StartupServiceType> {
                self.key.clone()
            }

            fn initialization_depends_on(&self) -> Vec<StartupServiceType> {
                self.depends_on.clone()
            }
        }

        impl Store for $name {}
    };
}

/// A trait the services are also registered as.
trait Store: StartupInitializable {}

impl AppService for dyn Store {}

/// Implements nothing beyond the required members: no `startup_service_type` at all.
#[derive(Clone, Default, AppService)]
struct Minimal {
    initialized: StartupInitializationState,
}

impl AsyncInitializable for Minimal {
    fn is_initialized(&self) -> bool {
        self.initialized.is_initialized()
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let initialized = self.initialized.clone();
        async move {
            initialized.mark_initialized();
            Ok(())
        }
        .boxed_local()
    }
}

impl StartupInitializable for Minimal {}

service!(Dependency);
service!(Dependent);

fn run(provider: &ServiceProvider) -> Result<(), StartupInitializationError> {
    futures::executor::block_on(
        provider
            .startup_registry()
            .initialize(StartupInitializationContext::new(provider.clone())),
    )
}

fn missing_dependency(error: &StartupInitializationError) -> &StartupServiceType {
    match error {
        StartupInitializationError::MissingDependency { dependency, .. } => dependency,
        other => panic!("expected a missing dependency, got {other}"),
    }
}

#[test]
fn di_startup_registration_key_default_is_none() {
    assert_eq!(Minimal::default().startup_service_type(), None);
    assert_eq!(Arc::new(Minimal::default()).startup_service_type(), None);
}

#[test]
fn di_startup_registration_key_minimal_service_initializes() {
    let provider = ServiceProvider::new().with_startup_singleton(Minimal::default());

    run(&provider).expect("a service without `startup_service_type` runs");

    assert!(
        provider
            .required_service::<Minimal>()
            .unwrap()
            .is_initialized()
    );
    assert!(provider.has_startup_role::<Minimal>());
}

#[test]
fn di_startup_registration_key_singleton_is_keyed_by_its_type() {
    let log = Log::default();
    let provider = ServiceProvider::new()
        .with_startup_singleton(
            Dependent::new("dependent", &log).depends_on(StartupServiceType::of::<Dependency>()),
        )
        .with_startup_singleton(Dependency::new("dependency", &log));

    run(&provider).expect("the dependency is keyed `Dependency`");

    assert_eq!(*log.lock().unwrap(), ["dependency", "dependent"]);
}

#[test]
fn di_startup_registration_key_singleton_as_is_keyed_by_the_trait_object() {
    let log = Log::default();
    let mut provider = ServiceProvider::new();
    provider.add_startup_singleton_as::<dyn Store>(
        Arc::new(Dependency::new("store", &log)) as Arc<dyn Store>
    );
    provider.add_startup_singleton(
        Dependent::new("dependent", &log).depends_on(StartupServiceType::of::<dyn Store>()),
    );

    run(&provider).expect("the dependency is keyed `dyn Store`");
    assert_eq!(*log.lock().unwrap(), ["store", "dependent"]);

    let mut by_concrete = ServiceProvider::new();
    by_concrete.add_startup_singleton_as::<dyn Store>(
        Arc::new(Dependency::new("store", &log)) as Arc<dyn Store>
    );
    by_concrete.add_startup_singleton(
        Dependent::new("dependent", &log).depends_on(StartupServiceType::of::<Dependency>()),
    );
    let error = run(&by_concrete).expect_err("not keyed by its concrete type");
    assert_eq!(
        *missing_dependency(&error),
        StartupServiceType::of::<Dependency>()
    );
}

#[test]
fn di_startup_registration_key_scoped_and_transient_are_keyed_by_registration() {
    for lifetime in ["scoped", "transient", "scoped_as", "transient_as"] {
        let log = Log::default();
        let mut provider = ServiceProvider::new();
        let dependency = match lifetime {
            "scoped" => {
                let log = Arc::clone(&log);
                provider.add_startup_scoped(move |_| Dependency::new("dependency", &log));
                StartupServiceType::of::<Dependency>()
            }
            "transient" => {
                let log = Arc::clone(&log);
                provider.add_startup_transient(move |_| Dependency::new("dependency", &log));
                StartupServiceType::of::<Dependency>()
            }
            "scoped_as" => {
                let log = Arc::clone(&log);
                provider.add_startup_scoped_as::<dyn Store>(move |_| {
                    Arc::new(Dependency::new("dependency", &log)) as Arc<dyn Store>
                });
                StartupServiceType::of::<dyn Store>()
            }
            _ => {
                let log = Arc::clone(&log);
                provider.add_startup_transient_as::<dyn Store>(move |_| {
                    Arc::new(Dependency::new("dependency", &log)) as Arc<dyn Store>
                });
                StartupServiceType::of::<dyn Store>()
            }
        };
        provider.add_startup_singleton(Dependent::new("dependent", &log).depends_on(dependency));

        run(&provider).unwrap_or_else(|error| panic!("{lifetime}: {error}"));
        assert_eq!(
            *log.lock().unwrap(),
            ["dependency", "dependent"],
            "{lifetime}"
        );
    }
}

#[test]
fn di_startup_registration_key_builder_methods_record_the_registration() {
    let log = Log::default();
    let builder = RayXAppBuilder::new()
        .add_startup_singleton(
            Dependent::new("dependent", &log).depends_on(StartupServiceType::of::<dyn Store>()),
        )
        .add_startup_singleton_as::<dyn Store>(
            Arc::new(Dependency::new("store", &log)) as Arc<dyn Store>
        );

    run(&builder.services).expect("the builder keys `dyn Store`");

    assert_eq!(*log.lock().unwrap(), ["store", "dependent"]);
}

#[test]
fn di_startup_registration_key_explicit_type_is_kept() {
    let log = Log::default();
    let mut provider = ServiceProvider::new();
    provider.add_startup_singleton(
        Dependency::new("dependency", &log).keyed(StartupServiceType::of::<dyn Store>()),
    );
    provider.add_startup_singleton(
        Dependent::new("dependent", &log).depends_on(StartupServiceType::of::<dyn Store>()),
    );

    run(&provider).expect("keyed by its own `startup_service_type`");
    assert_eq!(*log.lock().unwrap(), ["dependency", "dependent"]);

    let mut by_registration = ServiceProvider::new();
    by_registration.add_startup_singleton(
        Dependency::new("dependency", &log).keyed(StartupServiceType::of::<dyn Store>()),
    );
    by_registration.add_startup_singleton(
        Dependent::new("dependent", &log).depends_on(StartupServiceType::of::<Dependency>()),
    );
    let error = run(&by_registration).expect_err("its own key replaces the registration's");
    assert_eq!(
        *missing_dependency(&error),
        StartupServiceType::of::<Dependency>()
    );
}

#[test]
fn di_startup_registration_key_explicit_type_is_kept_for_resolved_services() {
    let log = Log::default();
    let mut provider = ServiceProvider::new();
    {
        let log = Arc::clone(&log);
        provider.add_startup_scoped(move |_| {
            Dependency::new("dependency", &log).keyed(StartupServiceType::of::<dyn Store>())
        });
    }
    provider.add_startup_singleton(
        Dependent::new("dependent", &log).depends_on(StartupServiceType::of::<dyn Store>()),
    );

    run(&provider).expect("a resolved service keeps its own key");
    assert_eq!(*log.lock().unwrap(), ["dependency", "dependent"]);
}

#[test]
fn di_startup_registration_key_registry_add_keys_by_the_concrete_type()
-> Result<(), StartupInitializationError> {
    let log = Log::default();
    let registry = StartupRegistry::new()
        .with(Dependent::new("dependent", &log).depends_on(StartupServiceType::of::<Dependency>()))
        .with(Dependency::new("dependency", &log));

    let layers = registry.initialization_layers()?;

    assert_eq!(layers.len(), 2);
    assert_eq!(
        layers[0][0].startup_service_type(),
        Some(StartupServiceType::of::<Dependency>())
    );
    assert_eq!(
        layers[1][0].startup_service_type(),
        Some(StartupServiceType::of::<Dependent>())
    );
    Ok(())
}
