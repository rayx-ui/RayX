//! The startup registry (ported from RayX's `rayx_runtime::di` tests; the GPUI
//! phase is not part of the core).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use futures::FutureExt;
use futures::future::{LocalBoxFuture, poll_fn};
use rayx_core::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::Poll;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn initialize(provider: &ServiceProvider) -> Result<(), StartupInitializationError> {
    futures::executor::block_on(
        provider
            .startup_registry()
            .initialize(StartupInitializationContext::new(provider.clone())),
    )
}

macro_rules! recorder {
    ($name:ident $(, depends_on = $dependency:ty)?) => {
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
                async move {
                    initialized.mark_initialized();
                    Ok(())
                }
                .boxed_local()
            }
        }

        impl StartupInitializable for $name {
            fn startup_service_type(&self) -> StartupServiceType {
                StartupServiceType::of::<Self>()
            }

            $(fn initialization_depends_on(&self) -> Vec<StartupServiceType> {
                vec![StartupServiceType::of::<$dependency>()]
            })?
        }
    };
}

recorder!(Recorder);
recorder!(DependencyRecorder);
recorder!(DependentRecorder, depends_on = DependencyRecorder);
recorder!(MissingDependencyRecorder, depends_on = Unregistered);
recorder!(Unregistered);
recorder!(CycleA, depends_on = CycleB);
recorder!(CycleB, depends_on = CycleA);

