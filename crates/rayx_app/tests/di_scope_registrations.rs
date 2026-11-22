//! Scopes with registrations of their own (`ServiceProvider::create_scope_with`): a scope's
//! registration wins over the root's for its key, also once the root made it, in that scope and
//! its descendants only; its singletons are one per declaring scope; the root's singletons are
//! made with the root; scoped and transient services resolve in the provider they are resolved
//! from; a missing dependency fails as it does in the root.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use rayx_app::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

type TestResult = Result<(), Box<dyn std::error::Error>>;

trait Clock: Send + Sync {
    fn now(&self) -> u32;
}

impl AppService for dyn Clock {}

struct FixedClock(u32);

impl Clock for FixedClock {
    fn now(&self) -> u32 {
        self.0
    }
}

fn clock(now: u32) -> Arc<dyn Clock> {
    Arc::new(FixedClock(now))
}

/// A service that reads the clock when it is made.
#[derive(AppService)]
struct Stamp(u32);

/// A scoped service, numbered as it is made.
#[derive(AppService)]
struct Page(usize);

/// A singleton depending on the scoped `Page`.
#[derive(AppService)]
struct Cache {
    page: Arc<Page>,
}

/// Counts drops.
#[derive(AppService)]
struct Counted(Arc<AtomicUsize>);

