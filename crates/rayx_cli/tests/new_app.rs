//! `rayx new` (NEW-1, design §4.7): the default pin's `nsh init`, then, for a RayX app, `rayx` at
//! this `rayx`'s tag and RayX's files, then the pin written into the app: `.cargo/config.toml`
//! patches and `cargo generate-lockfile` for `mixed` and `rust`, `rayx.toml` for `ts`
//! (`--nsh-only`), and the next steps. `nsh init` and Cargo are recorded; the recorded `nsh init`
//! writes the files nsh's templates write, with their first lines.

mod common;

use std::path::{Path, PathBuf};

use assert_cmd::cargo::cargo_bin_cmd;
use clap::Parser;
use common::Workspace;
use predicates::prelude::*;
use predicates::str::contains;
use rayx_cli::cli::{Cli, Command, NewArgs};
use rayx_cli::host::{Arch, HostFacts, Os, Outcome, Runner};
use rayx_cli::new::{
    cargo_config, create, native_module_specifier, nsh_dependencies, rayx_dependency, rayx_toml,
    with_rayx_dependency,
};
use rayx_cli::nsh::pin::{Origin, resolve};
use rayx_cli::nsh::store::{Kind, Receipt, Store};
use rayx_cli::project::pins::DEFAULTS;
use rayx_cli::setup::FakeMachine;

const ID: &str = "1d64f049d809";
const FORK_SOURCE: &str =
    "git+https://github.com/dXnSix/nsh?tag=fork-0.4.0-1#1d64f049d8096803740f65515b46941d47702344";

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
    home.write(&format!("rayx-home/nsh/{ID}/nsh/bin/nsh"), "#!");
    Store::at(home.path("rayx-home/nsh"))
        .complete(&Receipt {
            version: 1,
            id: ID.into(),
            kind: Kind::Distribution,
            url: "u".into(),
            reference: Some("fork-0.4.0-1".into()),
            commit: Some(DEFAULTS.nsh.commit.into()),
            nsh: "0.2.0".into(),
            cli: "nsh/bin/nsh".into(),
            installed: "2026-10-24T01:30:00Z".into(),
        })
        .expect("receipt");
    home.path(&format!("rayx-home/nsh/{ID}/nsh/bin/nsh"))
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
            return Some(Outcome::success().with_stdout("nsh 0.4.0 (1d64f04)\n"));
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
                    "[package]\nname = \"nsh-app-hello\"\nversion = \"0.1.0\"\nedition = \"2024\"\npublish = false\n\n[workspace]\n\n[dependencies]\nnsh-core = \"0.4.0\"\nnsh-base = \"0.4.0\"\n",
                );
                if template == "rust" {
                    manifest.push_str("env_logger = \"0.11\"\n\n[target.'cfg(target_os = \"macos\")'.dependencies]\nnsh-platform-macos = { version = \"0.4.0\", default-features = false }\n");
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
                    "version = 4\n\n[[package]]\nname = \"nsh-app-hello\"\nversion = \"0.1.0\"\ndependencies = [\"nsh-core\"]\n\n[[package]]\nname = \"nsh-core\"\nversion = \"0.2.0\"\nsource = \"{FORK_SOURCE}\"\n"
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
    out: String,
    runner: Runner,
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
    Created {
        result,
        out: String::from_utf8(out).expect("UTF-8"),
        runner,
    }
}

#[test]
fn a_mixed_app_is_made_by_the_default_pins_nsh_init_and_pinned_through_its_lock() {
    let home = Workspace::empty();
    let cli = with_default(&home);
    let work = Workspace::empty();
    let created = run(&["hello"], &work.root, &home, Os::MacOs, false);
    created.result.expect("created");
    let app = work.path("hello");

    let init = &created.runner.specs()[1];
    assert_eq!(
        PathBuf::from(&init.program),
        cli,
        "the default pin's CLI, from the store"
    );
    assert_eq!(
        init.args,
        [
            "init",
            app.display().to_string().as_str(),
            "--template",
            "mixed"
        ]
    );
    assert!(init.unset.contains(&"CARGO".to_string()));

    let config = std::fs::read_to_string(app.join(".cargo/config.toml")).expect("config");
    // The app names only crates the default patches already.
    assert_eq!(config, cargo_config(&DEFAULTS.nsh, &[]));
    let patched: Vec<&str> = config
        .lines()
        .filter(|l| {
            l.contains("= { git = \"https://github.com/dXnSix/nsh\", tag = \"fork-0.4.0-1\" }")
        })
        .collect();
    assert_eq!(patched.len(), DEFAULTS.nsh.crates.len());
    for name in DEFAULTS.nsh.crates {
        assert!(config.contains(&format!("\n{name} = {{ git")), "{name}");
    }
    let lock = created
        .runner
        .specs()
        .iter()
        .find(|s| s.program == "cargo")
        .expect("generate-lockfile");
    assert_eq!(lock.args, ["generate-lockfile"]);
    assert_eq!(lock.cwd.as_deref(), Some(app.as_path()));

    // The app's pin is now the default, from its own lock.
    let pin = resolve(&mut Runner::print(), &app).expect("pin");
    assert_eq!(pin.origin, Origin::Project);
    assert_eq!(pin.id().as_deref(), Some(ID));
    assert!(!app.join("rayx.toml").exists());
}

