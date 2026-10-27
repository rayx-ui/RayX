//! The application configuration service: binding, validation, reloads, effective motion and
//! programmatic scrolling.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use rayx_core::{Configuration, RayXAppBuilder, ServiceProvider};
use rayx_runtime::services::application_configuration::*;
use rayx_runtime::services::platform::PlatformInfoService;
use serde_json::json;

fn build(
    configure: impl Fn(&mut ApplicationConfiguration) + Send + Sync + 'static,
) -> ServiceProvider {
    let mut builder = RayXAppBuilder::new().add_application_configuration(configure);
    builder.configuration.add_value(json!({
        "RayX": { "Application": {
            "name": "Gamify",
            "motion": { "policy": "System" },
            "tooltips": { "showDelayMs": 250 },
            "someKeyFromElsewhere": true
        } }
    }));
    let services = builder.build().unwrap();
    services.validate_options_on_start().unwrap();
    services
}

#[test]
fn settings_bind_from_the_section_then_code() {
    let services = build(|options| options.layout.direction = LayoutDirection::Rtl);
    let configuration = services
        .required_service::<ApplicationConfigurationService>()
        .unwrap();
    let options = configuration.options();
    assert_eq!(options.name, "Gamify");
    assert_eq!(options.tooltips.show_delay_ms, 250);
    assert_eq!(options.tooltips.grace_period_ms, 300);
    assert!(configuration.layout_direction().is_rtl());
}

#[test]
fn invalid_settings_fail_on_start() {
    let services = RayXAppBuilder::new()
        .add_application_configuration(|options| options.notifications.auto_hide_ms = 0)
        .build()
        .unwrap();
    assert!(services.validate_options_on_start().is_err());
}

#[test]
fn the_service_follows_configuration_reloads_but_not_its_name() {
    let services = build(|_| {});
    let configuration = services
        .required_service::<ApplicationConfigurationService>()
        .unwrap();
    let heard = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&heard);
    let _subscription =
        configuration.subscribe(move |snapshot| sink.lock().unwrap().push(snapshot.revision));
    let root = services.required_service::<Configuration>().unwrap();

    root.set("RayX:Application:Motion:Policy", "Reduced");
    assert_eq!(configuration.options().motion.policy, MotionPolicy::Reduced);
    assert_eq!(configuration.snapshot().revision, 1);

    root.set("RayX:Application:Name", "Renamed");
    assert_eq!(configuration.options().name, "Gamify");
    assert_eq!(*heard.lock().unwrap(), vec![1]);
}

#[test]
fn startup_only_settings_are_refused_at_run_time() {
    let services = build(|_| {});
    let configuration = services
        .required_service::<ApplicationConfigurationService>()
        .unwrap();
    let mut options = configuration.options();
    options.description = "changed".into();
    assert_eq!(
        configuration.replace_options(options),
        Err(ApplicationConfigurationError::StartupOnlyOptionChanged(
            "description"
        ))
    );
    let mut options = configuration.options();
    options.scrolling.smooth_duration_ms = 0;
    assert!(configuration.replace_options(options).is_err());
    assert!(
        !configuration
            .replace_options(configuration.options())
            .unwrap()
    );
}

#[test]
fn motion_is_reduced_by_the_system_the_app_or_the_caller() {
    let services =
        build(|options| options.scrolling.programmatic = ProgrammaticScrollBehavior::Smooth);
    let configuration = services
        .required_service::<ApplicationConfigurationService>()
        .unwrap();
    let platform = services.required_service::<PlatformInfoService>().unwrap();

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

    platform.modify(|snapshot| snapshot.reduced_motion = true);
    let motion = configuration.effective_motion(None);
    assert!(motion.is_reduced() && motion.platform_reduced && !motion.app_reduced);
    assert_eq!(
        configuration
            .effective_programmatic_scroll_behavior(None, None, None)
            .unwrap(),
        ScrollBehavior::Instant
    );
}
