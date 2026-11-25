//! `rayx new` writes RayX's dev tools into a new app: a `mixed` app's `src/lib.rs` adds the
//! `DevTools` class and the `DevLayer` component to RayX's UI set, its `App.tsx` mounts
//! `<DevLayer />` at the root, out of its layout; a `rust` app's root mounts `dev_layer()`; the app's README says what
//! they are. The real proof, that such an app builds and passes its test, is
//! `proof_new_devtools`.

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

/// The default pin's lock source.
fn fork_source() -> String {
    format!(
        "git+https://github.com/dXnSix/nsh?tag={}#{}",
        DEFAULTS.nsh.tag, DEFAULTS.nsh.commit
    )
}

/// The default pin's store id: its commit's first 12 characters.
fn id() -> String {
    DEFAULTS.nsh.commit[..12].to_owned()
}

fn new_args(argv: &[&str]) -> NewArgs {
    let mut full = vec!["rayx", "new"];
    full.extend_from_slice(argv);
    match Cli::try_parse_from(full).expect("parses").command {
        Command::New(args) => args,
        other => panic!("{other:?}"),
    }
}

fn host(os: Os) -> HostFacts {
    HostFacts {
        os,
        arch: Arch::Arm64,
        emulated: false,
        wsl: false,
        distro: None,
    }
}

/// A store in `home` with the default pin installed.
fn with_default(home: &Workspace) -> PathBuf {
    home.write(&format!("rayx-home/nsh/{}/nsh/bin/nsh", id()), "#!");
    Store::at(home.path("rayx-home/nsh"))
        .complete(&Receipt {
            version: 1,
            id: id(),
            kind: Kind::Distribution,
            url: "u".into(),
            reference: Some(DEFAULTS.nsh.tag.into()),
            commit: Some(DEFAULTS.nsh.commit.into()),
            nsh: "0.2.0".into(),
            cli: "nsh/bin/nsh".into(),
            installed: "2026-10-24T01:30:00Z".into(),
        })
        .expect("receipt");
    home.path(&format!("rayx-home/nsh/{}/nsh/bin/nsh", id()))
}

fn machine(home: &Workspace) -> FakeMachine {
    FakeMachine::new()
        .with_home("/Users/dev")
        .with_env("RAYX_HOME", &home.path("rayx-home").display().to_string())
}

/// A recorded runner whose `nsh init` writes a template's files as nsh names them (the crate
/// `nsh-app-hello`), or fails when `refuse`; whose
/// `cargo generate-lockfile` writes a lock with the patched NSH source, and whose CLI answers its
/// version.
fn runner(refuse: bool) -> Runner {
    Runner::record().responder(move |spec| {
        if spec.args == ["--version"] {
            return Some(Outcome::success().with_stdout(format!("nsh 0.7.0 ({})\n", &DEFAULTS.nsh.commit[..7])));
        }
        if spec.args.first().is_some_and(|a| a == "init") {
            if refuse {
                return Some(Outcome {
                    code: Some(1),
                    stdout: String::new(),
                    stderr: "error: the directory is not empty".into(),
                });
            }
            let dir = PathBuf::from(&spec.args[1]);
            let template = &spec.args[spec.args.iter().position(|a| a == "--template").expect("template") + 1];
            std::fs::create_dir_all(&dir).expect("dir");
            std::fs::write(dir.join("nsh.json"), "{\"name\":\"hello\"}").expect("nsh.json");
            if template != "ts" {
                // nsh's manifest; a Rust app's runs on the desktop platforms too.
                let mut manifest = String::from(
                    "[package]\nname = \"nsh-app-hello\"\nversion = \"0.1.0\"\nedition = \"2024\"\npublish = false\n\n[workspace]\n\n[dependencies]\nnsh-core = \"0.7.0\"\nnsh-base = \"0.7.0\"\n",
                );
                if template == "rust" {
                    manifest.push_str("env_logger = \"0.11\"\n\n[target.'cfg(target_os = \"macos\")'.dependencies]\nnsh-platform-macos = { version = \"0.7.0\", default-features = false }\n");
                }
                std::fs::write(dir.join("Cargo.toml"), manifest).expect("Cargo.toml");
                std::fs::create_dir_all(dir.join("src")).expect("src");
                std::fs::write(
                    dir.join("src/lib.rs"),
                    "//! nsh's template.\n\nuse nsh_core::prelude::*;\n\n#[native_module(\"hello/native\")]\nfn native(registry: &mut Registry) {}\n",
                )
                .expect("lib");
            }
            if template == "mixed" {
                std::fs::write(dir.join("App.tsx"), "// nsh's App.tsx\n").expect("App.tsx");
            }
            return Some(Outcome::success());
        }
        if spec.program == "cargo" && spec.args == ["generate-lockfile"] {
            let dir = spec.cwd.clone().expect("cwd");
            std::fs::write(
                dir.join("Cargo.lock"),
                format!(
                    "version = 4\n\n[[package]]\nname = \"nsh-app-hello\"\nversion = \"0.1.0\"\ndependencies = [\"nsh-core\"]\n\n[[package]]\nname = \"nsh-core\"\nversion = \"0.2.0\"\nsource = \"{}\"\n",
                    fork_source()
                ),
            )
            .expect("lock");
            return Some(Outcome::success());
        }
        None
    })
}