#[test]
fn a_rust_app_gets_the_same_patches_and_lock() {
    let home = Workspace::empty();
    with_default(&home);
    let work = Workspace::empty();
    let created = run(
        &["svc", "--template", "rust", "--name", "Service"],
        &work.root,
        &home,
        Os::Linux,
        false,
    );
    created.result.expect("created");
    let init = &created.runner.specs()[1];
    assert_eq!(init.args[2..], ["--template", "rust", "--name", "Service"]);
    // The platform crate its desktop runner names is patched as well as RayX's crates.
    let config = std::fs::read_to_string(work.path("svc/.cargo/config.toml")).expect("config");
    assert_eq!(
        config,
        cargo_config(&DEFAULTS.nsh, &["nsh-platform-macos".to_string()])
    );
    assert!(
        config.contains(
            "\nnsh-platform-macos = { git = \"https://github.com/dXnSix/nsh\", tag = \"fork-0.4.0-1\" }\n"
        ),
        "{config}"
    );
    assert!(
        created
            .runner
            .specs()
            .iter()
            .any(|s| s.args == ["generate-lockfile"])
    );
    // The root starts the app's services.
    let lib = std::fs::read_to_string(work.path("svc/src/lib.rs")).expect("lib.rs");
    for needle in [
        "#[native_module(\"hello/native\")]",
        "pub fn App() -> impl IntoElement",
        "set_app(build_app);",
        "rayx::app::start();",
    ] {
        assert!(lib.contains(needle), "{needle}: {lib}");
    }
    assert!(
        std::fs::read_to_string(work.path("svc/App.test.tsx"))
            .expect("App.test.tsx")
            .contains("await app.waitFor(\"text:Ready\")")
    );
}

#[test]
fn a_ts_app_gets_rayx_toml_with_the_default_table_and_no_cargo_files() {
    let home = Workspace::empty();
    with_default(&home);
    let work = Workspace::empty();
    let created = run(
        &["web", "--template", "ts", "--nsh-only"],
        &work.root,
        &home,
        Os::MacOs,
        false,
    );
    created.result.expect("created");
    let app = work.path("web");
    assert_eq!(
        std::fs::read_to_string(app.join("rayx.toml")).expect("rayx.toml"),
        rayx_toml(&DEFAULTS.nsh)
    );
    assert!(!app.join(".cargo").exists());
    assert!(!created.runner.specs().iter().any(|s| s.program == "cargo"));
    let pin = resolve(&mut Runner::print(), &app).expect("pin");
    assert_eq!(pin.origin, Origin::RayxToml);
    assert_eq!(pin.id().as_deref(), Some(ID));
}

#[test]
fn the_next_steps_are_printed_with_ios_only_on_a_mac() {
    let home = Workspace::empty();
    with_default(&home);
    let work = Workspace::empty();
    let mac = run(&["hello"], &work.root, &home, Os::MacOs, false);
    assert!(
        mac.out.contains(&format!(
            "Created hello, a RayX app on rayx v{}, pinned to nsh fork-0.4.0-1 (1d64f04). Next:",
            env!("CARGO_PKG_VERSION")
        )),
        "{}",
        mac.out
    );
    let plain = Workspace::empty();
    let nsh = run(
        &["hello", "--nsh-only"],
        &plain.root,
        &home,
        Os::MacOs,
        false,
    );
    assert!(
        nsh.out
            .contains("Created hello, pinned to nsh fork-0.4.0-1 (1d64f04). Next:"),
        "{}",
        nsh.out
    );
    for line in [
        "  cd hello",
        "  rayx setup --web --ios",
        "  rayx app run --target web",
    ] {
        assert!(mac.out.contains(line), "{line:?} in {}", mac.out);
    }
    let other = Workspace::empty();
    let linux = run(&["hello"], &other.root, &home, Os::Linux, false);
    assert!(linux.out.contains("  rayx setup --web\n"), "{}", linux.out);
    assert!(!linux.out.contains("--ios"), "{}", linux.out);
}

