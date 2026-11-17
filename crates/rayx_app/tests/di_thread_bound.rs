//! Thread-bound services: values that are neither `Send` nor `Sync`, resolved on the thread that
//! made them only and never dropped on another thread.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use rayx_app::prelude::*;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

type TestResult = Result<(), Box<dyn std::error::Error>>;

thread_local! {
    /// Counts drops of `Local` values on the current thread.
    static DROPS: Rc<Cell<usize>> = Rc::new(Cell::new(0));
}

fn drops_here() -> usize {
    DROPS.with(|drops| drops.get())
}

/// A value that is neither `Send` nor `Sync`: it holds its thread's drop counter.
#[derive(Debug)]
struct Local {
    id: usize,
    drops: Rc<Cell<usize>>,
}

impl Local {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(1);
        Self {
            id: NEXT.fetch_add(1, Ordering::SeqCst),
            drops: DROPS.with(Rc::clone),
        }
    }
}

impl Drop for Local {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

#[derive(Debug, AppService)]
struct Shared(u32);

fn on_another_thread<R: Send + 'static>(work: impl FnOnce() -> R + Send + 'static) -> R {
    thread::spawn(work).join().expect("the other thread")
}

#[test]
fn di_thread_bound_singleton_resolves_on_its_thread() -> TestResult {
    let mut provider = ServiceProvider::new();
    provider.add_singleton_thread_bound(|_| Local::new());

    let first = provider.required_thread_bound_service::<Local>()?;
    let again = provider.required_thread_bound_service::<Local>()?;
    let from_scope = provider
        .create_scope()
        .required_thread_bound_service::<Local>()?;

    assert!(Rc::ptr_eq(&first, &again));
    assert!(Rc::ptr_eq(&first, &from_scope));
    Ok(())
}

#[test]
fn di_thread_bound_scoped_is_one_value_per_scope() -> TestResult {
    let mut provider = ServiceProvider::new();
    provider.add_scoped_thread_bound(|_| Local::new());

    let scope = provider.create_scope();
    let other = provider.create_scope();
    let first = scope.required_thread_bound_service::<Local>()?;
    let again = scope.required_thread_bound_service::<Local>()?;
    let from_other = other.required_thread_bound_service::<Local>()?;

    assert!(Rc::ptr_eq(&first, &again));
    assert_ne!(first.id, from_other.id);
    Ok(())
}

#[test]
fn di_thread_bound_transient_is_new_on_every_resolve() -> TestResult {
    let mut provider = ServiceProvider::new();
    provider.add_transient_thread_bound(|_| Local::new());

    let first = provider.required_thread_bound_service::<Local>()?;
    let second = provider.required_thread_bound_service::<Local>()?;

    assert_ne!(first.id, second.id);
    let before = drops_here();
    drop(first);
    assert_eq!(
        drops_here(),
        before + 1,
        "a transient value lives as long as its caller holds it"
    );
    Ok(())
}

#[test]
fn di_thread_bound_keyed_services_resolve_on_their_thread() -> TestResult {
    let singleton = RuntimeKey::new("LocalSingleton");
    let scoped = RuntimeKey::new("LocalScoped");
    let transient = RuntimeKey::new("LocalTransient");
    let mut provider = ServiceProvider::new();
    let make = |_: &ServiceProvider| -> Result<LocalService, ServiceResolutionError> {
        Ok(Rc::new(Local::new()))
    };
    provider.add_singleton_thread_bound_keyed(singleton.clone(), make);
    provider.add_scoped_thread_bound_keyed(scoped.clone(), make);
    provider.add_transient_thread_bound_keyed(transient.clone(), make);

    let local = |value: LocalService| value.downcast::<Local>().expect("a `Local`");
    let scope = provider.create_scope();
    let singleton_value = local(provider.required_thread_bound_service_by_key(&singleton)?);
    assert!(Rc::ptr_eq(
        &singleton_value,
        &local(scope.required_thread_bound_service_by_key(&singleton)?)
    ));
    let scoped_value = local(scope.required_thread_bound_service_by_key(&scoped)?);
    assert!(Rc::ptr_eq(
        &scoped_value,
        &local(scope.required_thread_bound_service_by_key(&scoped)?)
    ));
    assert_ne!(
        scoped_value.id,
        local(provider.required_thread_bound_service_by_key(&scoped)?).id
    );
    assert_ne!(
        local(provider.required_thread_bound_service_by_key(&transient)?).id,
        local(provider.required_thread_bound_service_by_key(&transient)?).id
    );
    Ok(())
}

