//! RayX's application configuration: binding, validation, reloads, effective motion (following
//! the platform's setting) and programmatic scrolling, and registration (the later of
//! `add_application_configuration` and the app's own service wins).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::executor::block_on;
use rayx_app::services::application_configuration::*;
use rayx_app::services::platform::{PlatformInfoService, PlatformSnapshot};
use rayx_app::testing::{FakeApplicationConfiguration, FakePlatform};
use rayx_app::{AppBuilder as _, Configuration, RayXAppBuilder, ServiceProvider};
use rayx_services_application_configuration::ApplicationConfigurationAppBuilderExt as _;
use serde_json::json;

fn build(
    configure: impl Fn(&mut ApplicationConfiguration) + Send + Sync + 'static,
) -> ServiceProvider {
    let mut builder = RayXAppBuilder::new().add_application_configuration(configure);
    add_fake_platform(&mut builder.services);
    builder.configuration.add_value(json!({
        "RayX": { "Application": {
            "name": "Notes",
            "motion": { "policy": "System" },
            "tooltips": { "showDelayMs": 250 },
            "someKeyFromElsewhere": true
        } }
    }));
    let services = builder.build().unwrap();
    services.validate_options_on_start().unwrap();
    services
}

/// Changes the fake platform's snapshot with `change` and publishes it.
fn modify(platform: &FakePlatform, change: impl FnOnce(&mut PlatformSnapshot)) {
    let mut snapshot = (*platform.platform().get()).clone();
    change(&mut snapshot);
    platform.publish(snapshot);
}

/// Registers a fake platform as the app's `dyn PlatformInfoService` (and as itself, for the
/// test to change).
fn add_fake_platform(services: &mut ServiceProvider) {
    let platform = FakePlatform::new(PlatformSnapshot::default());
    services.add_singleton_as::<FakePlatform>(Arc::clone(&platform));
    services.add_singleton_as::<dyn PlatformInfoService>(platform as Arc<dyn PlatformInfoService>);
}

#[test]
fn settings_bind_from_the_section_then_code() {
    let services = build(|options| options.layout.direction = LayoutDirection::Rtl);
    let configuration = services
        .required_service::<dyn ApplicationConfigurationService>()
        .unwrap();
    let options = configuration.options();
    assert_eq!(options.name, "Notes");
    assert_eq!(options.tooltips.show_delay_ms, 250);
    assert_eq!(options.tooltips.grace_period_ms, 300);
    assert!(configuration.layout_direction().is_rtl());
}

#[test]
fn invalid_settings_fail_on_start() {
    let mut builder = RayXAppBuilder::new()
        .add_application_configuration(|options| options.notifications.auto_hide_ms = 0);
    add_fake_platform(&mut builder.services);
    let services = builder.build().unwrap();
    assert!(services.validate_options_on_start().is_err());
}

#[test]
fn the_service_follows_configuration_reloads_but_not_its_name() {
    let services = build(|_| {});
    let configuration = services
        .required_service::<dyn ApplicationConfigurationService>()
        .unwrap();
    let heard = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&heard);
    let _subscription = configuration
        .configuration()
        .subscribe(move |snapshot| sink.lock().unwrap().push(snapshot.revision));
    let root = services.required_service::<Configuration>().unwrap();

    root.set("RayX:Application:Motion:Policy", "Reduced");
    assert_eq!(configuration.options().motion.policy, MotionPolicy::Reduced);
    assert_eq!(configuration.snapshot().revision, 1);

    root.set("RayX:Application:Name", "Renamed");
    assert_eq!(configuration.options().name, "Notes");
    assert_eq!(*heard.lock().unwrap(), vec![1]);
}

#[test]
fn startup_only_settings_are_refused_at_run_time() {
    let services = build(|_| {});
    let configuration = services
        .required_service::<dyn ApplicationConfigurationService>()
        .unwrap();
    let mut options = configuration.options();
    options.description = "changed".into();
    assert_eq!(
        block_on(configuration.replace_options(options)),
        Err(ApplicationConfigurationError::StartupOnlyOptionChanged(
            "description"
        ))
    );
    let mut options = configuration.options();
    options.scrolling.smooth_duration_ms = 0;
    assert!(block_on(configuration.replace_options(options)).is_err());
    assert!(!block_on(configuration.replace_options(configuration.options())).unwrap());
    let mut options = configuration.options();
    options.hover_cards.open_delay_ms = 100;
    assert!(block_on(configuration.replace_options(options)).unwrap());
    assert_eq!(
        configuration.snapshot().options.hover_cards.open_delay_ms,
        100
    );
    assert_eq!(configuration.snapshot().revision, 1);
}

