//! The `AppBuilder` extension pattern GameX's service crates use
//! (`with_sportsbook`, `with_sportsbook_slips`), with only `use` paths changed.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use rayx_core::prelude::{AppService, ConfigurationBuilder, ServiceProvider};
use rayx_core::services::app::{AppBuilder, ConfigurationAppBuilderExt, ServiceAppBuilderExt};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

// --- abstractions crate ----------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq)]
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
    fn with_feed<F>(mut self, configure_options: F) -> Self
    where
        F: FnOnce(&mut FeedOptions),
    {
        let mut options = FeedOptions::default();
        configure_options(&mut options);
        self.configuration_mut().options_value(options.clone());

        let feed = Arc::new(FeedService {
            label: options.label.clone(),
        });
        self.services_mut()
            .add_singleton::<FeedService>(Arc::clone(&feed));
        let feed_trait: Arc<dyn Feed> = feed;
        self = self.add_singleton_as::<dyn Feed>(feed_trait);

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

#[derive(Default)]
struct TestBuilder {
    configuration: ConfigurationBuilder,
    services: ServiceProvider,
}

impl AppBuilder for TestBuilder {
    fn configuration_mut(&mut self) -> &mut ConfigurationBuilder {
        &mut self.configuration
    }

    fn services_mut(&mut self) -> &mut ServiceProvider {
        &mut self.services
    }
}

#[test]
fn di_app_builder_extension_registers_options_and_trait_objects()
-> Result<(), Box<dyn std::error::Error>> {
    let mut builder = TestBuilder::default().with_feed(|options| {
        options.initial_now_ms = Some(1_000);
        options.label = "live".to_string();
    });
    let options = builder.configuration.build_options_sync()?;
    builder.services = builder.services.with_added_options(options);

    let options = builder.services.required_options::<FeedOptions>()?;
    assert_eq!(options.value().initial_now_ms, Some(1_000));
    assert_eq!(options.value().label, "live");

    let feed = builder.services.required_service::<dyn Feed>()?;
    assert_eq!(feed.label(), "live");
    let concrete = builder.services.required_service::<FeedService>()?;
    assert_eq!(
        Arc::as_ptr(&concrete).cast::<()>(),
        Arc::as_ptr(&feed).cast::<()>()
    );

    let scoped = builder.services.create_scope();
    let nav = scoped.required_service::<dyn NavViewModel>()?;
    assert_eq!(nav.feed_label(), "live");
    assert_eq!(
        scoped.required_options::<FeedOptions>()?.value().label,
        "live"
    );

    assert!(builder.services.startup_registry().is_empty());
    Ok(())
}

#[test]
fn di_app_builder_scoped_view_model_identity() -> Result<(), Box<dyn std::error::Error>> {
    let builder = TestBuilder::default().with_feed(|options| options.label = "x".to_string());
    let scoped = builder.services.create_scope();

    let concrete_nav = scoped.required_service::<SideNavViewModel>()?;
    let trait_nav = scoped.required_service::<dyn NavViewModel>()?;
    assert_eq!(
        Arc::as_ptr(&concrete_nav).cast::<()>(),
        Arc::as_ptr(&trait_nav).cast::<()>(),
        "concrete and trait nav resolution must share one scoped ViewModel"
    );

    let other = builder.services.create_scope();
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
fn di_app_builder_configure_and_transient_extensions() -> Result<(), Box<dyn std::error::Error>> {
    let builder = TestBuilder::default()
        .configure(|configuration| {
            configuration.options_value(FeedOptions {
                initial_now_ms: None,
                label: "configured".to_string(),
            });
        })
        .add_singleton(FeedService {
            label: "singleton".to_string(),
        })
        .add_transient_as::<dyn Feed>(|_| {
            Arc::new(FeedService {
                label: "transient".to_string(),
            }) as Arc<dyn Feed>
        });
    let services = builder
        .services
        .clone()
        .with_added_options(builder.configuration.build_options_sync()?);

    assert_eq!(
        services.required_options::<FeedOptions>()?.value().label,
        "configured"
    );
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
fn di_app_builder_missing_options_and_services_are_errors() {
    let builder = TestBuilder::default();
    let options_error = builder
        .services
        .required_options::<FeedOptions>()
        .unwrap_err();
    assert!(options_error.to_string().contains("FeedOptions"));
    let service_error = builder
        .services
        .create_scope()
        .required_service::<dyn NavViewModel>()
        .err()
        .expect("unregistered ViewModel must fail");
    assert!(service_error.to_string().contains("NavViewModel"));
}
