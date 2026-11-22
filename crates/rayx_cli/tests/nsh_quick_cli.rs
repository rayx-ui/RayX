//! A checkout's CLI, built quick for a design tool's run: `rayx app run` or `rayx app dev` with
//! `NSH_DESIGN_SOCKET` set builds a path checkout's CLI with `--profile quick` (release without
//! link-time optimization, 16 codegen units, incremental) into `target/quick/`, the profile
//! defined in the environment of the build and of the nsh it starts. Every other command, a run
//! started by hand, a store pin and `RAYX_NSH` keep the CLI as NSH ships it.

mod common;

use std::path::Path;

use clap::Parser;
use common::Workspace;
use rayx_cli::app::{Launch, prepare, quick_profile_env};
use rayx_cli::cli::{AppArgs, Cli, Command};
use rayx_cli::host::path_env::{MemoryPathStore, UserPath};
use rayx_cli::host::{Arch, CommandSpec, HostFacts, Os, Runner};
use rayx_cli::nsh::pin::resolve;
use rayx_cli::setup::{FakeMachine, Machine, PlanEnv, Set};

const SOCKET: &str = "/tmp/tool/design.sock";

fn app_args(argv: &[&str]) -> AppArgs {
    let mut full = vec!["rayx", "app"];
    full.extend_from_slice(argv);
    match Cli::try_parse_from(full).expect("parses").command {
        Command::App(args) => args,
        other => panic!("{other:?}"),
    }
}

fn mac() -> HostFacts {
    HostFacts {
        os: Os::MacOs,
        arch: Arch::Arm64,
        emulated: false,
        wsl: false,
        distro: None,
    }
}

struct Prepared {
    launch: Launch,
    warnings: String,
    runner: Runner,
}

fn run_prepare(args: &AppArgs, dir: &Path, machine: &FakeMachine) -> Prepared {
    let mut ensure =
        |_: &PlanEnv, _: &[Set], _: &Path, _: &mut Runner, _: &dyn Machine, _: &mut UserPath| Ok(0);
    let mut runner = common::reading_cargo();
    let mut err = Vec::new();
    let launch = prepare(
        args,
        dir,
        &mac(),
        &mut runner,
        machine,
        &mut UserPath::Windows(Box::new(MemoryPathStore::new(None))),
        &mut ensure,
        &mut err,
    )
    .expect("prepared");
    Prepared {
        launch,
        warnings: String::from_utf8(err).expect("UTF-8"),
        runner,
    }
}

/// The path-pin fixture with its lock written, and a machine whose environment has `env`.
fn path_app(env: &[(&str, &str)]) -> (Workspace, Workspace, FakeMachine) {
    let home = Workspace::empty();
    let app = Workspace::fixture("nsh-path-app");
    resolve(&mut Runner::execute(), &app.path("app")).expect("the lock is written");
    let mut machine = FakeMachine::new()
        .with_home("/Users/dev")
        .with_env("RAYX_HOME", &home.path("rayx-home").display().to_string());
    for (key, value) in env {
        machine = machine.with_env(key, value);
    }
    (home, app, machine)
}

fn cli_build(runner: &Runner) -> CommandSpec {
    runner
        .specs()
        .iter()
        .find(|spec| spec.program == "cargo" && spec.args.first().is_some_and(|a| a == "build"))
        .cloned()
        .expect("a build of the checkout's CLI")
}

fn has_quick_env(env: &[(String, String)]) -> bool {
    quick_profile_env().iter().all(|pair| env.contains(pair))
}

#[test]
fn the_quick_profile_is_release_without_link_time_optimization_built_incrementally() {
    assert_eq!(
        quick_profile_env(),
        [
            ("CARGO_PROFILE_QUICK_INHERITS", "release"),
            ("CARGO_PROFILE_QUICK_LTO", "off"),
            ("CARGO_PROFILE_QUICK_CODEGEN_UNITS", "16"),
            ("CARGO_PROFILE_QUICK_INCREMENTAL", "true"),
        ]
        .map(|(key, value)| (key.to_string(), value.to_string()))
    );
}

#[test]
fn a_design_tools_run_builds_the_checkouts_cli_quick_and_hands_nsh_the_profile() {
    for command in ["run", "dev"] {
        let (_home, app, machine) = path_app(&[("NSH_DESIGN_SOCKET", SOCKET)]);
        let prepared = run_prepare(
            &app_args(&[command, "Stage.tsx"]),
            &app.path("app"),
            &machine,
        );
        let checkout = app.path("nsh");
        let build = cli_build(&prepared.runner);
        assert_eq!(
            build.args,
            [
                "build",
                "--profile",
                "quick",
                "-p",
                "nsh-cli",
                "--target-dir",
                "target"
            ],
            "{command}"
        );
        assert_eq!(build.cwd.as_deref(), Some(checkout.as_path()));
        assert!(has_quick_env(&build.env), "{command}: {:?}", build.env);
        assert!(
            build.unset.contains(&"RUSTUP_TOOLCHAIN".to_string()),
            "the checkout still builds with its own toolchain"
        );
        assert_eq!(
            prepared.launch.program,
            checkout.join("target").join("quick").join("nsh"),
            "{command}: the quick CLI lives beside the release one, not over it"
        );
        assert!(
            has_quick_env(&prepared.launch.set),
            "{command}: nsh builds the checkout's hosts with the same profile: {:?}",
            prepared.launch.set
        );
        assert!(
            prepared
                .warnings
                .contains("(quick build, for a design tool's run)"),
            "{}",
            prepared.warnings
        );
    }
}

