//! A startup service's dependency on a key whose registered service does not initialize (an
//! app's own implementation of a trait, a test's fake) is met: the dependent initializes. A
//! dependency on a key nothing is registered under still fails startup, and so does one on the
//! registration key of a startup service that reports a startup key of its own.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use futures::FutureExt;
use futures::executor::block_on;
use futures::future::LocalBoxFuture;
use rayx_app::prelude::*;

/// What the dependent depends on, by trait.
trait Catalog: Send + Sync {}

/// An implementation that is no startup service.
struct PlainCatalog;

impl Catalog for PlainCatalog {}

/// A startup service that depends on `dyn Catalog`.
#[derive(Clone)]
struct Fonts {
    log: Arc<Mutex<Vec<&'static str>>>,
    initialized: StartupInitializationState,
}

impl AsyncInitializable for Fonts {
    fn is_initialized(&self) -> bool {
        self.initialized.is_initialized()
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let (log, initialized) = (Arc::clone(&self.log), self.initialized.clone());
        async move {
            log.lock().unwrap().push("fonts");
            initialized.mark_initialized();
            Ok(())
        }
        .boxed_local()
    }
}

impl StartupInitializable for Fonts {
    fn initialization_depends_on(&self) -> Vec<StartupServiceType> {
        vec![StartupServiceType::of::<dyn Catalog>()]
    }
}

fn start(with_catalog: bool) -> (Result<(), String>, Vec<&'static str>) {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut builder = RayXAppBuilder::new();
    let services = builder.services_mut();
    if with_catalog {
        services.add_singleton_as::<dyn Catalog>(Arc::new(PlainCatalog) as Arc<dyn Catalog>);
    }
    services.add_startup_singleton(Fonts {
        log: Arc::clone(&log),
        initialized: StartupInitializationState::default(),
    });
    let root = builder.build().unwrap();
    let started = block_on(
        root.startup_registry()
            .initialize(StartupInitializationContext::new(root.clone())),
    )
    .map_err(|error| error.to_string());
    let log = log.lock().unwrap().clone();
    (started, log)
}

#[test]
fn a_dependency_on_a_registered_plain_service_is_met() {
    let (started, log) = start(true);
    assert_eq!(started, Ok(()));
    assert_eq!(log, ["fonts"]);
}

#[test]
fn a_dependency_on_nothing_registered_still_fails() {
    let (started, log) = start(false);
    let error = started.unwrap_err();
    assert!(
        error.contains("no registered startup service matches that dependency"),
        "{error}"
    );
    assert!(log.is_empty());
}

/// A startup service registered as itself that reports `dyn Catalog` as its startup key.
#[derive(Clone)]
struct KeyedCatalog {
    initialized: StartupInitializationState,
}

impl AsyncInitializable for KeyedCatalog {
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

impl StartupInitializable for KeyedCatalog {
    fn startup_service_type(&self) -> Option<StartupServiceType> {
        Some(StartupServiceType::of::<dyn Catalog>())
    }
}

/// Depends on `KeyedCatalog`'s registration key, which is not its startup key.
#[derive(Clone)]
struct OnRegistrationKey {
    initialized: StartupInitializationState,
}

impl AsyncInitializable for OnRegistrationKey {
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

impl StartupInitializable for OnRegistrationKey {
    fn initialization_depends_on(&self) -> Vec<StartupServiceType> {
        vec![StartupServiceType::of::<KeyedCatalog>()]
    }
}

#[test]
fn a_dependency_on_a_startup_service_s_registration_key_under_another_startup_key_still_fails() {
    let mut builder = RayXAppBuilder::new();
    let services = builder.services_mut();
    services.add_startup_singleton(KeyedCatalog {
        initialized: StartupInitializationState::default(),
    });
    services.add_startup_singleton(OnRegistrationKey {
        initialized: StartupInitializationState::default(),
    });
    let root = builder.build().unwrap();
    assert!(root.contains_service::<KeyedCatalog>());
    let error = block_on(
        root.startup_registry()
            .initialize(StartupInitializationContext::new(root.clone())),
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("no registered startup service matches that dependency")
            && error.contains("KeyedCatalog"),
        "{error}"
    );
}
