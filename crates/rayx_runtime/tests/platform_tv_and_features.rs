//! The platform snapshot's TV input, platform name and features: a TV has a remote and an
//! on-screen keyboard and no pointer, a snapshot's platform name defaults to its family's, and
//! what the platform can do is all `false` until the platform tells.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use rayx_runtime::services::platform::*;

#[test]
fn a_tv_is_driven_by_a_remote() {
    let tv = InputCapabilities::for_form_factor(FormFactor::Tv);
    assert_eq!(tv.pointer, PointerCapability::None);
    assert_eq!(tv.keyboard, KeyboardCapability::Virtual);
    assert!(tv.remote);
    assert!(!tv.touch);
    for other in [
        FormFactor::Desktop,
        FormFactor::Phone,
        FormFactor::Tablet,
        FormFactor::Unknown,
    ] {
        assert!(
            !InputCapabilities::for_form_factor(other).remote,
            "{other:?}"
        );
    }
    let snapshot = PlatformSnapshot::handheld(
        ViewportSnapshot {
            width: 1920.0,
            height: 1080.0,
        },
        FormFactor::Tv,
    );
    assert!(snapshot.input.remote);
}

#[test]
fn a_snapshot_names_its_platform_and_knows_no_features_until_told() {
    let snapshot = PlatformSnapshot::default();
    assert_eq!(snapshot.platform_name, snapshot.family.name());
    assert_eq!(snapshot.features, PlatformFeatures::default());
    assert!(!snapshot.features.web_views);
    assert_eq!(snapshot.terminal_cell, None);
    assert_eq!(PlatformFamily::Tizen.name(), "tizen");
    assert_eq!(PlatformFamily::MacOS.name(), "macos");
    assert_eq!(PlatformFamily::Unknown("haiku".into()).name(), "haiku");
}
