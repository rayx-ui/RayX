//! The `AppBuilder` extension pattern service crates use (`with_sportsbook`,
//! `with_sportsbook_slips`) on `RayXAppBuilder`: options registered with the options model,
//! services reading them when they are made.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use rayx_core::prelude::{AppService, RayXAppBuilder, ServiceProvider};
use rayx_core::services::app::{AppBuilder, ConfigurationAppBuilderExt, ServiceAppBuilderExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

// --- abstractions crate ----------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct FeedOptions {
    initial_now_ms: Option<i64>,
    label: String,
}

trait Feed: Send + Sync + 'static {
    fn label(&self) -> String;
}

impl rayx_core::di::AppService for dyn Feed {}

trait NavViewModel: Send + Sync + 'static {
    fn id(&self) -> u32;
    fn feed_label(&self) -> String;
}

impl rayx_core::di::AppService for dyn NavViewModel {}

// --- services crate ----------------------------------------------------------

#[derive(AppService)]
struct FeedService {
    label: String,
}

impl Feed for FeedService {
    fn label(&self) -> String {
        self.label.clone()
    }
}

#[derive(AppService)]
struct SideNavViewModel {
    id: u32,
    feed: Arc<FeedService>,
}

impl SideNavViewModel {
    fn from_services(services: &ServiceProvider) -> Self {
        static NEXT_ID: AtomicU32 = AtomicU32::new(1);
        let feed = services
            .required_service::<FeedService>()
            .expect("the feed service is registered");
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::SeqCst),
            feed,
        }
    }
}

impl NavViewModel for SideNavViewModel {
    fn id(&self) -> u32 {
        self.id
    }

    fn feed_label(&self) -> String {
        self.feed.label()
    }
}

trait FeedAppBuilderExt: AppBuilder {
    /// The feed's options come from the `Feed` section, then `configure_options`.
    fn with_feed(
        mut self,
        configure_options: impl Fn(&mut FeedOptions) + Send + Sync + 'static,
    ) -> Self {
        self = self.add_options::<FeedOptions>(|options| {
            options
                .bind("Feed")
                .configure(configure_options)
                .validate(
                    |options| !options.label.is_empty(),
                    "the feed needs a label",
                )
                .validate_on_start();
        });
        let services = self.services_mut();
        services.try_add_service_factory::<FeedService>(|services| FeedService {
            label: services
                .options::<FeedOptions>()
                .expect("the feed options are valid")
                .label
                .clone(),
        });
        services.try_add_service_factory_as::<dyn Feed>(|services| {
            services
                .required_service::<FeedService>()
                .expect("the feed service is registered")
        });

        self.add_scoped::<SideNavViewModel>(SideNavViewModel::from_services)
            .add_scoped_as::<dyn NavViewModel>(|services| {
                services
                    .required_service::<SideNavViewModel>()
                    .expect("concrete nav ViewModel is registered")
            })
    }
}

impl<T> FeedAppBuilderExt for T where T: AppBuilder {}

// --- tests -------------------------------------------------------------------

#[test]
fn di_app_builder_extension_registers_options_and_trait_objects()
-> Result<(), Box<dyn std::error::Error>> {
    let services = RayXAppBuilder::new()
        .configure_app_configuration(|configuration| {
            configuration.add_value(json!({ "Feed": { "label": "configured" } }));
        })
        .with_feed(|options| {
            options.initial_now_ms = Some(1_000);
            options.label.push_str("+live");
        })
        .build()?;
    assert_eq!(services.validate_options_on_start(), Ok(()));

    let options = services.options::<FeedOptions>()?;
    assert_eq!(options.initial_now_ms, Some(1_000));
    assert_eq!(options.label, "configured+live");

    let feed = services.required_service::<dyn Feed>()?;
    assert_eq!(feed.label(), "configured+live");
    let concrete = services.required_service::<FeedService>()?;
    assert_eq!(
        Arc::as_ptr(&concrete).cast::<()>(),
        Arc::as_ptr(&feed).cast::<()>()
    );

    let scoped = services.create_scope();
    let nav = scoped.required_service::<dyn NavViewModel>()?;
    assert_eq!(nav.feed_label(), "configured+live");
    assert_eq!(scoped.options::<FeedOptions>()?.label, "configured+live");

    assert!(services.startup_registry().is_empty());
    Ok(())
}

#[test]
fn di_app_builder_scoped_view_model_identity() -> Result<(), Box<dyn std::error::Error>> {
    let services = RayXAppBuilder::new()
        .with_feed(|options| options.label = "x".to_string())
        .build()?;
    let scoped = services.create_scope();

    let concrete_nav = scoped.required_service::<SideNavViewModel>()?;
    let trait_nav = scoped.required_service::<dyn NavViewModel>()?;
    assert_eq!(
        Arc::as_ptr(&concrete_nav).cast::<()>(),
        Arc::as_ptr(&trait_nav).cast::<()>(),
        "concrete and trait nav resolution must share one scoped ViewModel"
    );

    let other = services.create_scope();
    assert_ne!(
        other.required_service::<dyn NavViewModel>()?.id(),
        trait_nav.id(),
        "every scope gets its own ViewModel"
    );
    assert!(Arc::ptr_eq(
        &other.required_service::<FeedService>()?,
        &concrete_nav.feed
    ));
    Ok(())
}

#[test]
fn di_app_builder_configure_options_and_transient_extensions()
-> Result<(), Box<dyn std::error::Error>> {
    let services = RayXAppBuilder::new()
        .configure_options::<FeedOptions>(|options| options.label = "configured".to_string())
        .add_singleton(FeedService {
            label: "singleton".to_string(),
        })
        .add_transient_as::<dyn Feed>(|_| {
            Arc::new(FeedService {
                label: "transient".to_string(),
            }) as Arc<dyn Feed>
        })
        .build()?;

    assert_eq!(services.options::<FeedOptions>()?.label, "configured");
    assert_eq!(
        services.required_service::<FeedService>()?.label,
        "singleton"
    );
    let first = services.required_service::<dyn Feed>()?;
    let second = services.required_service::<dyn Feed>()?;
    assert!(!Arc::ptr_eq(&first, &second));
    assert_eq!(first.label(), "transient");
    Ok(())
}

#[test]
fn di_app_builder_invalid_options_and_missing_services_are_errors() {
    let services = RayXAppBuilder::new()
        .with_feed(|_| {})
        .build()
        .expect("the configuration builds");
    let failures = services.validate_options_on_start().unwrap_err();
    assert!(failures[0].to_string().contains("the feed needs a label"));

    let service_error = RayXAppBuilder::new()
        .build()
        .expect("the configuration builds")
        .create_scope()
        .required_service::<dyn NavViewModel>()
        .err()
        .expect("unregistered ViewModel must fail");
    assert!(service_error.to_string().contains("NavViewModel"));
}
