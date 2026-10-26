//! Service lifetimes, scopes, trait-object registration and resolution errors
//! (ported from RayX's `rayx_runtime::di` tests).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use rayx_core::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[derive(Debug, AppService)]
struct IdentifiedService {
    id: u32,
    value: u32,
}

impl IdentifiedService {
    fn next(value: u32) -> Self {
        static NEXT_ID: AtomicU32 = AtomicU32::new(1);
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::SeqCst),
            value,
        }
    }
}

trait RegisteredService: Send + Sync {
    fn value(&self) -> u32;
}

impl AppService for dyn RegisteredService {}

#[derive(Debug)]
struct RegisteredServiceImpl(u32);

impl RegisteredService for RegisteredServiceImpl {
    fn value(&self) -> u32 {
        self.0
    }
}

#[derive(Clone, AppService)]
struct NonStartupService;

#[derive(AppService)]
#[app_service(startup)]
struct GenericService<T: Send + Sync + 'static> {
    value: T,
}

/// Counts drops, to observe when a scope releases its instances.
struct DropCounter {
    drops: Arc<AtomicUsize>,
}

struct SingletonDropCounter(#[allow(dead_code)] DropCounter);

impl Drop for DropCounter {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn di_derive_implements_app_service_for_generic_types() {
    fn assert_app_service<T: AppService + ?Sized>() {}
    assert_app_service::<IdentifiedService>();
    assert_app_service::<GenericService<String>>();
    assert_app_service::<dyn RegisteredService>();
    let service = GenericService { value: 3_u8 };
    assert_eq!(service.value, 3);
}

#[test]
fn di_singleton_reuses_instance_without_startup() -> TestResult {
    let provider = ServiceProvider::default().with_singleton(IdentifiedService::next(1));
    let first = provider.required_service::<IdentifiedService>()?;
    let second = provider.required_service::<IdentifiedService>()?;

    assert!(Arc::ptr_eq(&first, &second));
    assert!(provider.startup_registry().is_empty());
    Ok(())
}

#[test]
fn di_singleton_accepts_plain_services_and_arcs() -> TestResult {
    let mut provider = ServiceProvider::default();
    provider.add_singleton(NonStartupService);
    let registered =
        provider.add_singleton::<IdentifiedService>(Arc::new(IdentifiedService::next(2)));

    provider.required_service::<NonStartupService>()?;
    assert!(Arc::ptr_eq(
        &registered,
        &provider.required_service::<IdentifiedService>()?
    ));
    assert!(provider.startup_registry().is_empty());
    Ok(())
}

#[test]
fn di_singleton_is_shared_by_every_scope() -> TestResult {
    let mut provider = ServiceProvider::default();
    provider.replace_service_factory::<IdentifiedService>(|_| IdentifiedService::next(10));
    let first_scope = provider.create_scope();
    let nested_scope = first_scope.create_scope();

    let from_scope = first_scope.required_service::<IdentifiedService>()?;
    let from_root = provider.required_service::<IdentifiedService>()?;
    let from_nested = nested_scope.required_service::<IdentifiedService>()?;

    assert!(Arc::ptr_eq(&from_root, &from_scope));
    assert!(Arc::ptr_eq(&from_root, &from_nested));
    Ok(())
}

#[test]
fn di_singleton_factory_constructs_once() -> TestResult {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut provider = ServiceProvider::default();
    provider.replace_service_factory::<IdentifiedService>({
        let calls = Arc::clone(&calls);
        move |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            IdentifiedService::next(11)
        }
    });
    assert_eq!(calls.load(Ordering::SeqCst), 0, "factories are lazy");

    let clone = provider.clone();
    let scope = provider.create_scope();
    let services = [
        provider.required_service::<IdentifiedService>()?,
        clone.required_service::<IdentifiedService>()?,
        scope.required_service::<IdentifiedService>()?,
    ];

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(
        services
            .iter()
            .all(|service| Arc::ptr_eq(&services[0], service))
    );
    Ok(())
}

#[test]
fn di_scoped_reuses_instances_inside_scope_only() -> TestResult {
    let mut provider = ServiceProvider::default();
    provider.add_scoped::<IdentifiedService>(|_| IdentifiedService::next(300));
    let child = provider.create_scope();
    let sibling = provider.create_scope();

    let child_first = child.required_service::<IdentifiedService>()?;
    let child_second = child.required_service::<IdentifiedService>()?;
    let child_clone = child.clone().required_service::<IdentifiedService>()?;
    let sibling_first = sibling.required_service::<IdentifiedService>()?;

    assert!(Arc::ptr_eq(&child_first, &child_second));
    assert!(
        Arc::ptr_eq(&child_first, &child_clone),
        "clones share their scope"
    );
    assert_ne!(child_first.id, sibling_first.id);
    assert_eq!(child_first.value, 300);
    Ok(())
}

/// RayX behaviour: the root provider is its own scope, so a scoped service
/// resolves from the root (not an error) as an instance distinct from every
/// child scope's.
#[test]
fn di_scoped_resolved_from_root_uses_the_root_scope() -> TestResult {
    let mut provider = ServiceProvider::default();
    provider.add_scoped::<IdentifiedService>(|_| IdentifiedService::next(301));
    let child = provider.create_scope();

    let root_first = provider.required_service::<IdentifiedService>()?;
    let root_second = provider.required_service::<IdentifiedService>()?;
    let child_first = child.required_service::<IdentifiedService>()?;

    assert!(Arc::ptr_eq(&root_first, &root_second));
    assert_ne!(root_first.id, child_first.id);
    Ok(())
}

#[test]
fn di_dropping_a_scope_drops_its_scoped_instances() -> TestResult {
    let scoped_drops = Arc::new(AtomicUsize::new(0));
    let singleton_drops = Arc::new(AtomicUsize::new(0));
    let mut provider = ServiceProvider::default();
    provider.add_scoped::<DropCounter>({
        let drops = Arc::clone(&scoped_drops);
        move |_| DropCounter {
            drops: Arc::clone(&drops),
        }
    });
    provider.replace_service_factory::<SingletonDropCounter>({
        let drops = Arc::clone(&singleton_drops);
        move |_| {
            SingletonDropCounter(DropCounter {
                drops: Arc::clone(&drops),
            })
        }
    });

    let scope = provider.create_scope();
    let clone = scope.clone();
    scope.required_service::<DropCounter>()?;
    scope.required_service::<SingletonDropCounter>()?;
    assert_eq!(scoped_drops.load(Ordering::SeqCst), 0);

    drop(scope);
    assert_eq!(
        scoped_drops.load(Ordering::SeqCst),
        0,
        "a live clone keeps the scope alive"
    );
    drop(clone);
    assert_eq!(scoped_drops.load(Ordering::SeqCst), 1);

    let held = {
        let scope = provider.create_scope();
        scope.required_service::<DropCounter>()?
    };
    assert_eq!(
        scoped_drops.load(Ordering::SeqCst),
        1,
        "a caller's Arc outlives the scope"
    );
    drop(held);
    assert_eq!(scoped_drops.load(Ordering::SeqCst), 2);

    assert_eq!(
        singleton_drops.load(Ordering::SeqCst),
        0,
        "singletons outlive the scope that created them"
    );
    let singleton = provider.required_service::<SingletonDropCounter>()?;
    drop(provider);
    drop(singleton);
    assert_eq!(singleton_drops.load(Ordering::SeqCst), 1);
    Ok(())
}

#[test]
fn di_transient_recreates_instances() -> TestResult {
    let mut provider = ServiceProvider::default();
    provider.add_transient::<IdentifiedService>(|_| IdentifiedService::next(400));
    let scope = provider.create_scope();

    let first = provider.required_service::<IdentifiedService>()?;
    let second = provider.required_service::<IdentifiedService>()?;
    let scoped_first = scope.required_service::<IdentifiedService>()?;
    let scoped_second = scope.required_service::<IdentifiedService>()?;

    assert_ne!(first.id, second.id);
    assert_ne!(scoped_first.id, scoped_second.id);
    assert_eq!(first.value, 400);
    assert_eq!(scoped_second.value, 400);
    Ok(())
}

#[test]
fn di_factories_resolve_dependencies_from_the_provider() -> TestResult {
    let mut provider = ServiceProvider::default();
    provider.add_singleton_as::<dyn RegisteredService>(
        Arc::new(RegisteredServiceImpl(7)) as Arc<dyn RegisteredService>
    );
    provider.add_scoped::<IdentifiedService>(|services| {
        let dependency = services
            .required_service::<dyn RegisteredService>()
            .expect("dependency is registered");
        IdentifiedService::next(dependency.value() * 10)
    });

    let scope = provider.create_scope();
    assert_eq!(scope.required_service::<IdentifiedService>()?.value, 70);
    Ok(())
}

#[test]
fn di_registers_and_scopes_trait_object_services() -> TestResult {
    let service: Arc<dyn RegisteredService> = Arc::new(RegisteredServiceImpl(88));
    let provider = ServiceProvider::default().with_service::<dyn RegisteredService>(service);
    let scoped = provider.create_scope();

    assert_eq!(
        provider
            .required_service::<dyn RegisteredService>()?
            .value(),
        88
    );
    assert_eq!(
        scoped.required_service::<dyn RegisteredService>()?.value(),
        88
    );
    Ok(())
}

#[test]
fn di_registers_singleton_abstractions() -> TestResult {
    let service: Arc<dyn RegisteredService> = Arc::new(RegisteredServiceImpl(91));
    let provider =
        ServiceProvider::default().with_singleton_as::<dyn RegisteredService>(Arc::clone(&service));

    assert!(Arc::ptr_eq(
        &service,
        &provider.required_service::<dyn RegisteredService>()?
    ));
    Ok(())
}

#[test]
fn di_registers_scoped_and_transient_abstractions() -> TestResult {
    let mut provider = ServiceProvider::default();
    provider.add_scoped_as::<dyn RegisteredService>(|_| {
        Arc::new(RegisteredServiceImpl(101)) as Arc<dyn RegisteredService>
    });
    let child = provider.create_scope();

    let root_first = provider.required_service::<dyn RegisteredService>()?;
    let root_second = provider.required_service::<dyn RegisteredService>()?;
    let child_first = child.required_service::<dyn RegisteredService>()?;

    assert!(Arc::ptr_eq(&root_first, &root_second));
    assert!(!Arc::ptr_eq(&root_first, &child_first));

    provider.add_transient_as::<dyn RegisteredService>(|_| {
        Arc::new(RegisteredServiceImpl(202)) as Arc<dyn RegisteredService>
    });
    let transient_first = provider.required_service::<dyn RegisteredService>()?;
    let transient_second = provider.required_service::<dyn RegisteredService>()?;

    assert!(!Arc::ptr_eq(&transient_first, &transient_second));
    assert_eq!(transient_first.value(), 202);
    assert_eq!(transient_second.value(), 202);
    Ok(())
}

#[test]
fn di_try_add_keeps_the_first_registration_and_replace_overrides() -> TestResult {
    let mut provider = ServiceProvider::default();
    assert!(provider.try_add_service_factory::<IdentifiedService>(|_| IdentifiedService::next(1)));
    assert!(!provider.try_add_service_factory::<IdentifiedService>(|_| {
        panic!("TryAdd must not invoke or install a later factory")
    }));
    assert_eq!(provider.required_service::<IdentifiedService>()?.value, 1);

    provider.replace_service(IdentifiedService::next(2));
    assert_eq!(provider.required_service::<IdentifiedService>()?.value, 2);
    assert!(!provider.try_add_service(IdentifiedService::next(3)));
    assert!(provider.contains_service::<IdentifiedService>());
    assert!(!provider.contains_service::<NonStartupService>());
    Ok(())
}

#[test]
fn di_missing_service_is_an_error_naming_the_type() {
    let provider = ServiceProvider::default();
    let scope = provider.create_scope();

    assert!(provider.service::<IdentifiedService>().is_none());
    let error = provider
        .required_service::<IdentifiedService>()
        .expect_err("unregistered service must fail");
    assert_eq!(
        error,
        ServiceResolutionError::Missing {
            service: std::any::type_name::<IdentifiedService>()
        }
    );
    assert!(error.to_string().contains("IdentifiedService"));
    assert!(error.to_string().contains("not registered"));

    let error = scope
        .required_service::<dyn RegisteredService>()
        .err()
        .expect("unregistered trait object must fail");
    assert!(error.to_string().contains("RegisteredService"));
}

#[test]
fn di_scope_snapshots_registrations_at_creation() -> TestResult {
    let mut provider = ServiceProvider::default();
    let early = provider.create_scope();
    provider.add_scoped::<IdentifiedService>(|_| IdentifiedService::next(5));

    assert!(early.required_service::<IdentifiedService>().is_err());
    provider
        .create_scope()
        .required_service::<IdentifiedService>()?;
    Ok(())
}

#[test]
fn di_scope_wrappers_carry_service_provider() -> TestResult {
    let provider = ServiceProvider::default().with_singleton(NonStartupService);
    let app = AppScope(provider.clone());
    let window = WindowScope(provider.clone());
    let view = ViewScope(provider);

    app.0.required_service::<NonStartupService>()?;
    window.0.required_service::<NonStartupService>()?;
    view.0.required_service::<NonStartupService>()?;
    assert!(default_provider().startup_registry().is_empty());
    Ok(())
}

#[test]
fn di_service_registry_reports_lifetimes_through_the_provider() -> TestResult {
    let mut registry = ServiceRegistry::new();
    registry.insert(Arc::new(NonStartupService));
    assert!(registry.contains::<NonStartupService>());
    registry.required::<NonStartupService>()?;
    assert!(registry.get::<IdentifiedService>().is_none());
    assert_ne!(ServiceLifetime::Singleton, ServiceLifetime::Scoped);
    Ok(())
}
