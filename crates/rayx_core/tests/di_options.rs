//! Options and the configuration builder (ported from RayX's `options` and
//! `configuration` tests, without configuration sources).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use rayx_core::configuration::{ConfigurationError, OptionsFinalizeError, OptionsFinalizeStage};
use rayx_core::{ConfigurationBuilder, Options, OptionsError, OptionsRegistry, ServiceProvider};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[derive(Clone, Debug, Default, PartialEq)]
struct DatabaseOptions {
    connection_string: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct ServiceOptions {
    label: String,
    value: i64,
}

#[test]
fn di_options_wraps_shared_typed_value() {
    let options = Options::new(DatabaseOptions {
        connection_string: "postgres://localhost".to_string(),
    });
    let cloned = options.clone();

    assert_eq!(options.value(), cloned.value());
    assert_eq!(cloned.cloned().connection_string, "postgres://localhost");
}

#[test]
fn di_options_registry_resolves_by_type_and_reports_missing() {
    let mut registry = OptionsRegistry::default();
    registry.insert(DatabaseOptions {
        connection_string: "postgres://localhost".to_string(),
    });

    let options = registry.required::<DatabaseOptions>().unwrap();
    assert_eq!(options.value().connection_string, "postgres://localhost");
    assert!(registry.contains::<DatabaseOptions>());
    assert_eq!(
        registry
            .value::<DatabaseOptions>()
            .map(|value| value.connection_string.as_str()),
        Some("postgres://localhost")
    );

    let error = registry.required::<ServiceOptions>().unwrap_err();
    assert!(matches!(error, OptionsError::Missing { .. }));
    assert!(error.to_string().contains("ServiceOptions"));
}

#[test]
fn di_options_registry_extend_overrides_existing_options() {
    let mut base = OptionsRegistry::default();
    base.insert(DatabaseOptions {
        connection_string: "first".to_string(),
    });
    let mut next = OptionsRegistry::default();
    next.insert(DatabaseOptions {
        connection_string: "second".to_string(),
    });

    base.extend(next);

    assert_eq!(
        base.required::<DatabaseOptions>()
            .unwrap()
            .value()
            .connection_string,
        "second"
    );
}

#[test]
fn di_provider_carries_added_options_through_child_scopes() -> TestResult {
    let mut configuration = ConfigurationBuilder::default();
    configuration.options_value(ServiceOptions {
        label: "exact".to_string(),
        value: 44,
    });
    let provider =
        ServiceProvider::default().with_added_options(configuration.build_options_sync()?);
    let scoped = provider.create_scope();

    assert_eq!(
        provider.required_options::<ServiceOptions>()?.value().value,
        44
    );
    assert_eq!(
        scoped.required_options::<ServiceOptions>()?.value().label,
        "exact"
    );
    assert!(scoped.options::<DatabaseOptions>().is_none());
    assert!(provider.required_options::<DatabaseOptions>().is_err());

    let mut replacement = OptionsRegistry::default();
    replacement.insert(DatabaseOptions::default());
    let replaced = provider.with_options_registry(replacement);
    assert!(replaced.options::<ServiceOptions>().is_none());
    assert!(replaced.options_registry().contains::<DatabaseOptions>());
    Ok(())
}

#[test]
fn di_configuration_layers_configurators_and_finalizes_once() -> TestResult {
    let finalized = Arc::new(AtomicUsize::new(0));
    let mut builder = ConfigurationBuilder::default();
    let finalized_once = Arc::clone(&finalized);
    builder.configure_options(
        "service",
        || ServiceOptions {
            label: "base".to_string(),
            value: 1,
        },
        |options| options.value = 10,
        move |options| {
            finalized_once.fetch_add(1, Ordering::SeqCst);
            (options.value == 10)
                .then_some(())
                .ok_or_else(|| OptionsFinalizeError::validation("value was not configured"))
        },
    );
    builder.configure_options(
        "Service",
        ServiceOptions::default,
        |options| options.label = "final".to_string(),
        |_| Ok(()),
    );

    let options = builder.build_sync()?.options.required::<ServiceOptions>()?;
    assert_eq!(options.value().label, "final");
    assert_eq!(options.value().value, 10);
    assert_eq!(finalized.load(Ordering::SeqCst), 1);
    Ok(())
}

#[test]
fn di_configuration_default_options_and_async_build() -> TestResult {
    let mut builder = ConfigurationBuilder::default();
    builder.options::<ServiceOptions>("service");

    let options = futures::executor::block_on(builder.build_options_async())?;
    assert_eq!(
        options.required::<ServiceOptions>()?.value(),
        &ServiceOptions::default()
    );
    Ok(())
}

#[test]
fn di_configuration_exact_value_replaces_layered_plan_without_materializing_it() -> TestResult {
    let base_calls = Arc::new(AtomicUsize::new(0));
    let base_calls_in_plan = Arc::clone(&base_calls);
    let mut builder = ConfigurationBuilder::default();
    builder.configure_options(
        "service",
        move || {
            base_calls_in_plan.fetch_add(1, Ordering::SeqCst);
            ServiceOptions::default()
        },
        |_| {},
        |_| Ok(()),
    );
    builder.options_value(ServiceOptions {
        label: "exact".to_string(),
        value: 0,
    });
    builder.configure_options(
        "service",
        ServiceOptions::default,
        |options| options.label = "ignored".to_string(),
        |_| Ok(()),
    );

    let options = builder.build_options_sync()?;
    assert_eq!(base_calls.load(Ordering::SeqCst), 0);
    assert_eq!(options.required::<ServiceOptions>()?.value().label, "exact");
    Ok(())
}

#[test]
fn di_configuration_finalize_failure_publishes_no_partial_result() {
    let mut builder = ConfigurationBuilder::default();
    builder.options_value(DatabaseOptions::default());
    builder.configure_options(
        "service",
        ServiceOptions::default,
        |_| {},
        |_| {
            Err(OptionsFinalizeError::normalization(
                "invalid service options",
            ))
        },
    );

    let error = builder.build_options_sync().unwrap_err();
    assert!(matches!(
        error,
        ConfigurationError::OptionsFinalize {
            stage: OptionsFinalizeStage::Normalize,
            ..
        }
    ));
    let message = error.to_string();
    assert!(message.contains("ServiceOptions"));
    assert!(message.contains("service"));
    assert!(message.contains("normalize"));
    assert!(std::error::Error::source(&error).is_some());
}

#[test]
fn di_configuration_duplicate_option_type_with_different_section_fails() {
    let mut builder = ConfigurationBuilder::default();
    builder.configure_options("service", ServiceOptions::default, |_| {}, |_| Ok(()));
    builder.configure_options("other", ServiceOptions::default, |_| {}, |_| Ok(()));

    let error = builder.build_sync().unwrap_err();
    assert!(matches!(
        error,
        ConfigurationError::OptionsPlanConflict { .. }
    ));
    let message = error.to_string();
    assert!(message.contains("ServiceOptions"));
    assert!(message.contains("service"));
    assert!(message.contains("other"));
}
