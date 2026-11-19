//! `rayx new` writes RayX's app builder API: a `mixed` app's `src/lib.rs` is the one-line UI set
//! `#[rayx::native_module("<specifier>")]` (whose line NSH's CLI reads the specifier from) and
//! its `App.tsx` builds the app with
//! `RayXAppBuilder` and starts it before it resolves a service; a `rust` app's root builds
//! `RayXAppBuilder::new()…` and calls `.start()` before it follows `app_state()` (which services
//! the chains register is `new_app_services`); no file names
//! the API it replaced (the Rust setter of the app, the TS entry and scope classes, the static
//! `of` lookups).

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

/// The files RayX writes in place of nsh's, by template.
const MIXED: [&str; 4] = ["src/lib.rs", "App.tsx", "App.test.tsx", "README.md"];
const RUST: [&str; 3] = ["src/lib.rs", "App.test.tsx", "README.md"];

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

/// A store in `home` with the default pin installed.
fn with_default(home: &Workspace) {
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
}

/// A recorded runner whose `nsh init` writes a template's files as nsh names them, whose
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
            let template = &spec.args[spec
                .args
                .iter()
                .position(|a| a == "--template")
                .expect("template")
                + 1];
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
            if template == "mixed" {
                std::fs::write(dir.join("App.tsx"), "// nsh's App.tsx\n").expect("App.tsx");
            }
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

/// Runs `rayx new hello` with `argv` after it in a fresh directory; the app's directory.
fn created(work: &Workspace, argv: &[&str]) -> PathBuf {
    let home = Workspace::empty();
    with_default(&home);
    let mut full = vec!["hello"];
    full.extend_from_slice(argv);
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
        &new_args(&full),
        &work.root,
        &host,
        &machine,
        &mut runner(),
        &mut Vec::new(),
    )
    .expect("created");
    work.path("hello")
}

fn read(app: &Path, file: &str) -> String {
    std::fs::read_to_string(app.join(file)).expect(file)
}

/// Where `needle` is in `text`, which must have it.
fn at(text: &str, needle: &str) -> usize {
    text.find(needle)
        .unwrap_or_else(|| panic!("no `{needle}` in\n{text}"))
}

#[test]
fn a_mixed_app_s_crate_is_rayx_s_attribute_on_its_registry_function() {
    let work = Workspace::empty();
    let app = created(&work, &[]);
    let lib = read(&app, "src/lib.rs");
    at(
        &lib,
        "#[rayx::native_module(\"hello/native\")]\nfn native(registry: &mut Registry) {",
    );
    at(&lib, "registry.class::<DevTools>().register::<DevLayer>();");
    // NSH's own attribute is not used directly: RayX's registers RayX's set first.
    assert!(!lib.contains("\n#[native_module"), "{lib}");
}

#[test]
fn a_mixed_app_builds_and_starts_the_app_before_it_uses_a_service() {
    let work = Workspace::empty();
    let app = created(&work, &[]);
    let tsx = read(&app, "App.tsx");
    at(
        &tsx,
        "import { DevLayer, KeyValueStore, RayXAppBuilder } from \"hello/native\";",
    );
    let built = at(&tsx, "new RayXAppBuilder()");
    let services = at(&tsx, ".addStorage()");
    let started = at(&tsx, ".start()");
    let resolved = at(&tsx, "services.getRequiredService(KeyValueStore)");
    assert!(
        built < services && services < started && started < resolved,
        "{tsx}"
    );
    at(&tsx, "status.set(\"Ready\")");
    at(
        &read(&app, "App.test.tsx"),
        "await app.waitFor(\"text:Ready\")",
    );
    let readme = read(&app, "README.md");
    at(&readme, "`#[rayx::native_module(\"hello/native\")]`");
    at(&readme, "`App.tsx` builds the app with `RayXAppBuilder`");
    assert!(
        !readme.contains("__"),
        "every placeholder is filled: {readme}"
    );
}

#[test]
fn a_rust_app_s_root_builds_the_chain_and_starts_it() {
    let work = Workspace::empty();
    let app = created(&work, &["--template", "rust"]);
    let lib = read(&app, "src/lib.rs");
    at(&lib, "#[rayx::native_module(\"hello/native\")]");
    let root = at(&lib, "pub fn App() -> impl IntoElement");
    let built = at(&lib, "RayXAppBuilder::new()");
    let started = at(&lib, ".start();");
    let followed = at(&lib, "rayx::app_nsh::app_state()");
    assert!(
        root < built && built < started && started < followed,
        "{lib}"
    );
    let readme = read(&app, "README.md");
    at(&readme, "builds the app with `RayXAppBuilder::new()`");
    assert!(
        !readme.contains("__"),
        "every placeholder is filled: {readme}"
    );
}

#[test]
fn no_file_rayx_writes_names_the_old_api() {
    for (argv, files) in [
        (&[][..], &MIXED[..]),
        (&["--template", "rust"][..], &RUST[..]),
    ] {
        let work = Workspace::empty();
        let app = created(&work, argv);
        for file in files {
            let text = read(&app, file);
            for old in [
                concat!("set_", "app"),
                concat!("App", "Services"),
                concat!("Service", "Scope"),
                concat!(".of", "(app"),
                "fn build_app",
            ] {
                assert!(
                    !text.contains(old),
                    "{file} of {argv:?} names `{old}`:\n{text}"
                );
            }
        }
    }
}

/// NSH's CLI's specifier scan of a crate's source (`specifier_of` in `crates/nsh-cli/src/rust_app.rs`
/// at `nshx-0.6.0-6`): the first line that, trimmed, starts with `#[` and has `native_module`
/// followed by `("…"`. The web runtime keeps a set's gate by it.
fn nsh_specifier_of(code: &str) -> Option<String> {
    code.lines().find_map(|line| {
        const ATTR: &str = "native_module";
        let attr = line.trim().strip_prefix("#[")?;
        let rest = attr[attr.find(ATTR)? + ATTR.len()..].trim_start();
        let rest = rest.strip_prefix('(')?.trim_start().strip_prefix('"')?;
        Some(rest[..rest.find('"')?].to_owned())
    })
}

#[test]
fn nsh_s_cli_reads_the_specifier_of_either_template_s_crate() {
    for argv in [&[][..], &["--template", "rust"][..]] {
        let work = Workspace::empty();
        let app = created(&work, argv);
        let lib = read(&app, "src/lib.rs");
        assert_eq!(
            nsh_specifier_of(&lib).as_deref(),
            Some("hello/native"),
            "{argv:?}:\n{lib}"
        );
    }
}
