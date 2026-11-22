//! The prebuilt CLIs (`rayx dev`): `rayx dev install nsh|rayx` builds a checkout's CLI with the
//! quick profile and puts it in `$RAYX_HOME/dev` (`~/.rayx/dev`) beside a record of where it came
//! from; a project whose NSH is a local checkout runs that `nsh`, and `rayx` never builds a CLI
//! when it runs one.

mod common;

use std::fs;
use std::path::Path;

use clap::Parser;
use common::Workspace;
use rayx_cli::app::prepare;
use rayx_cli::cli::{AppArgs, Cli, Command, DevCommand, DevTool};
use rayx_cli::dev::{self, Dev, Tool, checkout_cli, find_checkout, install};
use rayx_cli::host::path_env::{MemoryPathStore, UserPath};
use rayx_cli::host::{Arch, CommandSpec, HostFacts, Os, Outcome, Runner};
use rayx_cli::nsh::pin::resolve;
use rayx_cli::setup::{FakeMachine, Machine, PlanEnv, Set};

fn mac() -> HostFacts {
    HostFacts {
        os: Os::MacOs,
        arch: Arch::Arm64,
        emulated: false,
        wsl: false,
        distro: None,
    }
}

fn outcome(code: i32, stdout: &str) -> Outcome {
    Outcome {
        code: Some(code),
        stdout: stdout.to_string(),
        stderr: String::new(),
    }
}

/// A recording runner whose git answers the checkout's commit, its uncommitted files and, for
/// `git diff --quiet`, `diff`'s exit code.
fn git_runner(diff: i32) -> Runner {
    Runner::record().responder(move |spec| {
        if spec.program != "git" {
            return None;
        }
        match spec.args.first().map(String::as_str) {
            Some("rev-parse") => Some(outcome(0, "c9bf1fe9a2b1\n")),
            Some("status") => Some(outcome(0, " M crates/nsh-cli/src/run.rs\n")),
            Some("diff") => Some(outcome(diff, "")),
            _ => None,
        }
    })
}

fn builds(runner: &Runner) -> Vec<CommandSpec> {
    runner
        .specs()
        .iter()
        .filter(|spec| spec.program == "cargo" && spec.args.first().is_some_and(|a| a == "build"))
        .cloned()
        .collect()
}

/// An NSH checkout whose build "made" `nsh` with `text`.
fn nsh_checkout(work: &Workspace, text: &str) -> std::path::PathBuf {
    work.write(
        "nsh/crates/nsh-cli/Cargo.toml",
        "[package]\nname = \"nsh-cli\"\n",
    );
    work.write("nsh/target/quick/nsh", text);
    work.path("nsh")
}

#[test]
fn the_folder_is_beside_the_nsh_store_in_rayx_home_else_in_dot_rayx() {
    let home = Workspace::empty();
    let machine = FakeMachine::new()
        .with_home("/Users/dev")
        .with_env("RAYX_HOME", &home.path("rayx-home").display().to_string());
    assert_eq!(
        Dev::locate(&machine).expect("a folder").root,
        home.path("rayx-home/dev")
    );
    let machine = FakeMachine::new().with_home("/Users/dev");
    assert_eq!(
        Dev::locate(&machine).expect("a folder").root,
        Path::new("/Users/dev").join(".rayx").join("dev")
    );
    let store = rayx_cli::nsh::store::Store::at(home.path("rayx-home/nsh"));
    assert_eq!(Dev::beside(&store).root, home.path("rayx-home/dev"));
}