struct Created {
    result: anyhow::Result<()>,
}

fn run(argv: &[&str], cwd: &Path, home: &Workspace, os: Os, refuse: bool) -> Created {
    let mut runner = runner(refuse);
    let mut out = Vec::new();
    let result = create(
        &new_args(argv),
        cwd,
        &host(os),
        &machine(home),
        &mut runner,
        &mut out,
    );
    Created { result }
}

#[test]
fn a_mixed_app_registers_the_dev_tools_and_mounts_their_layer() {
    let home = Workspace::empty();
    with_default(&home);
    let work = Workspace::empty();
    let created = run(&["hello"], &work.root, &home, Os::MacOs, false);
    created.result.expect("created");
    let app = work.path("hello");

    let lib = std::fs::read_to_string(app.join("src/lib.rs")).expect("lib.rs");
    let set = lib.find("#[rayx::native_module(").expect("RayX's UI set");
    let dev_tools = lib
        .find(".class::<DevTools>()")
        .expect("the DevTools class");
    assert!(set < dev_tools, "in the set's block:\n{lib}");
    assert!(lib.contains(".register::<DevLayer>()"), "{lib}");
    assert!(lib.contains("use rayx::prelude::*;"));

    let tsx = std::fs::read_to_string(app.join("App.tsx")).expect("App.tsx");
    assert!(
        tsx.contains("import { DevLayer, KeyValueStore, RayXAppBuilder } from"),
        "{tsx}"
    );
    // At the root, the last child of the root view, out of its layout (absolute).
    let mounted = r#"<DevLayer style={{ position: "absolute" }} />"#;
    let layer = tsx.find(mounted).expect("mounted");
    assert!(
        tsx[layer..]
            .trim_start_matches(mounted)
            .trim_start()
            .starts_with("</View>"),
        "{tsx}"
    );

    let readme = std::fs::read_to_string(app.join("README.md")).expect("README");
    assert!(readme.contains("dev tools"), "{readme}");
}

#[test]
fn a_rust_app_mounts_the_layer_at_its_root() {
    let home = Workspace::empty();
    with_default(&home);
    let work = Workspace::empty();
    let created = run(
        &["hello", "--template", "rust"],
        &work.root,
        &home,
        Os::MacOs,
        false,
    );
    created.result.expect("created");
    let lib = std::fs::read_to_string(work.path("hello/src/lib.rs")).expect("lib.rs");
    let root = lib.find("pub fn App()").expect("the root");
    assert!(lib[root..].contains(".child(dev_layer())"), "{lib}");
}
