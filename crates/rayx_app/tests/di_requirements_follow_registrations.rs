//! A registration's requirements (`require_for::<S, T>`) belong to the registration of `S` that
//! declared them: a later registration of `S` drops them unless it declares them again, while
//! what the app or its host requires (`require::<T>`) stays.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use rayx_app::prelude::*;

trait Assets: Send + Sync {}
trait Translator: Send + Sync {}
trait Platform: Send + Sync {}

struct RayxTranslator;
impl Translator for RayxTranslator {}

struct OwnTranslator;
impl Translator for OwnTranslator {}

struct Bundled;
impl Assets for Bundled {}

/// What a service crate's `add_translator` does: declares what its implementation resolves,
/// then registers it.
fn add_translator(services: &mut ServiceProvider) {
    services.require_for::<dyn Translator, dyn Assets>("add_assets()");
    services.replace_service_factory_as::<dyn Translator>(|_| Arc::new(RayxTranslator));
}

fn missing(services: &ServiceProvider) -> Vec<String> {
    match services.validate() {
        Ok(()) => Vec::new(),
        Err(error) => error
            .problems
            .iter()
            .map(|problem| match problem {
                ServiceValidationProblem::Missing { needed, .. } => needed.clone(),
                other => other.to_string(),
            })
            .collect(),
    }
}

fn names_assets(missing: &[String]) -> bool {
    missing.iter().any(|needed| needed.contains("Assets"))
}

#[test]
fn di_requirements_follow_registrations_a_crate_s_registration_requires_what_it_declared() {
    let mut services = ServiceProvider::new();
    add_translator(&mut services);
    assert!(names_assets(&missing(&services)));
}

#[test]
fn di_requirements_follow_registrations_the_app_s_own_registration_drops_them() {
    for register in [
        |services: &mut ServiceProvider| {
            services
                .add_singleton_as::<dyn Translator>(Arc::new(OwnTranslator) as Arc<dyn Translator>);
        },
        |services: &mut ServiceProvider| {
            services.add_scoped_as::<dyn Translator>(|_| Arc::new(OwnTranslator));
        },
        |services: &mut ServiceProvider| {
            services
                .replace_service::<dyn Translator>(Arc::new(OwnTranslator) as Arc<dyn Translator>);
        },
    ] {
        let mut services = ServiceProvider::new();
        add_translator(&mut services);
        register(&mut services);
        assert_eq!(missing(&services), Vec::<String>::new());
    }
}

#[test]
fn di_requirements_follow_registrations_a_registration_declaring_them_again_keeps_them() {
    let mut services = ServiceProvider::new();
    services.add_singleton_as::<dyn Translator>(Arc::new(OwnTranslator) as Arc<dyn Translator>);
    add_translator(&mut services);
    assert!(
        names_assets(&missing(&services)),
        "the crate's registration, last, requires what it declared"
    );

    let mut twice = ServiceProvider::new();
    add_translator(&mut twice);
    add_translator(&mut twice);
    twice.add_singleton_as::<dyn Assets>(Arc::new(Bundled) as Arc<dyn Assets>);
    assert_eq!(missing(&twice), Vec::<String>::new());
}

#[test]
fn di_requirements_follow_registrations_a_declaration_with_no_registration_after_it_stays() {
    let mut services = ServiceProvider::new();
    services.add_singleton_as::<dyn Translator>(Arc::new(OwnTranslator) as Arc<dyn Translator>);
    services.require_for::<dyn Translator, dyn Assets>("add_assets()");
    assert!(
        names_assets(&missing(&services)),
        "declared after the registration it belongs to: the current one"
    );
}

#[test]
fn di_requirements_follow_registrations_other_keys_and_the_app_s_requirements_stay() {
    let mut services = ServiceProvider::new();
    services.require::<dyn Platform>("add_platform()");
    add_translator(&mut services);
    services.require_for::<dyn Assets, dyn Platform>("add_platform()");
    services.add_singleton_as::<dyn Translator>(Arc::new(OwnTranslator) as Arc<dyn Translator>);
    services.add_singleton_as::<dyn Assets>(Arc::new(Bundled) as Arc<dyn Assets>);

    let missing = missing(&services);
    assert_eq!(missing.len(), 2, "{missing:?}");
    assert!(missing.iter().all(|needed| needed.contains("Platform")));
}