#[test]
fn a_directory_nsh_refuses_is_left_unchanged() {
    let home = Workspace::empty();
    with_default(&home);
    let work = Workspace::empty();
    work.write("taken/README.md", "mine");
    let created = run(&["taken"], &work.root, &home, Os::MacOs, true);
    let error = format!("{:#}", created.result.expect_err("refused"));
    assert!(error.contains("nsh init"), "{error}");
    let names: Vec<String> = std::fs::read_dir(work.path("taken"))
        .expect("dir")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["README.md"], "nothing was written");
    assert!(!created.runner.specs().iter().any(|s| s.program == "cargo"));
}

#[test]
fn a_missing_default_pin_is_installed_first() {
    let home = Workspace::empty();
    let work = Workspace::empty();
    let created = run(&["hello"], &work.root, &home, Os::MacOs, false);
    // The recording serves no archive, so the install stops at the download; it came first.
    assert!(created.result.is_err());
    let first = created.runner.lines().first().cloned().unwrap_or_default();
    assert!(
        first.contains("releases/download/fork-0.4.0-1/nsh-macos-arm64.tar.gz.sha256"),
        "{first}"
    );
    assert!(
        !created
            .runner
            .specs()
            .iter()
            .any(|s| s.args.first().is_some_and(|a| a == "init"))
    );
}

#[test]
fn an_unknown_template_is_rejected_listing_the_three() {
    cargo_bin_cmd!("rayx")
        .args(["new", "app", "--template", "rayx"])
        .assert()
        .code(2)
        .stderr(
            contains("invalid value 'rayx'")
                .and(contains("mixed"))
                .and(contains("ts"))
                .and(contains("rust")),
        );
}