impl ShutdownService for Recorder {
    fn shutdown(&self, services: &ServiceProvider) -> Result<(), String> {
        services
            .required_service::<Recorder>()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

trait TraitStartupRecorder: StartupInitializable {
    fn was_initialized(&self) -> bool;
}

#[derive(Clone, Default)]
struct TraitStartupRecorderImpl {
    inner: Recorder,
}

impl AsyncInitializable for TraitStartupRecorderImpl {
    fn is_initialized(&self) -> bool {
        self.inner.is_initialized()
    }

    fn initialize(
        &self,
        context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        self.inner.initialize(context)
    }
}

impl StartupInitializable for TraitStartupRecorderImpl {
    fn startup_service_type(&self) -> StartupServiceType {
        StartupServiceType::of::<dyn TraitStartupRecorder>()
    }
}

impl TraitStartupRecorder for TraitStartupRecorderImpl {
    fn was_initialized(&self) -> bool {
        self.inner.is_initialized()
    }
}

#[derive(Clone)]
struct ParallelRecorder<const ID: usize> {
    started: Arc<AtomicUsize>,
    events: Arc<Mutex<Vec<String>>>,
}

impl<const ID: usize> AsyncInitializable for ParallelRecorder<ID> {
    fn is_initialized(&self) -> bool {
        false
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let started = Arc::clone(&self.started);
        let events = Arc::clone(&self.events);
        async move {
            events
                .lock()
                .map_err(|_| StartupServiceError::new("events poisoned"))?
                .push(format!("{ID}:start"));
            started.fetch_add(1, Ordering::SeqCst);
            poll_fn(move |cx| {
                if started.load(Ordering::SeqCst) >= 2 {
                    Poll::Ready(())
                } else {
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            })
            .await;
            events
                .lock()
                .map_err(|_| StartupServiceError::new("events poisoned"))?
                .push(format!("{ID}:finish"));
            Ok(())
        }
        .boxed_local()
    }
}

impl<const ID: usize> StartupInitializable for ParallelRecorder<ID> {
    fn startup_service_type(&self) -> StartupServiceType {
        StartupServiceType::of::<Self>()
    }
}

#[derive(Clone, Default)]
struct FailingRecorder;

impl AsyncInitializable for FailingRecorder {
    fn is_initialized(&self) -> bool {
        false
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        async move { Err(StartupServiceError::from("boom")) }.boxed_local()
    }
}

impl StartupInitializable for FailingRecorder {
    fn startup_service_type(&self) -> StartupServiceType {
        StartupServiceType::of::<Self>()
    }
}

#[test]
fn di_startup_singleton_reuses_instance_and_registers_startup() -> TestResult {
    let provider = ServiceProvider::default().with_startup_singleton(Recorder::default());
    let service = provider.required_service::<Recorder>()?;

    assert_eq!(provider.startup_registry().services().len(), 1);
    assert!(provider.has_startup_role::<Recorder>());
    initialize(&provider)?;
    assert!(service.is_initialized());
    service.shutdown(&provider)?;
    Ok(())
}

#[test]
fn di_startup_scoped_and_transient_resolve_and_initialize() -> TestResult {
    let mut provider = ServiceProvider::default();
    provider.add_startup_scoped::<Recorder>(|_| Recorder::default());
    provider.add_startup_transient::<DependencyRecorder>(|_| DependencyRecorder::default());

    assert!(provider.startup_registry().services().is_empty());
    assert!(!provider.startup_registry().is_empty());
    initialize(&provider)?;
    assert!(provider.required_service::<Recorder>()?.is_initialized());
    assert!(
        !provider
            .required_service::<DependencyRecorder>()?
            .is_initialized()
    );
    Ok(())
}

#[test]
fn di_startup_trait_object_services_initialize() -> TestResult {
    let service: Arc<dyn TraitStartupRecorder> = Arc::new(TraitStartupRecorderImpl::default());
    let provider = ServiceProvider::default()
        .with_startup_singleton_as::<dyn TraitStartupRecorder>(Arc::clone(&service));

    assert_eq!(provider.startup_registry().services().len(), 1);
    let resolved = provider.required_service::<dyn TraitStartupRecorder>()?;
    assert!(Arc::ptr_eq(&service, &resolved));
    initialize(&provider)?;
    assert!(resolved.was_initialized());
    Ok(())
}

#[test]
fn di_startup_try_add_replace_order_matrix() -> TestResult {
    let mut provider = ServiceProvider::default();
    assert!(provider.try_add_app_service_factory::<Recorder>(|_| Recorder::default()));
    let custom = Recorder::default();
    provider.replace_app_service_factory::<Recorder>({
        let custom = custom.clone();
        move |_| custom.clone()
    });
    assert!(!provider.try_add_app_service_factory::<Recorder>(|_| {
        panic!("TryAdd must not invoke or install a later default factory")
    }));

    initialize(&provider)?;
    assert!(
        custom.is_initialized(),
        "the replacing factory was selected"
    );

    let mut plain = ServiceProvider::default();
    plain.try_add_app_service_factory::<Recorder>(|_| Recorder::default());
    plain.replace_service_factory::<Recorder>(|_| Recorder::default());
    assert!(
        plain.startup_registry().is_empty(),
        "a plain replacement removes the startup role"
    );
    initialize(&plain)?;
    assert!(!plain.required_service::<Recorder>()?.is_initialized());
    Ok(())
}

#[test]
fn di_startup_registry_builds_dependency_layers() -> TestResult {
    let registry = StartupRegistry::new()
        .with(DependentRecorder::default())
        .with(DependencyRecorder::default())
        .with(Recorder::default());

    let layers = registry.initialization_layers()?;
    assert_eq!(layers.len(), 2);
    assert_eq!(layers[0].len(), 2);
    assert!(
        layers[0]
            .iter()
            .any(|service| service.startup_service_type() == StartupServiceType::of::<Recorder>())
    );
    assert_eq!(
        layers[1][0].startup_service_type(),
        StartupServiceType::of::<DependentRecorder>()
    );
    Ok(())
}

#[test]
fn di_startup_registry_reports_missing_dependencies() {
    let registry = StartupRegistry::new().with(MissingDependencyRecorder::default());
    let error = registry
        .initialization_layers()
        .err()
        .expect("missing dependency should fail");
    assert!(matches!(
        error,
        StartupInitializationError::MissingDependency {
            phase: StartupInitializationPhase::Runtime,
            service,
            dependency,
        }
            if service == StartupServiceType::of::<MissingDependencyRecorder>()
                && dependency == StartupServiceType::of::<Unregistered>()
    ));
    assert!(error.to_string().contains("Unregistered"));
}

#[test]
fn di_startup_registry_reports_cycles() {
    let registry = StartupRegistry::new()
        .with(CycleA::default())
        .with(CycleB::default());
    let error = registry
        .initialization_layers()
        .err()
        .expect("cycle should fail");
    assert!(matches!(
        error,
        StartupInitializationError::CircularDependency { .. }
    ));
}

#[test]
fn di_startup_registry_reports_service_failures() {
    let provider = ServiceProvider::default().with_startup_singleton(FailingRecorder);
    let error = initialize(&provider).expect_err("failing service should fail startup");
    assert!(matches!(
        &error,
        StartupInitializationError::ServiceFailures { failures }
            if failures.as_slice().len() == 1 && failures.as_slice()[0].message == "boom"
    ));

    let shared = ServiceProvider::default().with_startup_service(Arc::new(Recorder::default()));
    initialize(&shared).expect("shared startup services need no registration");

    let mut registry = StartupRegistry::new();
    registry.add_resolved::<Unregistered>();
    let error = futures::executor::block_on(
        registry.initialize(StartupInitializationContext::new(ServiceProvider::default())),
    )
    .expect_err("an unregistered resolved startup service must fail");
    assert!(error.to_string().contains("Unregistered"));
}

#[test]
fn di_startup_registry_initializes_each_layer_concurrently() -> TestResult {
    let started = Arc::new(AtomicUsize::new(0));
    let events = Arc::new(Mutex::new(Vec::new()));
    let registry = StartupRegistry::new()
        .with(ParallelRecorder::<1> {
            started: Arc::clone(&started),
            events: Arc::clone(&events),
        })
        .with(ParallelRecorder::<2> {
            started: Arc::clone(&started),
            events: Arc::clone(&events),
        });

    futures::executor::block_on(
        registry.initialize(StartupInitializationContext::new(ServiceProvider::default())),
    )?;

    let events = events.lock().map_err(|_| "events poisoned")?;
    let first_finish = events
        .iter()
        .position(|event| event.ends_with(":finish"))
        .ok_or("missing finish event")?;
    assert_eq!(first_finish, 2);
    Ok(())
}