#[test]
fn di_thread_bound_resolve_on_another_thread_fails_without_touching_the_value() -> TestResult {
    let key = RuntimeKey::new("LocalKeyed");
    let factory_runs = Arc::new(AtomicUsize::new(0));
    let mut provider = ServiceProvider::new();
    provider.add_singleton_thread_bound({
        let factory_runs = Arc::clone(&factory_runs);
        move |_| {
            factory_runs.fetch_add(1, Ordering::SeqCst);
            Local::new()
        }
    });
    provider.add_singleton_thread_bound_keyed(key.clone(), |_| Ok(Rc::new(Local::new())));
    let value = provider.required_thread_bound_service::<Local>()?;
    provider.required_thread_bound_service_by_key(&key)?;
    let drops_before = drops_here();

    let (typed, keyed, optional) = on_another_thread({
        let provider = provider.clone();
        move || {
            (
                provider
                    .required_thread_bound_service::<Local>()
                    .expect_err("another thread must fail"),
                provider
                    .required_thread_bound_service_by_key(&key)
                    .expect_err("another thread must fail"),
                provider.thread_bound_service::<Local>().is_none(),
            )
        }
    });

    assert_eq!(
        typed,
        ServiceResolutionError::WrongThread {
            service: std::any::type_name::<Local>().into()
        }
    );
    assert!(typed.to_string().contains("its own thread only"));
    assert_eq!(
        keyed,
        ServiceResolutionError::WrongThread {
            service: "LocalKeyed".into()
        }
    );
    assert!(optional);
    assert_eq!(
        factory_runs.load(Ordering::SeqCst),
        1,
        "no value made elsewhere"
    );
    assert_eq!(drops_here(), drops_before, "no value dropped");
    assert!(Rc::ptr_eq(
        &value,
        &provider.required_thread_bound_service::<Local>()?
    ));
    Ok(())
}

#[test]
fn di_thread_bound_scoped_value_is_dropped_with_its_scope_on_its_thread() -> TestResult {
    let mut provider = ServiceProvider::new();
    provider.add_scoped_thread_bound(|_| Local::new());
    let scope = provider.create_scope();
    scope.required_thread_bound_service::<Local>()?;
    let before = drops_here();

    drop(scope);

    assert_eq!(drops_here(), before + 1);
    Ok(())
}

#[test]
fn di_thread_bound_provider_dropped_elsewhere_drops_the_value_later_on_its_thread() -> TestResult {
    let mut provider = ServiceProvider::new();
    provider.add_singleton_thread_bound(|_| Local::new());
    provider.add_transient_thread_bound(|_| 0_u8);
    let id = provider.required_thread_bound_service::<Local>()?.id;
    let before = drops_here();

    let dropped_elsewhere = on_another_thread(move || {
        let drops_there = drops_here();
        drop(provider);
        drops_here() - drops_there
    });

    assert_eq!(dropped_elsewhere, 0, "never dropped on the other thread");
    assert_eq!(
        drops_here(),
        before,
        "not yet dropped: released on the next table access"
    );

    let mut next = ServiceProvider::new();
    next.add_transient_thread_bound(|_| Local::new());
    let fresh = next.required_thread_bound_service::<Local>()?;
    assert_ne!(fresh.id, id);
    assert_eq!(drops_here(), before + 1, "released on its own thread");
    Ok(())
}

#[test]
fn di_thread_bound_orphans_can_be_released_explicitly() -> TestResult {
    let mut provider = ServiceProvider::new();
    provider.add_singleton_thread_bound(|_| Local::new());
    provider.required_thread_bound_service::<Local>()?;
    let before = drops_here();

    on_another_thread(move || drop(provider));

    assert_eq!(release_orphaned_thread_bound_services(), 1);
    assert_eq!(drops_here(), before + 1);
    assert_eq!(release_orphaned_thread_bound_services(), 0);
    Ok(())
}

#[test]
fn di_thread_bound_value_made_on_another_thread_belongs_to_it() -> TestResult {
    let mut provider = ServiceProvider::new();
    provider.add_singleton_thread_bound(|_| Local::new());

    let made_there = on_another_thread({
        let provider = provider.clone();
        move || {
            let drops_there = drops_here();
            let id = provider
                .required_thread_bound_service::<Local>()
                .map(|value| value.id);
            drop(provider);
            (id.is_ok(), drops_here() - drops_there)
        }
    });

    assert_eq!(
        made_there,
        (true, 0),
        "made there, still held by this provider"
    );
    assert!(matches!(
        provider.required_thread_bound_service::<Local>(),
        Err(ServiceResolutionError::WrongThread { .. })
    ));
    Ok(())
}