#[test]
fn builds_packages_and_publishes_get_the_cli_nsh_ships_even_from_a_design_tool() {
    for command in ["build", "package", "publish", "test", "check"] {
        let (_home, app, machine) = path_app(&[("NSH_DESIGN_SOCKET", SOCKET)]);
        let prepared = run_prepare(&app_args(&[command, "."]), &app.path("app"), &machine);
        let build = cli_build(&prepared.runner);
        assert_eq!(
            build.args,
            [
                "build",
                "--release",
                "-p",
                "nsh-cli",
                "--target-dir",
                "target"
            ],
            "{command}"
        );
        assert!(build.env.is_empty(), "{command}: {:?}", build.env);
        assert_eq!(
            prepared.launch.program,
            app.path("nsh").join("target").join("release").join("nsh"),
            "{command}"
        );
        assert!(
            !has_quick_env(&prepared.launch.set),
            "{command}: {:?}",
            prepared.launch.set
        );
        assert!(
            !prepared.warnings.contains("quick"),
            "{}",
            prepared.warnings
        );
    }
}

#[test]
fn a_run_no_design_tool_started_gets_the_cli_nsh_ships() {
    for socket in [None, Some(""), Some("  ")] {
        let env: Vec<(&str, &str)> = socket
            .map(|s| ("NSH_DESIGN_SOCKET", s))
            .into_iter()
            .collect();
        let (_home, app, machine) = path_app(&env);
        let prepared = run_prepare(&app_args(&["run", "--release"]), &app.path("app"), &machine);
        let build = cli_build(&prepared.runner);
        assert_eq!(build.args[1], "--release", "{socket:?}");
        assert_eq!(
            prepared.launch.program,
            app.path("nsh").join("target").join("release").join("nsh"),
            "{socket:?}"
        );
        assert!(!has_quick_env(&prepared.launch.set), "{socket:?}");
    }
}

#[test]
fn a_store_pin_or_rayx_nsh_builds_nothing_and_sets_no_profile_for_a_design_tool() {
    let home = Workspace::empty();
    let app = Workspace::fixture("nsh-fork-app");
    let built = home.path("nsh/target/release/nsh");
    let machine = FakeMachine::new()
        .with_home("/Users/dev")
        .with_env("RAYX_HOME", &home.path("rayx-home").display().to_string())
        .with_env("RAYX_NSH", &built.display().to_string())
        .with_env("NSH_DESIGN_SOCKET", SOCKET);
    let prepared = run_prepare(&app_args(&["run", "Stage.tsx"]), &app.root, &machine);
    assert_eq!(prepared.launch.program, built);
    assert!(
        !prepared
            .runner
            .specs()
            .iter()
            .any(|spec| spec.program == "cargo" && spec.args.first().is_some_and(|a| a == "build")),
        "nothing is built"
    );
    assert!(!has_quick_env(&prepared.launch.set));

    home.write("rayx-home/nsh/6e14d3507b61/nsh/bin/nsh", "#!");
    rayx_cli::nsh::store::Store::at(home.path("rayx-home/nsh"))
        .complete(&rayx_cli::nsh::store::Receipt {
            version: 1,
            id: "6e14d3507b61".into(),
            kind: rayx_cli::nsh::store::Kind::Distribution,
            url: "u".into(),
            reference: None,
            commit: None,
            nsh: "0.2.0".into(),
            cli: "nsh/bin/nsh".into(),
            installed: "2026-10-24T01:30:00Z".into(),
        })
        .expect("receipt");
    let machine = FakeMachine::new()
        .with_home("/Users/dev")
        .with_env("RAYX_HOME", &home.path("rayx-home").display().to_string())
        .with_env("NSH_DESIGN_SOCKET", SOCKET);
    let prepared = run_prepare(&app_args(&["run", "Stage.tsx"]), &app.root, &machine);
    assert_eq!(
        prepared.launch.program,
        home.path("rayx-home/nsh/6e14d3507b61/nsh/bin/nsh"),
        "a store pin's CLI is already built"
    );
    assert!(!has_quick_env(&prepared.launch.set));
}
