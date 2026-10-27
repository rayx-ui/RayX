//! The platform service: snapshots, their changes and listeners, and registration.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use rayx_core::RayXAppBuilder;
use rayx_runtime::services::platform::*;

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
fn updates_notify_only_real_changes() {
    let platform = PlatformInfoService::default();
    let heard = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&heard);
    let subscription = platform.subscribe(move |event| {
        sink.lock()
            .unwrap()
            .push((event.previous.color_scheme, event.current.color_scheme));
    });
    assert!(platform.modify(|snapshot| snapshot.color_scheme = ColorScheme::Dark));
    assert!(!platform.modify(|snapshot| snapshot.color_scheme = ColorScheme::Dark));
    assert_eq!(platform.current().color_scheme, ColorScheme::Dark);
    drop(subscription);
    assert!(platform.update(PlatformSnapshot::default()));
    assert_eq!(
        *heard.lock().unwrap(),
        vec![(ColorScheme::Light, ColorScheme::Dark)]
    );
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
    let platform = services.required_service::<PlatformInfoService>().unwrap();
    platform.modify(|snapshot| snapshot.font_scale = 1.5);
    let scoped = services
        .create_scope()
        .required_service::<PlatformInfoService>()
        .unwrap();
    assert_eq!(scoped.current().font_scale, 1.5);

    let custom = PlatformInfoService::new(PlatformSnapshot::handheld(
        ViewportSnapshot {
            width: 1.0,
            height: 2.0,
        },
        FormFactor::Tablet,
    ));
    let services = RayXAppBuilder::new()
        .with_platform_service(custom)
        .add_platform()
        .build()
        .unwrap();
    assert_eq!(
        services
            .required_service::<PlatformInfoService>()
            .unwrap()
            .current()
            .form_factor,
        FormFactor::Tablet
    );
}