impl Drop for Counted {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// A service that is never registered.
#[derive(AppService)]
struct Missing;

fn stamp(services: &ServiceProvider) -> Stamp {
    Stamp(services.required_service::<dyn Clock>().unwrap().now())
}

#[test]
fn a_scope_replaces_a_key_the_root_made_already() -> TestResult {
    let mut root = ServiceProvider::new();
    root.add_singleton_as::<dyn Clock>(clock(1));
    let before = root.create_scope();
    assert_eq!(root.required_service::<dyn Clock>()?.now(), 1);

    let scope = root.create_scope_with(|scope| {
        scope.add_singleton_as::<dyn Clock>(clock(2));
    });
    let sibling = root.create_scope();

    assert_eq!(scope.required_service::<dyn Clock>()?.now(), 2);
    assert_eq!(
        scope.create_scope().required_service::<dyn Clock>()?.now(),
        2
    );
    assert_eq!(root.required_service::<dyn Clock>()?.now(), 1);
    assert_eq!(before.required_service::<dyn Clock>()?.now(), 1);
    assert_eq!(sibling.required_service::<dyn Clock>()?.now(), 1);
    Ok(())
}

#[test]
fn a_factory_singleton_made_by_the_root_is_replaced_too() -> TestResult {
    let mut root = ServiceProvider::new();
    root.replace_service_factory::<Stamp>(|_| Stamp(1));
    let made = root.required_service::<Stamp>()?;

    let scope = root.create_scope_with(|scope| {
        scope.add_singleton_factory::<Stamp>(|_| Stamp(2));
    });

    assert_eq!(scope.required_service::<Stamp>()?.0, 2);
    assert!(Arc::ptr_eq(&made, &root.required_service::<Stamp>()?));
    Ok(())
}

#[test]
fn the_nearest_scope_wins() -> TestResult {
    let mut root = ServiceProvider::new();
    root.add_singleton_as::<dyn Clock>(clock(1));
    let outer = root.create_scope_with(|scope| {
        scope.add_singleton_as::<dyn Clock>(clock(2));
    });
    let inner = outer.create_scope().create_scope_with(|scope| {
        scope.add_singleton_as::<dyn Clock>(clock(3));
    });

    assert_eq!(inner.required_service::<dyn Clock>()?.now(), 3);
    assert_eq!(
        inner.create_scope().required_service::<dyn Clock>()?.now(),
        3
    );
    assert_eq!(outer.required_service::<dyn Clock>()?.now(), 2);
    assert_eq!(root.required_service::<dyn Clock>()?.now(), 1);
    Ok(())
}

#[test]
fn a_root_singleton_first_resolved_in_a_scope_keeps_the_roots_dependency() -> TestResult {
    let mut root = ServiceProvider::new();
    root.add_singleton_as::<dyn Clock>(clock(1));
    root.replace_service_factory::<Stamp>(stamp);

    let scope = root.create_scope_with(|scope| {
        scope.add_singleton_as::<dyn Clock>(clock(2));
    });
    let first = scope.required_service::<Stamp>()?;

    assert_eq!(first.0, 1, "the root's singleton is made with the root");
    assert!(Arc::ptr_eq(&first, &root.required_service::<Stamp>()?));
    Ok(())
}

#[test]
fn a_root_singleton_first_resolved_in_a_scope_never_captures_its_scoped_instances() -> TestResult {
    let pages = Arc::new(AtomicUsize::new(0));
    let mut root = ServiceProvider::new();
    root.add_scoped::<Page>({
        let pages = Arc::clone(&pages);
        move |_| Page(pages.fetch_add(1, Ordering::SeqCst))
    });
    root.replace_service_factory::<Cache>(|services| Cache {
        page: services.required_service::<Page>().unwrap(),
    });

    let scope = root.create_scope_with(|_| {});
    let scope_page = scope.required_service::<Page>()?;
    let cache = scope.required_service::<Cache>()?;

    assert!(!Arc::ptr_eq(&cache.page, &scope_page));
    assert!(Arc::ptr_eq(&cache.page, &root.required_service::<Page>()?));
    Ok(())
}

#[test]
fn a_scopes_singleton_is_one_per_declaring_scope_shared_by_its_descendants() -> TestResult {
    let made = Arc::new(AtomicUsize::new(0));
    let root = ServiceProvider::new();
    let configure = |made: &Arc<AtomicUsize>| {
        let made = Arc::clone(made);
        move |scope: &mut ScopeRegistrations| {
            scope.add_singleton_factory::<Stamp>(move |_| {
                Stamp(made.fetch_add(1, Ordering::SeqCst) as u32)
            });
        }
    };
    let scope = root.create_scope_with(configure(&made));
    let other = root.create_scope_with(configure(&made));
    let child = scope.create_scope();
    let grandchild = child.create_scope();

    let own = scope.required_service::<Stamp>()?;
    assert!(Arc::ptr_eq(&own, &child.required_service::<Stamp>()?));
    assert!(Arc::ptr_eq(&own, &grandchild.required_service::<Stamp>()?));
    assert!(Arc::ptr_eq(
        &own,
        &scope.clone().required_service::<Stamp>()?
    ));
    assert!(!Arc::ptr_eq(&own, &other.required_service::<Stamp>()?));
    assert_eq!(made.load(Ordering::SeqCst), 2);
    assert!(root.service::<Stamp>().is_none(), "the root never sees it");
    Ok(())
}

#[test]
fn a_scopes_singleton_first_resolved_by_a_descendant_is_made_with_the_scope() -> TestResult {
    let mut root = ServiceProvider::new();
    root.add_singleton_as::<dyn Clock>(clock(1));
    let scope = root.create_scope_with(|scope| {
        scope
            .add_singleton_as::<dyn Clock>(clock(2))
            .add_singleton_factory::<Stamp>(stamp);
    });
    let descendant = scope.create_scope_with(|scope| {
        scope.add_singleton_as::<dyn Clock>(clock(3));
    });

    assert_eq!(descendant.required_service::<Stamp>()?.0, 2);
    Ok(())
}

#[test]
fn a_scopes_singleton_is_dropped_with_the_scope() -> TestResult {
    let drops = Arc::new(AtomicUsize::new(0));
    let root = ServiceProvider::new();
    let scope = root.create_scope_with({
        let drops = Arc::clone(&drops);
        move |scope| {
            scope.add_singleton_factory::<Counted>(move |_| Counted(Arc::clone(&drops)));
        }
    });
    let child = scope.create_scope();
    child.required_service::<Counted>()?;

    drop(scope);
    assert_eq!(drops.load(Ordering::SeqCst), 0, "its descendant keeps it");
    drop(child);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    Ok(())
}

#[test]
fn a_root_scoped_service_resolved_in_a_scope_sees_its_replacements() -> TestResult {
    let mut root = ServiceProvider::new();
    root.add_singleton_as::<dyn Clock>(clock(1));
    root.add_scoped::<Stamp>(stamp);
    root.add_transient::<Page>(|services| {
        Page(services.required_service::<dyn Clock>().unwrap().now() as usize)
    });

    let scope = root.create_scope_with(|scope| {
        scope.add_singleton_as::<dyn Clock>(clock(2));
    });

    assert_eq!(scope.required_service::<Stamp>()?.0, 2);
    assert_eq!(scope.create_scope().required_service::<Stamp>()?.0, 2);
    assert_eq!(scope.required_service::<Page>()?.0, 2);
    assert_eq!(root.required_service::<Stamp>()?.0, 1);
    assert_eq!(root.create_scope().required_service::<Stamp>()?.0, 1);
    assert_eq!(root.required_service::<Page>()?.0, 1);
    Ok(())
}

#[test]
fn a_scopes_scoped_and_transient_registrations_follow_their_lifetimes() -> TestResult {
    let pages = Arc::new(AtomicUsize::new(0));
    let root = ServiceProvider::new();
    let scope = root.create_scope_with({
        let pages = Arc::clone(&pages);
        move |scope| {
            scope
                .add_scoped::<Page>(move |_| Page(pages.fetch_add(1, Ordering::SeqCst)))
                .add_transient_as::<dyn Clock>(|_| clock(4));
        }
    });
    let child = scope.create_scope();

    let page = scope.required_service::<Page>()?;
    assert!(Arc::ptr_eq(&page, &scope.required_service::<Page>()?));
    assert!(!Arc::ptr_eq(&page, &child.required_service::<Page>()?));
    assert!(!Arc::ptr_eq(
        &scope.required_service::<dyn Clock>()?,
        &scope.required_service::<dyn Clock>()?
    ));
    assert_eq!(child.required_service::<dyn Clock>()?.now(), 4);
    assert!(root.service::<Page>().is_none());
    Ok(())
}

#[test]
fn a_scope_registers_by_runtime_key_and_thread_bound() -> TestResult {
    let key = RuntimeKey::new("greeting");
    let mut root = ServiceProvider::new();
    root.add_singleton_keyed(key.clone(), |_| {
        Ok(Arc::new("root".to_owned()) as ErasedService)
    });
    let local = RuntimeKey::new("local");

    let scope = root.create_scope_with(|scope| {
        scope
            .add_singleton_keyed(key.clone(), |_| {
                Ok(Arc::new("scope".to_owned()) as ErasedService)
            })
            .add_scoped_thread_bound_keyed(local.clone(), |_| {
                Ok(std::rc::Rc::new(std::cell::Cell::new(5_u8)) as LocalService)
            });
    });
    let text = |services: &ServiceProvider| {
        services
            .required_service_by_key(&key)
            .unwrap()
            .downcast::<String>()
            .unwrap()
    };

    assert_eq!(*text(&scope), "scope");
    assert_eq!(*text(&root), "root");
    let value = scope.required_thread_bound_service_by_key(&local)?;
    assert_eq!(value.downcast::<std::cell::Cell<u8>>().unwrap().get(), 5);
    assert!(
        scope.service_by_key(&local).is_none(),
        "thread-bound: not shared"
    );
    assert!(!root.contains_service_by_key(&local));
    Ok(())
}

#[test]
fn a_scopes_missing_dependency_fails_as_the_roots_does() {
    let key = RuntimeKey::new("needs-missing");
    let needs_missing = |services: &ServiceProvider| {
        services
            .required_service::<Missing>()
            .map(|missing| missing as ErasedService)
    };
    let mut root = ServiceProvider::new();
    root.add_scoped_keyed(key.clone(), needs_missing);
    let in_root = root.required_service_by_key(&key).unwrap_err();

    let scope = ServiceProvider::new().create_scope_with(|scope| {
        scope.add_scoped_keyed(key.clone(), needs_missing);
    });
    let in_scope = scope.required_service_by_key(&key).unwrap_err();

    assert_eq!(in_scope, in_root);
    assert_eq!(
        in_scope.to_string(),
        format!(
            "service `{}` is not registered",
            std::any::type_name::<Missing>()
        )
    );
}

#[test]
fn a_scope_with_no_registrations_behaves_as_a_plain_scope() -> TestResult {
    let mut root = ServiceProvider::new();
    root.add_singleton_as::<dyn Clock>(clock(1));
    root.add_scoped::<Stamp>(stamp);

    let scope = root.create_scope_with(|_| {});
    let plain = root.create_scope();

    assert!(Arc::ptr_eq(
        &scope.required_service::<dyn Clock>()?,
        &plain.required_service::<dyn Clock>()?
    ));
    assert!(!Arc::ptr_eq(
        &scope.required_service::<Stamp>()?,
        &plain.required_service::<Stamp>()?
    ));
    assert!(scope.required_service::<Missing>().is_err());
    Ok(())
}

#[test]
fn a_root_singleton_is_made_when_every_clone_of_the_root_is_gone() -> TestResult {
    let scope = {
        let mut root = ServiceProvider::new();
        root.add_singleton_as::<dyn Clock>(clock(1));
        root.replace_service_factory::<Stamp>(stamp);
        root.create_scope_with(|scope| {
            scope.add_singleton_as::<dyn Clock>(clock(2));
        })
    };

    assert_eq!(scope.required_service::<Stamp>()?.0, 1);
    Ok(())
}

#[test]
fn a_scopes_singleton_first_resolved_after_the_scope_is_gone_keeps_its_dependencies() -> TestResult
{
    let mut root = ServiceProvider::new();
    root.add_singleton_as::<dyn Clock>(clock(1));
    let scope = root.create_scope_with(|scope| {
        scope
            .add_singleton_as::<dyn Clock>(clock(2))
            .add_singleton_factory::<Stamp>(stamp);
    });
    let first = scope.create_scope_with(|scope| {
        scope.add_singleton_as::<dyn Clock>(clock(3));
    });
    let second = scope.create_scope();
    drop(scope);

    let stamp = first.required_service::<Stamp>()?;
    assert_eq!(
        stamp.0, 2,
        "made over the declaring scope's clock, not the descendant's"
    );
    assert!(Arc::ptr_eq(&stamp, &second.required_service::<Stamp>()?));
    Ok(())
}

#[test]
fn a_scopes_singleton_first_resolved_after_the_scope_is_gone_takes_no_childs_scoped_service()
-> TestResult {
    let pages = Arc::new(AtomicUsize::new(0));
    let mut root = ServiceProvider::new();
    root.add_scoped::<Page>({
        let pages = Arc::clone(&pages);
        move |_| Page(pages.fetch_add(1, Ordering::SeqCst))
    });
    let scope = root.create_scope_with(|scope| {
        scope.add_singleton_factory::<Cache>(|services| Cache {
            page: services.required_service::<Page>().unwrap(),
        });
    });
    let first = scope.create_scope();
    let second = scope.create_scope();
    drop(scope);

    let first_page = first.required_service::<Page>()?;
    let cache = second.required_service::<Cache>()?;
    let second_page = second.required_service::<Page>()?;

    assert!(!Arc::ptr_eq(&cache.page, &first_page));
    assert!(!Arc::ptr_eq(&cache.page, &second_page));
    assert!(Arc::ptr_eq(&cache, &first.required_service::<Cache>()?));
    assert!(Arc::ptr_eq(
        &cache.page,
        &first.required_service::<Cache>()?.page
    ));
    Ok(())
}
