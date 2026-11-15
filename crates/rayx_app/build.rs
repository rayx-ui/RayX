//! The dev tools' switch: `cfg(rayx_devtools)` is set when NSH's dev tools hooks are compiled in
//! and `RAYX_DEVTOOLS` is not `0`.
//!
//! The hooks (`nsh-core`'s feature `inspect`) are on exactly in a dev build: every platform's `dev`
//! feature turns them on and release runners never do, whatever the cargo profile. `nsh-core`
//! says so to the crates that depend on it (`DEP_NSH_CORE_INSPECT`, through its `links` key), so a
//! plain `cargo test` or `cargo check` of an app's crate, or the `describe` program, builds
//! without the agent and without the hooks, and a release build has neither.

fn main() {
    println!("cargo::rustc-check-cfg=cfg(rayx_devtools)");
    println!("cargo::rerun-if-env-changed=RAYX_DEVTOOLS");
    let hooks = std::env::var_os("DEP_NSH_CORE_INSPECT").is_some_and(|value| value == "1");
    let off = std::env::var_os("RAYX_DEVTOOLS").is_some_and(|value| value == "0");
    if hooks && !off {
        println!("cargo::rustc-cfg=rayx_devtools");
    }
}
