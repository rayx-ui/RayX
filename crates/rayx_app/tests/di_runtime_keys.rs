//! Services registered under runtime keys (`RuntimeKey`): lifetimes, scopes, factories, missing
//! keys and key identity, alongside typed services.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use rayx_app::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Weak};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// A runtime-keyed value: a numbered instance.
#[derive(Debug)]
struct Made {
    id: usize,
    label: String,
}

fn made(label: &str) -> ErasedService {
    static NEXT: AtomicUsize = AtomicUsize::new(1);
    Arc::new(Made {
        id: NEXT.fetch_add(1, Ordering::SeqCst),
        label: label.to_owned(),
    })
}

fn as_made(service: ErasedService) -> Arc<Made> {
    service.downcast::<Made>().expect("a runtime-keyed `Made`")
}

#[derive(Debug, AppService)]
struct Typed(u32);

#[test]
fn di_runtime_key_singleton_is_one_value_per_root_shared_by_scopes() -> TestResult {
    let key = RuntimeKey::new("Settings");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut provider = ServiceProvider::new();
    provider.add_singleton_keyed(key.clone(), {
        let calls = Arc::clone(&calls);
        move |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(made("settings"))
        }
    });

    let first = as_made(provider.required_service_by_key(&key)?);
    let again = as_made(provider.required_service_by_key(&key)?);
    let scope = provider.create_scope();
    let nested = scope.create_scope();
    let from_scope = as_made(scope.required_service_by_key(&key)?);
    let from_nested = as_made(nested.required_service_by_key(&key)?);

    assert_eq!(first.label, "settings");
    assert!(Arc::ptr_eq(&first, &again));
    assert!(Arc::ptr_eq(&first, &from_scope));
    assert!(Arc::ptr_eq(&first, &from_nested));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    Ok(())
}

#[test]
fn di_runtime_key_singleton_resolved_first_in_a_scope_is_shared_with_the_root() -> TestResult {
    let key = RuntimeKey::new("Settings");
    let mut provider = ServiceProvider::new();
    provider.add_singleton_keyed(key.clone(), |_| Ok(made("settings")));

    let scope = provider.create_scope();
    let from_scope = as_made(scope.required_service_by_key(&key)?);
    let from_root = as_made(provider.required_service_by_key(&key)?);

    assert!(Arc::ptr_eq(&from_scope, &from_root));
    Ok(())
}

#[test]
fn di_runtime_key_scoped_is_one_value_per_scope() -> TestResult {
    let key = RuntimeKey::new("Session");
    let mut provider = ServiceProvider::new();
    provider.add_scoped_keyed(key.clone(), |_| Ok(made("session")));

    let root_value = as_made(provider.required_service_by_key(&key)?);
    let root_again = as_made(provider.required_service_by_key(&key)?);
    let first_scope = provider.create_scope();
    let second_scope = provider.create_scope();
    let first = as_made(first_scope.required_service_by_key(&key)?);
    let first_again = as_made(first_scope.clone().required_service_by_key(&key)?);
    let second = as_made(second_scope.required_service_by_key(&key)?);

    assert!(Arc::ptr_eq(&root_value, &root_again));
    assert!(Arc::ptr_eq(&first, &first_again));
    assert!(!Arc::ptr_eq(&first, &second));
    assert!(!Arc::ptr_eq(&root_value, &first));
    assert_ne!(first.id, second.id);
    Ok(())
}

#[test]
fn di_runtime_key_scoped_value_is_dropped_with_its_scope() -> TestResult {
    let key = RuntimeKey::new("Session");
    let mut provider = ServiceProvider::new();
    provider.add_scoped_keyed(key.clone(), |_| Ok(made("session")));

    let scope = provider.create_scope();
    let value: Weak<Made> = Arc::downgrade(&as_made(scope.required_service_by_key(&key)?));
    assert!(value.upgrade().is_some());
    drop(scope);

    assert!(value.upgrade().is_none());
    Ok(())
}

#[test]
fn di_runtime_key_singleton_is_dropped_with_its_last_provider() -> TestResult {
    let key = RuntimeKey::new("Settings");
    let mut provider = ServiceProvider::new();
    provider.add_singleton_keyed(key.clone(), |_| Ok(made("settings")));
    let scope = provider.create_scope();
    let value: Weak<Made> = Arc::downgrade(&as_made(scope.required_service_by_key(&key)?));

    drop(provider);
    assert!(
        value.upgrade().is_some(),
        "the scope still shares the singleton"
    );
    drop(scope);
    assert!(value.upgrade().is_none());
    Ok(())
}

#[test]
fn di_runtime_key_transient_is_new_on_every_resolve() -> TestResult {
    let key = RuntimeKey::new("Request");
    let mut provider = ServiceProvider::new();
    provider.add_transient_keyed(key.clone(), |_| Ok(made("request")));

    let first = as_made(provider.required_service_by_key(&key)?);
    let second = as_made(provider.required_service_by_key(&key)?);
    let from_scope = as_made(provider.create_scope().required_service_by_key(&key)?);

    assert!(!Arc::ptr_eq(&first, &second));
    assert!(!Arc::ptr_eq(&first, &from_scope));
    Ok(())
}