#[test]
fn motion_is_reduced_by_the_system_the_app_or_the_caller() {
    let services =
        build(|options| options.scrolling.programmatic = ProgrammaticScrollBehavior::Smooth);
    let configuration = services
        .required_service::<dyn ApplicationConfigurationService>()
        .unwrap();
    let platform = services.required_service::<FakePlatform>().unwrap();

    assert!(!configuration.effective_motion(None).is_reduced());
    assert_eq!(
        configuration
            .effective_programmatic_scroll_behavior(None, None, None)
            .unwrap(),
        ScrollBehavior::Smooth(Duration::from_millis(200))
    );
    assert!(
        configuration
            .effective_motion(Some(MotionPolicy::Reduced))
            .local_reduced
    );
    assert_eq!(
        configuration
            .effective_programmatic_scroll_behavior(None, Some(Duration::from_millis(80)), None)
            .unwrap(),
        ScrollBehavior::Smooth(Duration::from_millis(80))
    );
    assert!(
        configuration
            .effective_programmatic_scroll_behavior(None, Some(Duration::ZERO), None)
            .is_err()
    );

    modify(&platform, |snapshot| snapshot.reduced_motion = true);
    let motion = configuration.effective_motion(None);
    assert!(motion.is_reduced() && motion.platform_reduced && !motion.app_reduced);
    assert_eq!(
        configuration
            .effective_programmatic_scroll_behavior(None, None, None)
            .unwrap(),
        ScrollBehavior::Instant
    );
}

#[test]
fn the_snapshot_follows_the_platform_s_motion_setting() {
    let services = build(|_| {});
    let configuration = services
        .required_service::<dyn ApplicationConfigurationService>()
        .unwrap();
    let platform = services.required_service::<FakePlatform>().unwrap();
    let heard = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&heard);
    let _subscription = configuration.configuration().subscribe(move |snapshot| {
        sink.lock()
            .unwrap()
            .push((snapshot.platform_reduced_motion, snapshot.revision))
    });

    modify(&platform, |snapshot| snapshot.reduced_motion = true);
    // Another change of the platform is no change of the snapshot.
    modify(&platform, |snapshot| {
        snapshot.color_scheme = rayx_app::services::platform::ColorScheme::Dark
    });
    modify(&platform, |snapshot| snapshot.reduced_motion = false);
    assert_eq!(*heard.lock().unwrap(), vec![(true, 1), (false, 2)]);
    assert!(!configuration.snapshot().platform_reduced_motion);
}

#[test]
fn without_a_platform_the_build_names_what_to_add() {
    let Err(error) = RayXAppBuilder::new()
        .add_application_configuration(|_| {})
        .build()
    else {
        panic!("the build fails");
    };
    let error = error.to_string();
    assert!(error.contains("ApplicationConfigurationService"), "{error}");
    assert!(error.contains("PlatformInfoService"), "{error}");
    assert!(error.contains("add_platform()"), "{error}");
    assert!(error.contains("rayx_services_platform"), "{error}");
    // Nor does it register one.
    let mut builder = RayXAppBuilder::new().add_application_configuration(|_| {});
    assert!(
        !builder
            .services_mut()
            .contains_service::<dyn PlatformInfoService>()
    );
}

#[test]
fn the_later_registration_of_a_configuration_wins() {
    let own: Arc<dyn ApplicationConfigurationService> =
        FakeApplicationConfiguration::new(ApplicationConfigurationSnapshot::default());

    // `add_application_configuration` replaces one the app registered before it.
    let mut builder = RayXAppBuilder::new();
    builder
        .services_mut()
        .replace_service::<dyn ApplicationConfigurationService>(Arc::clone(&own));
    let mut builder =
        builder.add_application_configuration(|options| options.name = "Notes".into());
    add_fake_platform(&mut builder.services);
    let services = builder.build().unwrap();
    let configuration = services
        .required_service::<dyn ApplicationConfigurationService>()
        .unwrap();
    assert!(!Arc::ptr_eq(&configuration, &own));
    assert_eq!(configuration.options().name, "Notes");

    // One the app registers after it replaces RayX's.
    let mut builder = RayXAppBuilder::new().add_application_configuration(|_| {});
    add_fake_platform(&mut builder.services);
    builder
        .services_mut()
        .replace_service::<dyn ApplicationConfigurationService>(Arc::clone(&own));
    let services = builder.build().unwrap();
    let configuration = services
        .required_service::<dyn ApplicationConfigurationService>()
        .unwrap();
    assert!(Arc::ptr_eq(&configuration, &own));
}