#[test]
fn install_builds_the_checkouts_cli_quick_and_puts_it_in_the_folder_with_its_record() {
    let work = Workspace::empty();
    let checkout = nsh_checkout(&work, "built nsh");
    let dev = Dev::at(work.path("rayx-home/dev"));
    let mut runner = git_runner(0);
    let mut out = Vec::new();
    let record = install(
        Tool::Nsh,
        &checkout,
        &dev,
        &mut runner,
        &FakeMachine::new().with_home("/Users/dev"),
        &mac(),
        &mut out,
    )
    .expect("installed");
    let builds = builds(&runner);
    assert_eq!(builds.len(), 1, "one build: {builds:?}");
    let build = &builds[0];
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
        ]
    );
    assert_eq!(build.cwd.as_deref(), Some(checkout.as_path()));
    assert!(
        dev::quick_profile_env()
            .iter()
            .all(|pair| build.env.contains(pair)),
        "the profile is defined in the build's environment: {:?}",
        build.env
    );
    assert!(build.unset.contains(&"CARGO".to_string()));
    assert!(
        build.unset.contains(&"RUSTUP_TOOLCHAIN".to_string()),
        "the checkout builds with its own toolchain"
    );
    assert_eq!(
        fs::read_to_string(dev.cli(Tool::Nsh, Os::MacOs)).expect("the CLI"),
        "built nsh"
    );
    assert_eq!(record.tool, "nsh");
    assert_eq!(record.checkout, checkout);
    assert_eq!(record.commit.as_deref(), Some("c9bf1fe9a2b1"));
    assert_eq!(record.uncommitted, Some(1));
    assert_eq!(record.profile, "quick");
    assert_eq!(dev.record(Tool::Nsh), Some(record), "the record is kept");
    let left: Vec<_> = fs::read_dir(&dev.root)
        .expect("the folder")
        .map(|entry| entry.expect("an entry").file_name())
        .collect();
    assert_eq!(
        left.len(),
        2,
        "the CLI and its record, nothing staged: {left:?}"
    );
    let said = String::from_utf8(out).expect("UTF-8");
    assert!(said.contains("rayx: nsh is now"), "{said}");
}

#[test]
fn install_of_rayx_builds_rayx_cli_from_a_rayx_checkout() {
    let work = Workspace::empty();
    work.write(
        "rayx/crates/rayx_cli/Cargo.toml",
        "[package]\nname = \"rayx-cli\"\n",
    );
    work.write("rayx/target/quick/rayx", "built rayx");
    let dev = Dev::at(work.path("dev"));
    let mut runner = git_runner(0);
    install(
        Tool::Rayx,
        &work.path("rayx"),
        &dev,
        &mut runner,
        &FakeMachine::new().with_home("/Users/dev"),
        &mac(),
        &mut Vec::new(),
    )
    .expect("installed");
    assert_eq!(builds(&runner)[0].args[4], "rayx-cli");
    assert_eq!(
        fs::read_to_string(dev.cli(Tool::Rayx, Os::MacOs)).expect("the CLI"),
        "built rayx"
    );
}

#[test]
fn install_refuses_a_folder_that_is_no_checkout_and_a_build_that_made_nothing() {
    let work = Workspace::empty();
    let dev = Dev::at(work.path("dev"));
    let mut runner = git_runner(0);
    let error = install(
        Tool::Nsh,
        &work.root,
        &dev,
        &mut runner,
        &FakeMachine::new().with_home("/Users/dev"),
        &mac(),
        &mut Vec::new(),
    )
    .expect_err("no checkout");
    assert!(
        error.to_string().contains("crates/nsh-cli/Cargo.toml"),
        "{error}"
    );
    assert!(builds(&runner).is_empty(), "nothing is built");

    work.write("nsh/crates/nsh-cli/Cargo.toml", "[package]\n");
    let error = install(
        Tool::Nsh,
        &work.path("nsh"),
        &dev,
        &mut runner,
        &FakeMachine::new().with_home("/Users/dev"),
        &mac(),
        &mut Vec::new(),
    )
    .expect_err("the build made no CLI");
    assert!(error.to_string().contains("the build made no"), "{error}");
    assert!(!dev.cli(Tool::Nsh, Os::MacOs).exists(), "nothing is placed");
}