#[test]
fn di_runtime_key_factory_receives_the_resolving_provider() -> TestResult {
    #[derive(Debug, AppService)]
    struct ScopeMarker(usize);

    let base = RuntimeKey::new("Base");
    let composed = RuntimeKey::new("Composed");
    let mut provider = ServiceProvider::new();
    provider.add_singleton(Typed(7));
    provider.add_scoped::<ScopeMarker>({
        let next = Arc::new(AtomicUsize::new(0));
        move |_| ScopeMarker(next.fetch_add(1, Ordering::SeqCst))
    });
    provider.add_singleton_keyed(base.clone(), |_| Ok(made("base")));
    provider.add_scoped_keyed(composed.clone(), {
        let base = base.clone();
        move |services| {
            let typed = services.required_service::<Typed>()?;
            let marker = services.required_service::<ScopeMarker>()?;
            let base = as_made(services.required_service_by_key(&base)?);
            let value: ErasedService = Arc::new((typed.0, marker.0, base.label.clone()));
            Ok(value)
        }
    });

    let scope = provider.create_scope();
    let marker = scope.required_service::<ScopeMarker>()?;
    let value = scope
        .required_service_by_key(&composed)?
        .downcast::<(u32, usize, String)>()
        .expect("a tuple");

    assert_eq!(*value, (7, marker.0, "base".to_owned()));
    Ok(())
}

#[test]
fn di_runtime_key_missing_is_none_or_an_error_naming_the_key() {
    let key = RuntimeKey::new("Unregistered.Service");
    let provider = ServiceProvider::new();
    let scope = provider.create_scope();

    assert!(provider.service_by_key(&key).is_none());
    assert!(!provider.contains_service_by_key(&key));
    let error = scope
        .required_service_by_key(&key)
        .expect_err("an unregistered key must fail");

    assert_eq!(
        error,
        ServiceResolutionError::MissingKey {
            service: "Unregistered.Service".into()
        }
    );
    assert!(error.to_string().contains("Unregistered.Service"));
    assert!(error.to_string().contains("not registered"));
}

#[test]
fn di_runtime_key_factory_error_is_returned_and_not_cached() -> TestResult {
    let key = RuntimeKey::new("Flaky");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut provider = ServiceProvider::new();
    provider.add_singleton_keyed(key.clone(), {
        let calls = Arc::clone(&calls);
        move |_| {
            if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                Err(ServiceResolutionError::failed("Flaky", "not yet"))
            } else {
                Ok(made("flaky"))
            }
        }
    });

    let error = provider
        .required_service_by_key(&key)
        .expect_err("the first make fails");
    assert!(error.to_string().contains("Flaky"));
    assert!(error.to_string().contains("not yet"));
    let value = as_made(provider.required_service_by_key(&key)?);
    let again = as_made(provider.required_service_by_key(&key)?);

    assert!(Arc::ptr_eq(&value, &again));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    Ok(())
}

#[test]
fn di_runtime_keys_of_the_same_name_stay_distinct() -> TestResult {
    let first = RuntimeKey::new("Same");
    let second = RuntimeKey::new("Same");
    let mut provider = ServiceProvider::new();
    provider.add_singleton_keyed(first.clone(), |_| Ok(made("first")));

    assert_ne!(first, second);
    assert_eq!(first, first.clone());
    assert_eq!(first.name(), second.name());
    assert_ne!(ServiceKey::from(&first), ServiceKey::from(&second));
    assert!(provider.service_by_key(&second).is_none());

    provider.add_singleton_keyed(second.clone(), |_| Ok(made("second")));
    assert_eq!(
        as_made(provider.required_service_by_key(&first)?).label,
        "first"
    );
    assert_eq!(
        as_made(provider.required_service_by_key(&second)?).label,
        "second"
    );
    Ok(())
}

#[test]
fn di_runtime_keys_and_typed_services_live_side_by_side() -> TestResult {
    let key = RuntimeKey::new("Typed");
    let mut provider = ServiceProvider::new();
    provider.add_singleton(Typed(1));
    provider.add_singleton_keyed(key.clone(), |_| Ok(made("keyed")));

    assert_ne!(ServiceKey::from(&key), ServiceKey::of::<Typed>());
    assert_eq!(provider.required_service::<Typed>()?.0, 1);
    assert_eq!(
        as_made(provider.required_service_by_key(&key)?).label,
        "keyed"
    );
    assert!(provider.contains_service::<Typed>());
    assert!(provider.contains_service_by_key(&key));

    let error = provider
        .required_service::<Made>()
        .expect_err("a runtime key does not register a type");
    assert_eq!(
        error,
        ServiceResolutionError::Missing {
            service: std::any::type_name::<Made>()
        }
    );
    Ok(())
}

#[test]
fn di_runtime_key_registration_replaces_the_earlier_one() -> TestResult {
    let key = RuntimeKey::new("Replaced");
    let mut provider = ServiceProvider::new();
    provider.add_singleton_keyed(key.clone(), |_| Ok(made("old")));
    assert_eq!(
        as_made(provider.required_service_by_key(&key)?).label,
        "old"
    );

    provider.add_transient_keyed(key.clone(), |_| Ok(made("new")));
    let first = as_made(provider.required_service_by_key(&key)?);
    let second = as_made(provider.required_service_by_key(&key)?);

    assert_eq!(first.label, "new");
    assert!(!Arc::ptr_eq(&first, &second));
    Ok(())
}