#[test]
fn di_thread_bound_typed_services_stay_usable_from_every_thread() -> TestResult {
    let mut provider = ServiceProvider::new();
    provider.add_singleton(Shared(3));
    provider.add_scoped::<Arc<AtomicUsize>>(|_| Arc::new(AtomicUsize::new(0)));
    provider.add_singleton_thread_bound(|_| Local::new());
    provider.required_thread_bound_service::<Local>()?;

    let there = on_another_thread({
        let provider = provider.clone();
        move || {
            let shared = provider.required_service::<Shared>().map(|value| value.0);
            let scoped = provider
                .create_scope()
                .required_service::<Arc<AtomicUsize>>()
                .is_ok();
            (shared.ok(), scoped)
        }
    });

    assert_eq!(there, (Some(3), true));
    assert_eq!(provider.required_service::<Shared>()?.0, 3);
    Ok(())
}

#[test]
fn di_thread_bound_and_shared_resolution_name_the_other_api() -> TestResult {
    let local = RuntimeKey::new("LocalOnly");
    let shared = RuntimeKey::new("SharedOnly");
    let mut provider = ServiceProvider::new();
    provider.add_singleton_thread_bound_keyed(local.clone(), |_| Ok(Rc::new(Local::new())));
    provider.add_singleton_keyed(shared.clone(), |_| {
        let value: ErasedService = Arc::new(Shared(1));
        Ok(value)
    });

    let error = provider
        .required_service_by_key(&local)
        .expect_err("thread-bound");
    assert!(error.to_string().contains("thread_bound_service_by_key"));
    let error = provider
        .required_thread_bound_service_by_key(&shared)
        .expect_err("not thread-bound");
    assert!(error.to_string().contains("not thread-bound"));
    assert!(matches!(
        provider.required_thread_bound_service_by_key(&RuntimeKey::new("Nothing")),
        Err(ServiceResolutionError::MissingKey { .. })
    ));
    Ok(())
}

#[derive(Debug)]
struct SendableLocal(usize);

#[test]
fn di_thread_bound_resolve_through_the_shared_api_makes_nothing() -> TestResult {
    let key = RuntimeKey::new("LocalElsewhere");
    let runs = Arc::new(AtomicUsize::new(0));
    let mut provider = ServiceProvider::new();
    provider.add_singleton_thread_bound_keyed(key.clone(), {
        let runs = Arc::clone(&runs);
        move |_| {
            runs.fetch_add(1, Ordering::SeqCst);
            Ok(Rc::new(Local::new()))
        }
    });
    provider.add_singleton_thread_bound({
        let runs = Arc::clone(&runs);
        move |_| {
            runs.fetch_add(1, Ordering::SeqCst);
            SendableLocal(1)
        }
    });

    let refused = on_another_thread({
        let provider = provider.clone();
        let key = key.clone();
        move || {
            let keyed = provider.required_service_by_key(&key).is_err();
            let typed = provider.service::<SendableLocal>().is_none();
            let typed_error = provider
                .required_service::<SendableLocal>()
                .expect_err("thread-bound")
                .to_string();
            (keyed, typed, typed_error)
        }
    });

    assert!(refused.0 && refused.1);
    assert!(refused.2.contains("thread-bound"), "{}", refused.2);
    assert_eq!(
        runs.load(Ordering::SeqCst),
        0,
        "no factory ran on the other thread"
    );
    provider.required_thread_bound_service_by_key(&key)?;
    assert_eq!(
        provider.required_thread_bound_service::<SendableLocal>()?.0,
        1,
        "still made, and owned, by the first thread-bound resolve"
    );
    assert_eq!(runs.load(Ordering::SeqCst), 2);
    Ok(())
}

#[test]
fn di_thread_bound_api_on_a_shared_service_makes_nothing() -> TestResult {
    let key = RuntimeKey::new("SharedKeyed");
    let runs = Arc::new(AtomicUsize::new(0));
    let mut provider = ServiceProvider::new();
    provider.add_singleton_keyed(key.clone(), {
        let runs = Arc::clone(&runs);
        move |_| {
            runs.fetch_add(1, Ordering::SeqCst);
            let value: ErasedService = Arc::new(Shared(2));
            Ok(value)
        }
    });
    provider.add_singleton(Shared(3));

    assert!(provider.required_thread_bound_service_by_key(&key).is_err());
    assert!(provider.thread_bound_service::<Shared>().is_none());
    assert_eq!(
        runs.load(Ordering::SeqCst),
        0,
        "the shared factory did not run"
    );
    assert!(
        provider
            .required_service_by_key(&key)?
            .downcast::<Shared>()
            .is_ok()
    );
    assert_eq!(provider.required_service::<Shared>()?.0, 3);
    Ok(())
}
