//! The README `rayx new` writes for the templates that write RayX's (`mixed`, `rust`) points to
//! RayX's app-architecture page and says its three rules in a line each (dumb views, view models
//! under contracts, services named `*Service` and swapped in `configurePreviews`), and how an app
//! of the template adopts them as it grows: the `mixed` app's composition root moving into an
//! `App.ts` beside `App.tsx`, the `rust` app's `App.ts` composing through the TS builder. The
//! templates' code stays as it was: no `App.ts` is written.

mod common;

use std::path::{Path, PathBuf};

use clap::Parser;
use common::Workspace;
use rayx_cli::cli::{Cli, Command, NewArgs};
use rayx_cli::host::{Arch, HostFacts, Os, Outcome, Runner};
use rayx_cli::new::create;
use rayx_cli::nsh::store::{Kind, Receipt, Store};
use rayx_cli::project::pins::DEFAULTS;
use rayx_cli::setup::FakeMachine;

fn new_args(argv: &[&str]) -> NewArgs {
    let mut full = vec!["rayx", "new"];
    full.extend_from_slice(argv);
    match Cli::try_parse_from(full).expect("parses").command {
        Command::New(args) => args,
        other => panic!("{other:?}"),
    }
}

/// A store in `home` with the default pin installed.
fn with_default(home: &Workspace) {
    let id = DEFAULTS.nsh.commit[..12].to_owned();
    home.write(&format!("rayx-home/nsh/{id}/nsh/bin/nsh"), "#!");
    Store::at(home.path("rayx-home/nsh"))
        .complete(&Receipt {
            version: 1,
            id,
            kind: Kind::Distribution,
            url: "u".into(),
            reference: Some(DEFAULTS.nsh.tag.into()),
            commit: Some(DEFAULTS.nsh.commit.into()),
            nsh: "0.6.0".into(),
            cli: "nsh/bin/nsh".into(),
            installed: "2026-11-18T01:30:00Z".into(),
        })
        .expect("receipt");
}

/// A recorded runner whose `nsh init` writes the files nsh's templates write, whose
/// `cargo generate-lockfile` writes a lock and whose CLI answers its version.
fn runner() -> Runner {
    Runner::record().responder(move |spec| {
        if spec.args == ["--version"] {
            return Some(
                Outcome::success()
                    .with_stdout(format!("nsh 0.6.0 ({})\n", &DEFAULTS.nsh.commit[..7])),
            );
        }
        if spec.args.first().is_some_and(|a| a == "init") {
            let dir = PathBuf::from(&spec.args[1]);
            std::fs::create_dir_all(dir.join("src")).expect("src");
            std::fs::write(dir.join("nsh.json"), "{\"name\":\"hello\"}").expect("nsh.json");
            std::fs::write(
                dir.join("Cargo.toml"),
                "[package]\nname = \"nsh-app-hello\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n\n[dependencies]\nnsh-core = \"0.6.0\"\nnsh-base = \"0.6.0\"\n",
            )
            .expect("Cargo.toml");
            std::fs::write(
                dir.join("src/lib.rs"),
                "use nsh_core::prelude::*;\n\n#[native_module(\"hello/native\")]\nfn native(registry: &mut Registry) {}\n",
            )
            .expect("lib");
            return Some(Outcome::success());
        }
        if spec.program == "cargo" && spec.args == ["generate-lockfile"] {
            let dir = spec.cwd.clone().expect("cwd");
            std::fs::write(dir.join("Cargo.lock"), "version = 4\n").expect("lock");
            return Some(Outcome::success());
        }
        None
    })
}

/// Runs `rayx new hello --template <template>` in `work`; the app's directory.
fn created(work: &Workspace, template: &str) -> PathBuf {
    let home = Workspace::empty();
    with_default(&home);
    let machine = FakeMachine::new()
        .with_home("/Users/dev")
        .with_env("RAYX_HOME", &home.path("rayx-home").display().to_string());
    let host = HostFacts {
        os: Os::MacOs,
        arch: Arch::Arm64,
        emulated: false,
        wsl: false,
        distro: None,
    };
    create(
        &new_args(&["hello", "--template", template]),
        &work.root,
        &host,
        &machine,
        &mut runner(),
        &mut Vec::new(),
    )
    .expect("created");
    work.path("hello")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|_| panic!("{}", path.display()))
}

/// The README's section on the app architecture: from its heading to the next paragraph that is
/// no part of it.
fn architecture_section(readme: &str) -> &str {
    let start = readme
        .find("## App architecture")
        .unwrap_or_else(|| panic!("no `## App architecture` in\n{readme}"));
    let rest = &readme[start..];
    let end = rest.find("`rayx app` runs").unwrap_or(rest.len());
    &rest[..end]
}

#[test]
fn both_readmes_point_to_the_app_architecture_with_its_three_rules() {
    for template in ["mixed", "rust"] {
        let work = Workspace::empty();
        let app = created(&work, template);
        let readme = read(&app.join("README.md"));
        let section = architecture_section(&readme);
        for said in [
            &format!(
                "[app architecture](https://github.com/rayx-ui/RayX/blob/v{}/docs/app-architecture.md)",
                env!("CARGO_PKG_VERSION")
            )[..],
            "- Views are dumb: they render only the data and event props they are given.",
            "is a view model, resolved with `useViewModel` (Rust\n  `use_view_model`) under a contract.",
            "- Work and data are services under a contract, named `<Name>Service`;",
            "replace them in `configurePreviews(builder)`.",
        ] {
            assert!(
                section.contains(said),
                "{template}: no `{said}` in\n{section}"
            );
        }
        assert!(
            !section.contains("__"),
            "{template}: a placeholder is left in\n{section}"
        );
    }
}

#[test]
fn each_readme_says_how_its_app_adopts_the_architecture() {
    let mixed = read(&created(&Workspace::empty(), "mixed").join("README.md"));
    let mixed = architecture_section(&mixed);
    for said in [
        "move the composition root out of `App.tsx` into an `App.ts` beside it",
        "`export function configure(builder)`",
        "put `configurePreviews(builder)` beside it",
    ] {
        assert!(mixed.contains(said), "mixed: no `{said}` in\n{mixed}");
    }

    let rust = read(&created(&Workspace::empty(), "rust").join("README.md"));
    let rust = architecture_section(&rust);
    for said in [
        "the composition root is Rust (`src/lib.rs`)",
        "add an `App.ts` at the app's root, beside `App.tsx` once the app has TSX views",
        "composes through the TS builder and its extensions",
        "`#[app_extension]`",
        "`configurePreviews(builder)` the design view models and services",
    ] {
        assert!(rust.contains(said), "rust: no `{said}` in\n{rust}");
    }
}

#[test]
fn the_templates_code_writes_no_app_ts() {
    for template in ["mixed", "rust"] {
        let work = Workspace::empty();
        let app = created(&work, template);
        assert!(!app.join("App.ts").exists(), "{template} writes an App.ts");
    }
}
