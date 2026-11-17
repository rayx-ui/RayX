//! RayX's macros find the crates under their names through the `rayx` umbrella: a crate that
//! depends on `rayx` alone uses `#[derive(AppService)]` (`::rayx::app::di`, the core),
//! `#[rayx::contract]` and `#[rayx::app_extension]` (`::rayx::app_nsh`, the NSH host) and
//! `#[rayx::native_module]`, and what they generate compiles and registers.
//!
//! A crate on `rayx_app` alone and one on `rayx_app_nsh` alone (`tests/macro_paths/`, written
//! out and checked with `cargo check`) use the derive through the crate they depend on, and the
//! second `#[contract]` and `#[app_extension]` through the host; `rayx_app_macros`' unit tests
//! cover the lookup of a renamed dependency.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

use rayx::app_nsh::ts::{ContractError, Exception};
use rayx::prelude::*;

/// A service registered through the umbrella's derive.
#[derive(AppService)]
struct Greeting(&'static str);

/// A generic service: the derive keeps its bounds.
#[derive(AppService)]
struct Cache<T: Send + Sync + 'static> {
    entries: Vec<T>,
}

/// Why a forecast failed.
#[derive(Debug)]
pub enum ForecastError {
    /// The call did not reach TS.
    Contract(ContractError),
    /// What TS threw.
    Js(String),
}

impl From<ContractError> for ForecastError {
    fn from(error: ContractError) -> Self {
        Self::Contract(error)
    }
}

impl From<Exception> for ForecastError {
    fn from(exception: Exception) -> Self {
        Self::Js(exception.message)
    }
}

/// Forecasts, implementable in TS.
#[rayx::contract]
pub trait Forecasts {
    /// The forecast for `city`.
    async fn forecast(&self, city: String) -> Result<String, ForecastError>;
}

/// Registers the greeting.
#[rayx::app_extension]
fn add_greeting(builder: RayXAppBuilder) -> RayXAppBuilder {
    builder.add_singleton(Greeting("hello"))
}

#[rayx::native_module("test/macro-paths")]
fn native(registry: &mut Registry) {
    registry
        .contract::<dyn Forecasts>()
        .module::<add_greeting>();
}

fn is_app_service<T: AppService + ?Sized>() {}

#[test]
fn the_derive_names_the_core_through_the_umbrella() {
    is_app_service::<Greeting>();
    is_app_service::<Cache<u32>>();
    let services = ServiceProvider::default()
        .with_singleton(Greeting("hi"))
        .with_singleton(Cache::<u32> {
            entries: vec![1, 2],
        });
    assert_eq!(services.required_service::<Greeting>().unwrap().0, "hi");
    assert_eq!(
        services.required_service::<Cache<u32>>().unwrap().entries,
        [1, 2]
    );
}

#[test]
fn the_contract_extension_and_set_name_the_host_through_the_umbrella() {
    let set = registry();
    let mut names = BTreeSet::new();
    names.extend(
        set.module_specs()
            .iter()
            .map(|m| format!("module {}", m.name)),
    );
    names.extend(
        set.interface_defs()
            .iter()
            .map(|i| format!("interface {}", i.name)),
    );
    names.extend(
        set.class_specs()
            .iter()
            .map(|c| format!("class {}", c.name)),
    );
    for expected in [
        "class RayXAppBuilder",
        "interface Forecasts",
        "module ForecastsToken",
        "module add_greeting",
    ] {
        assert!(names.contains(expected), "{expected} missing: {names:?}");
    }
}

/// Writes the fixture `name` of `tests/macro_paths/` into `target/macro-paths/<name>` (its
/// `Cargo.toml.in` with the crates' paths and the workspace's NSH patch, and the workspace's
/// lock file) and runs `cargo check` on it; its errors when it does not compile.
fn fixture_compiles(name: &str) -> Result<(), String> {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = crate_dir.ancestors().nth(2).unwrap();
    let fixture = crate_dir.join("tests/macro_paths").join(name);
    let dir = root.join("target/macro-paths").join(name);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let workspace = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    let patch = &workspace[workspace.find("[patch.crates-io]").unwrap()..];
    // Forward slashes: a Windows path's backslashes are escapes in a TOML string.
    let path_of = |krate: &str| {
        root.join("crates")
            .join(krate)
            .to_str()
            .unwrap()
            .replace('\\', "/")
    };
    let manifest = std::fs::read_to_string(fixture.join("Cargo.toml.in"))
        .unwrap()
        .replace("@RAYX_APP_NSH@", &path_of("rayx_app_nsh"))
        .replace("@RAYX_APP@", &path_of("rayx_app"))
        .replace("@PATCH@", patch);
    std::fs::write(dir.join("Cargo.toml"), manifest).unwrap();
    std::fs::copy(fixture.join("src/lib.rs"), dir.join("src/lib.rs")).unwrap();
    std::fs::copy(root.join("Cargo.lock"), dir.join("Cargo.lock")).unwrap();
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let out = Command::new(cargo)
        .current_dir(&dir)
        .args(["check", "--quiet"])
        .env("CARGO_TARGET_DIR", root.join("target/macro-paths/target"))
        .output()
        .unwrap();
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

#[test]
fn the_derive_names_the_core_in_a_crate_on_the_core_alone() {
    fixture_compiles("core_only").unwrap_or_else(|errors| panic!("{errors}"));
}

#[test]
fn the_derive_contract_and_extension_name_the_host_in_a_crate_on_the_host_alone() {
    fixture_compiles("host_only").unwrap_or_else(|errors| panic!("{errors}"));
}
