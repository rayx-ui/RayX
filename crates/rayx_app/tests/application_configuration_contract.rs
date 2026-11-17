//! The application configuration contract in the core, NSH-free: the layout direction, the
//! effective motion and the programmatic scroll behaviour are functions of the published
//! `ApplicationConfigurationSnapshot`, so they answer the same over any implementation
//! (`rayx_app::testing::FakeApplicationConfiguration` here, registered as
//! `dyn ApplicationConfigurationService` in a plain provider), and a failed call is in the
//! contract's error type.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;
use std::time::Duration;

use futures::executor::block_on;
use rayx_app::services::ContractFailure;
use rayx_app::services::application_configuration::{
    ApplicationConfigurationError, ApplicationConfigurationService,
    ApplicationConfigurationSnapshot, LayoutDirection, MotionPolicy, ProgrammaticScrollBehavior,
    ScrollBehavior,
};
use rayx_app::testing::FakeApplicationConfiguration;
use rayx_app::{AppBuilder as _, RayXAppBuilder};

#[test]
fn the_behaviour_is_a_function_of_the_snapshot() {
    let mut snapshot = ApplicationConfigurationSnapshot::default();
    snapshot.options.scrolling.programmatic = ProgrammaticScrollBehavior::Smooth;
    assert_eq!(snapshot.layout_direction(), LayoutDirection::Ltr);
    assert!(!snapshot.effective_motion(None).is_reduced());
    assert!(
        snapshot
            .effective_motion(Some(MotionPolicy::Reduced))
            .local_reduced
    );
    assert_eq!(
        snapshot
            .effective_programmatic_scroll_behavior(None, None, None)
            .unwrap(),
        ScrollBehavior::Smooth(Duration::from_millis(200))
    );
    assert!(
        snapshot
            .effective_programmatic_scroll_behavior(None, Some(Duration::ZERO), None)
            .is_err()
    );

    snapshot.platform_reduced_motion = true;
    let motion = snapshot.effective_motion(None);
    assert!(motion.is_reduced() && motion.platform_reduced && !motion.app_reduced);
    assert_eq!(
        snapshot
            .effective_programmatic_scroll_behavior(None, None, None)
            .unwrap(),
        ScrollBehavior::Instant
    );
}

#[test]
fn the_fake_is_a_configuration_resolved_by_trait() {
    let fake = FakeApplicationConfiguration::new(ApplicationConfigurationSnapshot::default());
    let mut builder = RayXAppBuilder::new();
    builder
        .services_mut()
        .add_singleton_as::<dyn ApplicationConfigurationService>(
            Arc::clone(&fake) as Arc<dyn ApplicationConfigurationService>
        );
    let services = builder.build().unwrap();
    let configuration = services
        .required_service::<dyn ApplicationConfigurationService>()
        .unwrap();

    let mut options = configuration.options();
    options.layout.direction = LayoutDirection::Rtl;
    assert!(block_on(configuration.replace_options(options.clone())).unwrap());
    assert!(!block_on(configuration.replace_options(options.clone())).unwrap());
    assert!(configuration.layout_direction().is_rtl());
    assert_eq!(configuration.snapshot().revision, 1);
    options.name = "Renamed".into();
    assert_eq!(
        block_on(configuration.replace_options(options)),
        Err(ApplicationConfigurationError::StartupOnlyOptionChanged(
            "name"
        ))
    );

    fake.publish(ApplicationConfigurationSnapshot {
        platform_reduced_motion: true,
        ..configuration.snapshot()
    });
    assert!(configuration.effective_motion(None).platform_reduced);
}

#[test]
fn a_failed_call_is_the_contract_s_error() {
    let failure = ContractFailure {
        contract: "ApplicationConfigurationServiceContract",
        member: "replaceOptions",
        message: "the engine is gone".into(),
    };
    let error = ApplicationConfigurationError::from(failure.clone());
    assert_eq!(error, ApplicationConfigurationError::Failed(failure));
    assert!(error.to_string().contains("the engine is gone"), "{error}");
}
