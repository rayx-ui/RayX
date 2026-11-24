//! The installed `rayx` hands its command line over to the prebuilt one (`rayx dev`): a release
//! on `PATH` older than `~/.rayx/dev/rayx` never runs a checkout's commands, where it would build
//! NSH's CLI from the checkout.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use rayx_cli::dev::{HANDED_OVER, NO_HANDOVER, hand_over_to};
use rayx_cli::host::Os;
use rayx_cli::setup::FakeMachine;

const HOME: &str = "/home/me";

fn installed() -> PathBuf {
    PathBuf::from(HOME).join(".rayx/bin/rayx")
}

fn prebuilt() -> PathBuf {
    PathBuf::from(HOME).join(".rayx/dev/rayx")
}

/// A machine with both CLIs.
fn machine() -> FakeMachine {
    FakeMachine::new()
        .with_home(HOME)
        .with_file(installed())
        .with_file(prebuilt())
}

fn same(path: &Path) -> PathBuf {
    path.to_path_buf()
}

#[test]
fn the_installed_rayx_hands_over_to_the_prebuilt_one() {
    let to = hand_over_to(
        &machine(),
        Os::MacOs,
        &installed(),
        Some(OsStr::new("app")),
        same,
    );
    assert_eq!(to, Some(prebuilt()));
}

#[test]
fn no_hand_over_without_a_prebuilt_rayx() {
    let machine = FakeMachine::new().with_home(HOME).with_file(installed());
    assert_eq!(
        hand_over_to(&machine, Os::MacOs, &installed(), None, same),
        None
    );
}

#[test]
fn the_prebuilt_rayx_and_a_link_to_it_run_themselves() {
    assert_eq!(
        hand_over_to(&machine(), Os::MacOs, &prebuilt(), None, same),
        None
    );
    // `~/.rayx/bin/rayx` a link to the prebuilt one.
    let linked = |path: &Path| {
        if path == installed() {
            prebuilt()
        } else {
            path.to_path_buf()
        }
    };
    assert_eq!(
        hand_over_to(&machine(), Os::MacOs, &installed(), None, linked),
        None
    );
}

#[test]
fn another_rayx_runs_itself() {
    let elsewhere = PathBuf::from("/work/RayX/target/debug/rayx");
    assert_eq!(
        hand_over_to(&machine(), Os::MacOs, &elsewhere, None, same),
        None
    );
}

#[test]
fn self_commands_and_the_switches_run_themselves() {
    let m = machine();
    assert_eq!(
        hand_over_to(&m, Os::MacOs, &installed(), Some(OsStr::new("self")), same),
        None
    );
    let m = machine().with_env(HANDED_OVER, "1");
    assert_eq!(hand_over_to(&m, Os::MacOs, &installed(), None, same), None);
    let m = machine().with_env(NO_HANDOVER, "1");
    assert_eq!(hand_over_to(&m, Os::MacOs, &installed(), None, same), None);
}

#[test]
fn rayx_home_and_windows_names() {
    let m = FakeMachine::new()
        .with_env("RAYX_HOME", "/opt/rx")
        .with_file("/opt/rx/bin/rayx.exe")
        .with_file("/opt/rx/dev/rayx.exe");
    let to = hand_over_to(
        &m,
        Os::Windows,
        Path::new("/opt/rx/bin/rayx.exe"),
        None,
        same,
    );
    assert_eq!(to, Some(PathBuf::from("/opt/rx/dev/rayx.exe")));
}
