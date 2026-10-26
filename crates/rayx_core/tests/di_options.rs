//! The .NET options model: `add_options::<T>()` with bind, configure, post-configure and
//! validate steps; named options; the app's options, snapshots and monitors; validation on
//! start.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use futures::executor::block_on;
use rayx_core::configuration::ConfigurationError;
use rayx_core::di::{StartupInitializationContext, StartupInitializationError};
use rayx_core::{
    Configuration, Options, OptionsError, RayXAppBuilder, ServiceProvider, ServiceResolutionError,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::{Arc, Mutex};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct FeedOptions {
    label: String,
    retry_limit: u32,
    hosts: Vec<String>,
}

impl Default for FeedOptions {
    fn default() -> Self {
        Self {
            label: "default".to_owned(),
            retry_limit: 1,
            hosts: vec!["default.host".to_owned()],
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct EndpointOptions {
    url: String,
    timeout_ms: u32,
    tags: Vec<&'static str>,
}

fn app(configuration: serde_json::Value) -> RayXAppBuilder {
    let mut app = RayXAppBuilder::new();
    app.configuration.add_value(configuration);
    app
}

#[test]
fn di_options_run_configure_steps_in_order_then_post_configure_then_validate() -> TestResult {
    let mut app = app(json!({
        "Feed": { "Label": "json", "retryLimit": 7, "hosts": ["a.host", "b.host"] }
    }));
    // A post-configure registered first still runs after every configure step.
    app.services
        .post_configure::<FeedOptions>(|options| options.retry_limit = options.retry_limit.min(5));
    app.services
        .add_options::<FeedOptions>()
        .configure(|options| options.label = "before".to_owned())
        .bind("Feed")
        .configure(|options| options.label.push_str("+after"))
        .validate(|options| options.retry_limit <= 5, "retry limit above 5");
    let services = app.build()?;

    let options = services.options::<FeedOptions>()?;
    assert_eq!(options.label, "json+after");
    assert_eq!(options.retry_limit, 5);
    // Binding replaces a list rather than appending to the default.
    assert_eq!(options.hosts, ["a.host", "b.host"]);
    Ok(())
}

#[test]
fn di_options_bind_keeps_what_the_section_does_not_name() -> TestResult {
    let mut app = app(json!({ "Feed": { "label": "json", "retry_limit": null } }));
    app.services.add_options::<FeedOptions>().bind("feed");
    let options = app.build()?.options::<FeedOptions>()?;

    assert_eq!(
        options.cloned(),
        FeedOptions {
            label: "json".to_owned(),
            ..FeedOptions::default()
        }
    );
    Ok(())
}

#[test]
fn di_options_are_named_and_configure_all_reaches_every_name() -> TestResult {
    let mut services = ServiceProvider::new();
    services
        .add_named_options::<EndpointOptions>("primary")
        .configure(|options| options.url = "https://primary".to_owned());
    services
        .configure_named::<EndpointOptions>("secondary", |options| {
            options.url = "https://secondary".to_owned()
        })
        .configure_all::<EndpointOptions>(|options| options.timeout_ms = 500)
        .post_configure_all::<EndpointOptions>(|options| options.tags.push("post"))
        .post_configure_named::<EndpointOptions>("primary", |options| options.tags.push("primary"));

    let monitor = services.options_monitor::<EndpointOptions>();
    let primary = monitor.get("primary")?;
    let secondary = monitor.get("secondary")?;
    let unnamed = monitor.current_value()?;
    assert_eq!(
        (
            primary.url.as_str(),
            primary.timeout_ms,
            primary.tags.clone()
        ),
        ("https://primary", 500, vec!["post", "primary"])
    );
    assert_eq!(
        (secondary.url.as_str(), secondary.tags.clone()),
        ("https://secondary", vec!["post"])
    );
    assert_eq!((unnamed.url.as_str(), unnamed.timeout_ms), ("", 500));
    assert_eq!(services.options::<EndpointOptions>()?.timeout_ms, 500);
    Ok(())
}

#[test]
fn di_options_validation_reports_every_failure() {
    let mut services = ServiceProvider::new();
    services
        .add_options::<EndpointOptions>()
        .validate(|options| !options.url.is_empty(), "the URL is required")
        .validate(
            |options| options.timeout_ms > 0,
            "the timeout must be positive",
        )
        .validate(|_| true, "never shown");

    let error = services.options::<EndpointOptions>().unwrap_err();
    assert_eq!(
        error,
        OptionsError::Validation {
            type_name: std::any::type_name::<EndpointOptions>(),
            name: String::new(),
            failures: vec![
                "the URL is required".to_owned(),
                "the timeout must be positive".to_owned()
            ],
        }
    );
    let message = error.to_string();
    assert!(message.contains("EndpointOptions"), "{message}");
    assert!(
        message.contains("the URL is required; the timeout must be positive"),
        "{message}"
    );
}

#[test]
fn di_options_without_registrations_are_the_default() -> TestResult {
    let services = ServiceProvider::new();
    assert_eq!(
        services.options::<FeedOptions>()?.cloned(),
        FeedOptions::default()
    );
    assert_eq!(
        services
            .options_snapshot::<FeedOptions>()
            .get("any")?
            .cloned(),
        FeedOptions::default()
    );
    assert_eq!(
        services
            .options_monitor::<FeedOptions>()
            .current_value()?
            .label,
        "default"
    );
    Ok(())
}

#[test]
fn di_options_app_options_stay_snapshots_follow_scopes_and_monitors_follow_changes() -> TestResult {
    let mut app = app(json!({ "Feed": { "label": "first" } }));
    app.services.add_options::<FeedOptions>().bind("Feed");
    let services = app.build()?;
    let configuration = services.required_service::<Configuration>()?;

    let app_options = services.options::<FeedOptions>()?;
    assert!(Options::ptr_eq(
        &app_options,
        &services.options::<FeedOptions>()?
    ));
    let read_scope = services.create_scope();
    let read_snapshot = read_scope.options_snapshot::<FeedOptions>().value()?;
    assert!(Options::ptr_eq(
        &read_snapshot,
        &read_scope.options_snapshot::<FeedOptions>().value()?
    ));

    let monitor = services.options_monitor::<FeedOptions>();
    assert_eq!(monitor.current_value()?.label, "first");
    let changes = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&changes);
    let subscription = monitor.on_change(move |options, name| {
        seen.lock()
            .unwrap()
            .push((name.to_owned(), options.label.clone()));
    });

    configuration.set("Feed:Label", "second");

    assert_eq!(
        *changes.lock().unwrap(),
        [(String::new(), "second".to_owned())]
    );
    assert_eq!(monitor.current_value()?.label, "second");
    assert_eq!(services.options::<FeedOptions>()?.label, "first");
    assert_eq!(
        read_scope.options_snapshot::<FeedOptions>().value()?.label,
        "first"
    );
    let new_scope = services.create_scope();
    assert_eq!(
        new_scope.options_snapshot::<FeedOptions>().value()?.label,
        "second"
    );

    drop(subscription);
    configuration.set("Feed:Label", "third");
    assert_eq!(changes.lock().unwrap().len(), 1);
    assert_eq!(monitor.current_value()?.label, "third");
    Ok(())
}

#[test]
fn di_options_monitor_keeps_the_error_of_options_that_fail_after_a_change() -> TestResult {
    let mut app = app(json!({ "Feed": { "retryLimit": 2 } }));
    app.services
        .add_options::<FeedOptions>()
        .bind("Feed")
        .validate(|options| options.retry_limit < 10, "retry limit too high");
    let services = app.build()?;
    let monitor = services.options_monitor::<FeedOptions>();
    let calls = Arc::new(Mutex::new(0));
    let counted = Arc::clone(&calls);
    monitor
        .on_change(move |_, _| *counted.lock().unwrap() += 1)
        .detach();

    services
        .required_service::<Configuration>()?
        .set("Feed:retryLimit", 20);

    assert_eq!(*calls.lock().unwrap(), 0);
    assert!(matches!(
        monitor.current_value(),
        Err(OptionsError::Validation { .. })
    ));
    Ok(())
}

#[test]
fn di_options_steps_take_services() -> TestResult {
    struct Region(&'static str);
    let mut services = ServiceProvider::new().with_singleton(Region("eu"));
    services
        .add_options::<EndpointOptions>()
        .configure_with::<Region>(|options, region| {
            options.url = format!("https://{}.example", region.0)
        })
        .post_configure_with::<Region>(|options, region| options.tags.push(region.0))
        .validate_with::<Region>(
            |options, region| options.url.contains(region.0),
            "wrong region",
        );

    let options = services.options::<EndpointOptions>()?;
    assert_eq!(options.url, "https://eu.example");
    assert_eq!(options.tags, ["eu"]);

    let mut missing = ServiceProvider::new();
    missing
        .add_options::<EndpointOptions>()
        .configure_with::<Region>(|_, _| {});
    let error = missing.options::<EndpointOptions>().unwrap_err();
    assert!(matches!(
        &error,
        OptionsError::Dependency {
            error: ServiceResolutionError::Missing { service },
            ..
        } if service.contains("Region")
    ));
    Ok(())
}

#[test]
fn di_options_bind_needs_the_configuration_and_a_section_that_converts() -> TestResult {
    let mut services = ServiceProvider::new();
    services.add_options::<FeedOptions>().bind("Feed");
    let error = services.options::<FeedOptions>().unwrap_err();
    assert!(error.to_string().contains("Configuration"), "{error}");

    let mut app = app(json!({ "Feed": { "retryLimit": "many" } }));
    app.services.add_options::<FeedOptions>().bind("Feed");
    let error = app.build()?.options::<FeedOptions>().unwrap_err();
    assert!(matches!(
        &error,
        OptionsError::Bind {
            error: ConfigurationError::Bind { path, .. },
            ..
        } if path == "Feed"
    ));
    Ok(())
}

#[test]
fn di_options_validated_on_start_fail_startup_before_any_service() -> TestResult {
    let mut app = app(json!({ "Endpoint": { "url": "" } }));
    app.services
        .add_named_options::<EndpointOptions>("api")
        .validate(|options| !options.url.is_empty(), "the API URL is required")
        .validate_on_start();
    app.services
        .add_options::<FeedOptions>()
        .bind("Feed")
        .validate_on_start()
        .validate_on_start();
    let services = app.build()?;

    let error = block_on(
        services
            .startup_registry()
            .initialize(StartupInitializationContext::new(services.clone())),
    )
    .unwrap_err();
    let StartupInitializationError::Options { failures } = &error else {
        panic!("expected an options failure, got {error}");
    };
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].name(), "api");
    assert!(
        error.to_string().contains("the API URL is required"),
        "{error}"
    );
    assert!(services.validate_options_on_start().is_err());
    Ok(())
}