#[test]
fn a_new_install_replaces_the_cli_in_one_step() {
    let work = Workspace::empty();
    let checkout = nsh_checkout(&work, "first");
    let dev = Dev::at(work.path("dev"));
    let machine = FakeMachine::new().with_home("/Users/dev");
    for text in ["first", "second"] {
        work.write("nsh/target/quick/nsh", text);
        install(
            Tool::Nsh,
            &checkout,
            &dev,
            &mut git_runner(0),
            &machine,
            &mac(),
            &mut Vec::new(),
        )
        .expect("installed");
    }
    assert_eq!(
        fs::read_to_string(dev.cli(Tool::Nsh, Os::MacOs)).expect("the CLI"),
        "second"
    );
}

#[test]
fn the_checkout_is_the_one_the_folder_is_in() {
    let work = Workspace::empty();
    work.write("nsh/crates/nsh-cli/Cargo.toml", "[package]\n");
    work.write("nsh/crates/nsh-core/src/lib.rs", "");
    assert_eq!(
        find_checkout(Tool::Nsh, &work.path("nsh/crates/nsh-core/src")).expect("found"),
        work.path("nsh")
    );
    let error = find_checkout(Tool::Rayx, &work.path("nsh")).expect_err("no RayX checkout");
    assert!(error.to_string().contains("crates/rayx_cli"), "{error}");
}

#[test]
fn a_checkout_pin_runs_the_prebuilt_nsh_and_a_missing_one_names_the_install() {
    let work = Workspace::empty();
    let checkout = work.path("nsh");
    let dev = Dev::at(work.path("dev"));
    let mut err = Vec::new();
    let error = checkout_cli(&checkout, &dev, &mut git_runner(0), &mac(), &mut err)
        .expect_err("no prebuilt nsh");
    assert!(
        error
            .to_string()
            .contains(&format!("`rayx dev install nsh {}`", checkout.display())),
        "{error}"
    );
    work.write("dev/nsh", "#!");
    let cli = checkout_cli(&checkout, &dev, &mut git_runner(0), &mac(), &mut err)
        .expect("the prebuilt nsh");
    assert_eq!(cli, work.path("dev/nsh"));
    assert!(err.is_empty(), "no record, nothing to warn of");
}

/// The folder with an nsh whose record says it came from `checkout` at `c9bf1fe9a2b1`.
fn recorded(work: &Workspace, checkout: &Path) -> Dev {
    let dev = Dev::at(work.path("dev"));
    work.write("dev/nsh", "#!");
    let record = dev::Record {
        version: 1,
        tool: "nsh".into(),
        checkout: checkout.to_path_buf(),
        commit: Some("c9bf1fe9a2b1".into()),
        uncommitted: Some(0),
        profile: "quick".into(),
        installed: "2026-11-22T10:00:00Z".into(),
    };
    work.write(
        "dev/nsh.json",
        &serde_json::to_string(&record).expect("a record"),
    );
    dev
}

#[test]
fn a_change_to_the_clis_code_since_its_commit_is_warned_of_and_a_library_change_is_not() {
    let work = Workspace::empty();
    work.write("nsh/crates/nsh-cli/Cargo.toml", "[package]\n");
    let checkout = work.path("nsh");
    let dev = recorded(&work, &checkout);
    let mut err = Vec::new();
    let mut runner = git_runner(1);
    checkout_cli(&checkout, &dev, &mut runner, &mac(), &mut err).expect("the prebuilt nsh");
    let said = String::from_utf8(err).expect("UTF-8");
    assert!(
        said.contains("NSH's CLI code changed since the prebuilt nsh (c9bf1fe9a2b1)"),
        "{said}"
    );
    let diff = runner
        .specs()
        .iter()
        .find(|spec| spec.program == "git")
        .cloned()
        .expect("a git diff");
    assert_eq!(
        diff.args,
        [
            "diff",
            "--quiet",
            "c9bf1fe9a2b1",
            "--",
            "crates/nsh-cli",
            "crates/nsh-bundler",
            "crates/nsh-dev",
            "crates/nsh-package"
        ],
        "only the CLI's own code counts"
    );
    let mut err = Vec::new();
    checkout_cli(&checkout, &dev, &mut git_runner(0), &mac(), &mut err).expect("the prebuilt nsh");
    assert!(
        err.is_empty(),
        "unchanged CLI code: {}",
        String::from_utf8_lossy(&err)
    );
}

