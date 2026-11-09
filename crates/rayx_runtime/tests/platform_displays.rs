//! A desktop's displays and window in the platform snapshot: the primary display, a display by
//! id and the one the window is on; none of them elsewhere.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use rayx_runtime::services::platform::*;

fn display(id: &str, primary: bool, x: f32) -> DisplaySnapshot {
    let bounds = PlatformRect {
        x,
        y: 0.0,
        width: 1920.0,
        height: 1080.0,
    };
    DisplaySnapshot {
        id: id.to_owned(),
        name: format!("Display {id}"),
        bounds,
        visible_bounds: bounds,
        primary,
        scale_factor: 2.0,
        refresh_rate: Some(60.0),
    }
}

#[test]
fn a_desktop_snapshot_finds_its_displays() {
    let snapshot = PlatformSnapshot {
        displays: vec![display("a", true, 0.0), display("b", false, 1920.0)],
        window: Some(WindowSnapshot {
            focused: true,
            visible: true,
            display: Some("b".to_owned()),
            ..WindowSnapshot::default()
        }),
        ..PlatformSnapshot::default()
    };
    assert_eq!(snapshot.primary_display().unwrap().id, "a");
    assert_eq!(snapshot.display("b").unwrap().bounds.x, 1920.0);
    assert_eq!(snapshot.window_display().unwrap().id, "b");
    assert!(snapshot.display("c").is_none());
}

#[test]
fn a_handheld_snapshot_has_no_displays_or_window() {
    let snapshot = PlatformSnapshot::handheld(
        ViewportSnapshot {
            width: 390.0,
            height: 844.0,
        },
        FormFactor::Phone,
    );
    assert!(snapshot.displays.is_empty());
    assert!(snapshot.primary_display().is_none());
    assert!(snapshot.window_display().is_none());
}