#[test]
fn a_rayx_app_depends_on_rayx_at_this_rayx_s_tag_and_registers_its_classes() {
    let home = Workspace::empty();
    with_default(&home);
    let work = Workspace::empty();
    let created = run(&["hello"], &work.root, &home, Os::MacOs, false);
    created.result.expect("created");
    let app = work.path("hello");
    let read = |file: &str| std::fs::read_to_string(app.join(file)).expect(file);

    let manifest = read("Cargo.toml");
    let dependency = format!(
        "rayx = {{ git = \"https://github.com/rayx-ui/RayX\", tag = \"v{}\" }}",
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(rayx_dependency(), dependency);
    assert!(
        manifest.contains(&format!("[dependencies]\n{dependency}\nnsh-core = ")),
        "{manifest}"
    );
    let lib = read("src/lib.rs");
    for needle in [
        "#[native_module(\"hello/native\")]",
        "set_app(build_app);",
        ".class::<AppServices>()",
        ".class::<ServiceScope>()",
        ".class::<I18n>()",
        "fn build_app(builder: RayXAppBuilder) -> RayXAppBuilder",
    ] {
        assert!(lib.contains(needle), "{needle}: {lib}");
    }
    let entry = read("App.tsx");
    assert!(
        entry.contains("import { AppServices } from \"hello/native\";"),
        "{entry}"
    );
    assert!(entry.contains("AppServices.load()"), "{entry}");
    assert!(read("App.test.tsx").contains("await app.waitFor(\"text:Ready\")"));
    let readme = read("README.md");
    assert!(readme.starts_with("# hello\n"), "{readme}");
    assert!(
        readme.contains(&format!("(`v{}`)", env!("CARGO_PKG_VERSION"))),
        "{readme}"
    );
    assert!(
        !readme.contains("__"),
        "every placeholder is filled: {readme}"
    );
}

#[test]
fn an_nsh_only_app_keeps_nsh_s_files() {
    let home = Workspace::empty();
    with_default(&home);
    let work = Workspace::empty();
    let created = run(
        &["hello", "--nsh-only"],
        &work.root,
        &home,
        Os::MacOs,
        false,
    );
    created.result.expect("created");
    let app = work.path("hello");
    let manifest = std::fs::read_to_string(app.join("Cargo.toml")).expect("Cargo.toml");
    assert!(!manifest.contains("rayx"), "{manifest}");
    assert!(
        std::fs::read_to_string(app.join("src/lib.rs"))
            .expect("lib.rs")
            .starts_with("//! nsh's template.")
    );
    assert_eq!(
        std::fs::read_to_string(app.join("App.tsx")).expect("App.tsx"),
        "// nsh's App.tsx\n"
    );
    assert!(app.join(".cargo/config.toml").is_file());
}

#[test]
fn a_ts_app_without_nsh_only_is_refused_before_anything_runs() {
    let home = Workspace::empty();
    with_default(&home);
    let work = Workspace::empty();
    let created = run(
        &["web", "--template", "ts"],
        &work.root,
        &home,
        Os::MacOs,
        false,
    );
    let error = format!("{:#}", created.result.expect_err("refused"));
    assert!(error.contains("--nsh-only"), "{error}");
    assert!(created.runner.specs().is_empty(), "nothing ran");
    assert!(!work.path("web").exists());
}

#[test]
fn the_manifest_edits_and_reads_follow_cargo_s_tables() {
    // `rayx` goes first into `[dependencies]`, or into a table of its own.
    let edited =
        with_rayx_dependency("[package]\nname = \"a\"\n\n[dependencies]\nnsh-core = \"0.3.0\"\n");
    assert!(
        edited.ends_with(&format!(
            "[dependencies]\n{}\nnsh-core = \"0.3.0\"\n",
            rayx_dependency()
        )),
        "{edited}"
    );
    let added = with_rayx_dependency("[package]\nname = \"a\"");
    assert!(
        added.ends_with(&format!("\n[dependencies]\n{}\n", rayx_dependency())),
        "{added}"
    );
    // NSH's crates in every table, renamed ones by their package; others are not NSH's.
    let names = nsh_dependencies(
        "[dependencies]\nnsh-core = \"0.3.0\"\ncore = { package = \"nsh-base\", version = \"0.3.0\" }\nserde = \"1\"\nnsh-app-hello = \"0.1\"\n\n[dev-dependencies]\nnsh-testing = \"0.3.0\"\n\n[target.'cfg(windows)'.dependencies]\nnsh-platform-windows = \"0.3.0\"\n",
    )
    .expect("parses");
    assert_eq!(
        names,
        [
            "nsh-base",
            "nsh-core",
            "nsh-platform-windows",
            "nsh-testing"
        ]
    );
    assert_eq!(
        native_module_specifier("use x;\n#[native_module(\"my-app/native\")]\nfn native() {}"),
        Some("my-app/native")
    );
    assert_eq!(native_module_specifier("fn native() {}"), None);
}

#[test]
fn the_files_rayx_writes_name_the_default_pin() {
    let config = cargo_config(&DEFAULTS.nsh, &[]);
    assert!(
        config.starts_with("# The NSH this app is built with"),
        "{config}"
    );
    assert!(config.contains("[patch.crates-io]\n"), "{config}");
    assert!(config.contains("fork-0.4.0-1 at 1d64f04"), "{config}");
    let table = rayx_toml(&DEFAULTS.nsh);
    for line in [
        "[nsh]",
        "git = \"https://github.com/dXnSix/nsh\"",
        "tag = \"fork-0.4.0-1\"",
        "commit = \"1d64f049d8096803740f65515b46941d47702344\"",
    ] {
        assert!(table.contains(line), "{table}");
    }
}

#[test]
fn a_pin_file_already_there_is_never_replaced() {
    let home = Workspace::empty();
    with_default(&home);
    for (template, file, kind) in [
        ("mixed", ".cargo/config.toml", "--nsh-only"),
        ("mixed", ".cargo/config.toml", ""),
        ("rust", ".cargo/config.toml", ""),
        ("ts", "rayx.toml", "--nsh-only"),
    ] {
        let work = Workspace::empty();
        work.write(&format!("app/{file}"), "mine");
        let mut argv = vec!["app", "--template", template];
        argv.extend((!kind.is_empty()).then_some(kind));
        let created = run(&argv, &work.root, &home, Os::MacOs, false);
        let error = format!("{:#}", created.result.expect_err("refused"));
        // The path as this host writes it (`\\` on Windows).
        let shown = file
            .split('/')
            .fold(PathBuf::new(), |path, part| path.join(part));
        assert!(
            error.contains(&format!("{} exists", shown.display()))
                && error.contains("move it away first"),
            "{template}: {error}"
        );
        assert_eq!(work.read(&format!("app/{file}")), "mine", "{template}");
        assert!(
            created.runner.specs().is_empty(),
            "{template}: nothing ran, not even nsh init"
        );
    }
}
