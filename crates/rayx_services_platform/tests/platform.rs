//! The platform service: snapshots, their changes as the service's `StateCell` publishes them,
//! and registration.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use rayx_app::services::platform::*;
use rayx_app::testing::FakePlatform;
use rayx_app::{AppBuilder as _, RayXAppBuilder};
use rayx_services_platform::PlatformAppBuilderExt as _;

#[test]
fn snapshots_describe_desktops_and_handhelds() {
    let desktop = PlatformSnapshot::default();
    assert!(desktop.desktop);
    assert_eq!(desktop.input.pointer, PointerCapability::Fine);
    assert_eq!(desktop.orientation(), Orientation::Landscape);
    let phone = PlatformSnapshot::handheld(
        ViewportSnapshot {
            width: 390.0,
            height: 844.0,
        },
        FormFactor::Phone,
    );
    assert!(!phone.desktop);
    assert!(phone.input.touch);
    assert_eq!(phone.input.keyboard, KeyboardCapability::Virtual);
    assert_eq!(phone.orientation(), Orientation::Portrait);
}

#[test]
fn subscribers_hear_each_published_snapshot_until_they_stop() {
    let platform = FakePlatform::new(PlatformSnapshot::default());
    let service: &dyn PlatformInfoService = &*platform;
    let heard = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&heard);
    let subscription = service.platform().subscribe(move |snapshot| {
        sink.lock().unwrap().push(snapshot.color_scheme);
    });
    platform.publish(PlatformSnapshot {
        color_scheme: ColorScheme::Dark,
        ..PlatformSnapshot::default()
    });
    assert_eq!(service.platform().get().color_scheme, ColorScheme::Dark);
    drop(subscription);
    platform.publish(PlatformSnapshot::default());
    assert_eq!(service.platform().get().color_scheme, ColorScheme::Light);
    assert_eq!(*heard.lock().unwrap(), vec![ColorScheme::Dark]);
    assert_eq!(service.platform().subscribers(), 0);
}

#[test]
fn snapshots_round_trip_as_json() {
    let mut snapshot = PlatformSnapshot::default();
    snapshot.locale.languages = vec!["hr-HR".into(), "en".into()];
    snapshot.locale.text_direction = TextDirection::Ltr;
    let json = serde_json::to_string(&snapshot).unwrap();
    assert_eq!(
        serde_json::from_str::<PlatformSnapshot>(&json).unwrap(),
        snapshot
    );
}

#[test]
fn add_platform_registers_one_service_per_root() {
    let services = RayXAppBuilder::new().add_platform().build().unwrap();
    // The service initializes at startup, where it starts following NSH's environment.
    assert_eq!(services.startup_registry().services().len(), 1);
    let platform = services
        .required_service::<dyn PlatformInfoService>()
        .unwrap();
    platform.platform().publish(PlatformSnapshot {
        font_scale: 1.5,
        ..PlatformSnapshot::default()
    });
    let scoped = services
        .create_scope()
        .required_service::<dyn PlatformInfoService>()
        .unwrap();
    assert!(Arc::ptr_eq(&platform, &scoped));
    assert_eq!(scoped.platform().get().font_scale, 1.5);

    // Each root has its own.
    let other = RayXAppBuilder::new()
        .add_platform()
        .build()
        .unwrap()
        .required_service::<dyn PlatformInfoService>()
        .unwrap();
    assert!(!Arc::ptr_eq(&platform, &other));
    assert_eq!(other.platform().get().font_scale, 1.0);
}

#[test]
fn add_platform_replaces_an_apps_earlier_platform_service() {
    let custom = FakePlatform::new(PlatformSnapshot::handheld(
        ViewportSnapshot {
            width: 1.0,
            height: 2.0,
        },
        FormFactor::Tablet,
    ));
    let mut builder = RayXAppBuilder::new();
    builder
        .services_mut()
        .add_singleton_as::<dyn PlatformInfoService>(custom as Arc<dyn PlatformInfoService>);
    let services = builder.add_platform().add_platform().build().unwrap();
    assert_eq!(services.startup_registry().services().len(), 1);
    assert_eq!(
        services
            .required_service::<dyn PlatformInfoService>()
            .unwrap()
            .platform()
            .get()
            .form_factor,
        FormFactor::Desktop
    );
}