#[test]
fn an_nsh_built_from_another_checkout_is_warned_of() {
    let work = Workspace::empty();
    work.write("worktree/crates/nsh-cli/Cargo.toml", "[package]\n");
    let dev = recorded(&work, &work.path("worktree"));
    let mut err = Vec::new();
    checkout_cli(
        &work.path("nsh"),
        &dev,
        &mut git_runner(0),
        &mac(),
        &mut err,
    )
    .expect("the prebuilt nsh still runs");
    let said = String::from_utf8(err).expect("UTF-8");
    assert!(
        said.contains("was built from") && said.contains("worktree"),
        "{said}"
    );
}

#[test]
fn status_lists_each_cli_with_where_it_came_from() {
    let work = Workspace::empty();
    let dev = recorded(&work, &work.path("nsh"));
    let mut out = Vec::new();
    dev::status(&dev, &mac(), false, &mut out).expect("status");
    let text = String::from_utf8(out).expect("UTF-8");
    assert!(text.contains("at c9bf1fe9a2b1"), "{text}");
    assert!(
        text.contains("rayx  none: `rayx dev install rayx` builds it"),
        "{text}"
    );
    let mut out = Vec::new();
    dev::status(&dev, &mac(), true, &mut out).expect("status");
    let value: serde_json::Value = serde_json::from_slice(&out).expect("JSON");
    assert_eq!(value["tools"][0]["tool"], "nsh");
    assert_eq!(value["tools"][0]["present"], true);
    assert_eq!(value["tools"][0]["record"]["commit"], "c9bf1fe9a2b1");
    assert_eq!(value["tools"][1]["present"], false);
}

#[test]
fn the_command_line_takes_a_tool_and_an_optional_checkout() {
    let cli = Cli::try_parse_from(["rayx", "dev", "install", "nsh", "../nsh"]).expect("parses");
    match cli.command {
        Command::Dev {
            command: DevCommand::Install { tool, checkout },
        } => {
            assert_eq!(tool, DevTool::Nsh);
            assert_eq!(checkout.as_deref(), Some(Path::new("../nsh")));
        }
        other => panic!("{other:?}"),
    }
    let cli = Cli::try_parse_from(["rayx", "dev", "status", "--json"]).expect("parses");
    assert!(matches!(
        cli.command,
        Command::Dev {
            command: DevCommand::Status { json: true }
        }
    ));
    assert!(Cli::try_parse_from(["rayx", "dev", "install", "cargo"]).is_err());
}

fn app_args(argv: &[&str]) -> AppArgs {
    let mut full = vec!["rayx", "app"];
    full.extend_from_slice(argv);
    match Cli::try_parse_from(full).expect("parses").command {
        Command::App(args) => args,
        other => panic!("{other:?}"),
    }
}

#[test]
fn rayx_app_on_a_checkout_pin_with_no_prebuilt_nsh_stops_and_builds_nothing() {
    let home = Workspace::empty();
    let app = Workspace::fixture("nsh-path-app");
    resolve(&mut Runner::execute(), &app.path("app")).expect("the lock is written");
    let machine = FakeMachine::new()
        .with_home("/Users/dev")
        .with_env("RAYX_HOME", &home.path("rayx-home").display().to_string())
        .with_env("NSH_DESIGN_SOCKET", "/tmp/tool/design.sock");
    let mut ensure =
        |_: &PlanEnv, _: &[Set], _: &Path, _: &mut Runner, _: &dyn Machine, _: &mut UserPath| Ok(0);
    let mut runner = common::reading_cargo();
    let error = prepare(
        &app_args(&["run", "Stage.tsx"]),
        &app.path("app"),
        &mac(),
        &mut runner,
        &machine,
        &mut UserPath::Windows(Box::new(MemoryPathStore::new(None))),
        &mut ensure,
        &mut Vec::new(),
    )
    .expect_err("no prebuilt nsh");
    assert!(
        error.to_string().contains("rayx dev install nsh"),
        "{error}"
    );
    assert!(builds(&runner).is_empty(), "nothing is built");
}
