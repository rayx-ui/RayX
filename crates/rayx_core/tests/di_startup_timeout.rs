//! Startup initialization timeouts: each service may set its own, the registry has a default,
//! and the executor's timer enforces them (`StartupRegistry::initialize_with_timer`).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use futures::FutureExt;
use futures::future::{LocalBoxFuture, pending, ready};
use rayx_core::prelude::*;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A startup service whose init completes at once, or never (`hangs`), with an optional
/// timeout of its own.
macro_rules! service {
    ($name:ident, hangs = $hangs:expr, timeout = $timeout:expr) => {
        #[derive(Clone, Default, AppService)]
        #[app_service(startup)]
        struct $name {
            initialized: StartupInitializationState,
        }

        impl AsyncInitializable for $name {
            fn is_initialized(&self) -> bool {
                self.initialized.is_initialized()
            }

            fn initialize(
                &self,
                _context: StartupInitializationContext,
            ) -> LocalBoxFuture<'static, StartupInitializationResult> {
                let initialized = self.initialized.clone();
                if $hangs {
                    pending().boxed_local()
                } else {
                    async move {
                        initialized.mark_initialized();
                        Ok(())
                    }
                    .boxed_local()
                }
            }

            fn initialization_timeout(&self) -> Option<Duration> {
                $timeout
            }
        }

        impl StartupInitializable for $name {
            fn startup_service_type(&self) -> StartupServiceType {
                StartupServiceType::of::<Self>()
            }
        }
    };
}

service!(
    Quick,
    hangs = false,
    timeout = Some(Duration::from_millis(5))
);
service!(Defaulted, hangs = false, timeout = None);
service!(
    Stuck,
    hangs = true,
    timeout = Some(Duration::from_millis(250))
);

/// A timer that records what it was asked for and expires at once (`expire`) or never.
fn timer(
    asked: &Arc<Mutex<Vec<Duration>>>,
    expire: bool,
) -> impl Fn(Duration) -> LocalBoxFuture<'static, ()> {
    let asked = Arc::clone(asked);
    move |duration| {
        asked.lock().unwrap().push(duration);
        if expire {
            ready(()).boxed_local()
        } else {
            pending().boxed_local()
        }
    }
}

#[test]
fn di_startup_timeout_is_per_service_with_the_registry_default() {
    let mut provider = ServiceProvider::default()
        .with_startup_singleton(Quick::default())
        .with_startup_singleton(Defaulted::default());
    assert_eq!(
        provider.startup_registry().default_initialization_timeout(),
        StartupRegistry::DEFAULT_INITIALIZATION_TIMEOUT
    );
    provider.set_default_initialization_timeout(Duration::from_secs(7));
    let asked = Arc::new(Mutex::new(Vec::new()));
    futures::executor::block_on(provider.startup_registry().initialize_with_timer(
        StartupInitializationContext::new(provider.clone()),
        timer(&asked, false),
    ))
    .expect("services that finish in time initialize");

    let mut asked = asked.lock().unwrap().clone();
    asked.sort();
    assert_eq!(asked, [Duration::from_millis(5), Duration::from_secs(7)]);
    assert!(
        provider
            .required_service::<Quick>()
            .unwrap()
            .is_initialized()
    );
    assert!(
        provider
            .required_service::<Defaulted>()
            .unwrap()
            .is_initialized()
    );
}

#[test]
fn di_startup_timeout_fails_startup_with_the_service_that_hung() {
    let provider = ServiceProvider::default()
        .with_startup_singleton(Stuck::default())
        .with_startup_singleton(Quick::default());
    let asked = Arc::new(Mutex::new(Vec::new()));
    let error = futures::executor::block_on(provider.startup_registry().initialize_with_timer(
        StartupInitializationContext::new(provider.clone()),
        |duration| {
            asked.lock().unwrap().push(duration);
            // Only the hung service's deadline expires.
            if duration == Duration::from_millis(250) {
                ready(()).boxed_local()
            } else {
                pending().boxed_local()
            }
        },
    ))
    .expect_err("a service past its timeout fails startup");

    let StartupInitializationError::ServiceFailures { failures } = &error else {
        panic!("expected service failures, got {error}");
    };
    assert_eq!(failures.as_slice().len(), 1);
    let failure = &failures.as_slice()[0];
    assert_eq!(failure.service, StartupServiceType::of::<Stuck>());
    assert!(failure.timed_out);
    assert_eq!(failure.message, "did not initialize within 250 ms");
    assert!(error.to_string().contains("Stuck"), "{error}");
}

#[test]
fn di_startup_timeout_is_not_enforced_without_a_timer() {
    // `initialize` (no timer) waits for every service: a quick one finishes.
    let provider = ServiceProvider::default().with_startup_singleton(Quick::default());
    futures::executor::block_on(
        provider
            .startup_registry()
            .initialize(StartupInitializationContext::new(provider.clone())),
    )
    .unwrap();
    assert!(
        provider
            .required_service::<Quick>()
            .unwrap()
            .is_initialized()
    );
}

#[test]
fn di_startup_timeout_default_is_set_from_the_app_builder() {
    let builder = RayXAppBuilder::new().default_initialization_timeout(Duration::from_secs(3));
    assert_eq!(
        builder
            .services
            .startup_registry()
            .default_initialization_timeout(),
        Duration::from_secs(3)
    );
    // Scopes share the registry and its default.
    assert_eq!(
        builder
            .services
            .create_scope()
            .startup_registry()
            .default_initialization_timeout(),
        Duration::from_secs(3)
    );
}
