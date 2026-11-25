//! `rayx new`'s services and features (UMB-2, d-template): a RayX app's `rayx` dependency lists
//! the features its builder uses beyond the defaults (`theme`, `storage`, `operations`,
//! `history`), and both composition roots, the `rust` root and the `mixed` app's `App.tsx`,
//! register the four services the host requires (platform, suspension, assets, fonts) before the
//! theme service (which keeps new apps following the system's light/dark), storage and history,
//! and operations before history, under a comment saying the host requires the four and each can
//! be replaced by the app's own. The app's README, the CLI's README and `docs/cli/projects.md`
//! say the same.

mod common;

use std::path::{Path, PathBuf};

use clap::Parser;
use common::Workspace;
use rayx_cli::cli::{Cli, Command, NewArgs};
use rayx_cli::host::{Arch, HostFacts, Os, Outcome, Runner};
use rayx_cli::new::{RAYX_FEATURES, create, rayx_dependency};
use rayx_cli::nsh::store::{Kind, Receipt, Store};
use rayx_cli::project::pins::DEFAULTS;
use rayx_cli::setup::FakeMachine;

/// `rayx`'s default features: the four services the host requires.
const DEFAULT_FEATURES: [&str; 4] = ["platform", "suspension", "assets", "fonts"];

/// Each template's composition root: the file and its builder calls, in the order they must come.
const ROOTS: [(&str, &str, [&str; 9]); 2] = [
    (
        "mixed",
        "App.tsx",
        [
            "new RayXAppBuilder()",
            ".addPlatform()",
            ".addSuspension()",
            ".addAssets()",
            ".addFonts()",
            ".addTheme()",
            ".addStorage()",
            ".addOperations()",
            ".addHistory()",
        ],
    ),
    (
        "rust",
        "src/lib.rs",
        [
            "RayXAppBuilder::new()",
            ".add_platform()",
            ".add_suspension()",
            ".add_assets(|_| {})",
            ".add_fonts(|_| {})",
            ".add_theme(|_| {})",
            ".add_storage()",
            ".add_operations()",
            ".add_history()",
        ],
    ),
];

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
            nsh: "0.7.0".into(),
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
                    .with_stdout(format!("nsh 0.7.0 ({})\n", &DEFAULTS.nsh.commit[..7])),
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

/// Where `needle` is in `text`, which must have it once.
fn once(text: &str, needle: &str) -> usize {
    let at = text
        .find(needle)
        .unwrap_or_else(|| panic!("no `{needle}` in\n{text}"));
    assert_eq!(
        text.matches(needle).count(),
        1,
        "`{needle}` twice in\n{text}"
    );
    at
}

/// The service a builder call registers: `.add_assets(|_| {})` and `.addAssets()` are `assets`.
fn service_of(call: &str) -> String {
    let name = call
        .trim_start_matches('.')
        .split('(')
        .next()
        .expect("a name");
    let name = name.strip_prefix("add").expect("an add call");
    let mut snake = String::new();
    for c in name.trim_start_matches('_').chars() {
        if c.is_ascii_uppercase() && !snake.is_empty() {
            snake.push('_');
        }
        snake.push(c.to_ascii_lowercase());
    }
    snake
}

/// The `features` of the `rayx` dependency in a manifest.
fn rayx_features(manifest: &str) -> Vec<String> {
    let manifest: toml::Table = manifest.parse().expect("Cargo.toml parses");
    let rayx = &manifest["dependencies"]["rayx"];
    assert!(
        rayx.get("default-features").is_none(),
        "the defaults stay on: {rayx}"
    );
    rayx["features"]
        .as_array()
        .expect("features")
        .iter()
        .map(|feature| feature.as_str().expect("a name").to_owned())
        .collect()
}

#[test]
fn the_dependency_line_lists_the_features_beyond_the_defaults() {
    assert_eq!(RAYX_FEATURES, ["theme", "storage", "operations", "history"]);
    assert_eq!(
        rayx_dependency(),
        format!(
            "rayx = {{ git = \"https://github.com/rayx-ui/RayX\", tag = \"v{}\", features = [\"theme\", \"storage\", \"operations\", \"history\"] }}",
            env!("CARGO_PKG_VERSION")
        )
    );
    for (template, _, _) in ROOTS {
        let work = Workspace::empty();
        let app = created(&work, template);
        let manifest = read(&app.join("Cargo.toml"));
        assert!(
            manifest.contains(&format!("[dependencies]\n{}\n", rayx_dependency())),
            "{template}: {manifest}"
        );
        let features = rayx_features(&manifest);
        assert_eq!(features, RAYX_FEATURES, "{template}");
        for default in DEFAULT_FEATURES {
            assert!(
                !features.iter().any(|feature| feature == default),
                "{template} lists the default {default}"
            );
        }
    }
}

#[test]
fn both_roots_register_the_four_before_the_theme_storage_and_history() {
    for (template, file, calls) in ROOTS {
        let work = Workspace::empty();
        let app = created(&work, template);
        let root = read(&app.join(file));
        let at: Vec<usize> = calls.iter().map(|call| once(&root, call)).collect();
        assert!(
            at.windows(2).all(|pair| pair[0] < pair[1]),
            "{template}: {calls:?} in order in\n{root}"
        );
        let started = once(&root, ".start()");
        assert!(at[8] < started, "{template}: history before start()");
        // The four the host requires come first, then the theme, operations before history.
        let services: Vec<String> = calls[1..].iter().map(|call| service_of(call)).collect();
        assert_eq!(services[..4], DEFAULT_FEATURES, "{template}");
        assert_eq!(
            services[4..],
            ["theme", "storage", "operations", "history"],
            "{template}"
        );
    }
}

#[test]
fn the_roots_comment_says_the_host_requires_the_four_and_each_is_replaceable() {
    for (template, file, calls) in ROOTS {
        let work = Workspace::empty();
        let app = created(&work, template);
        let root = read(&app.join(file));
        let built = once(&root, calls[0]);
        let start = root[..built]
            .rfind("// The app's composition root")
            .unwrap_or_else(|| panic!("{template}: no comment above the root in\n{root}"));
        let comment = root[start..built]
            .lines()
            .filter(|line| line.trim_start().starts_with("//"))
            .map(|line| line.trim_start().trim_start_matches("//").trim())
            .collect::<Vec<_>>()
            .join(" ");
        for needle in [
            "The host requires the first four (platform state, suspension, assets and fonts)",
            "each can be replaced by the app's own implementation of its contract",
            "the theme keeps the theme's light/dark modifier in step with the system",
            "history needs operations",
        ] {
            assert!(
                comment.contains(needle),
                "{template}: `{needle}` in {comment}"
            );
        }
    }
}

#[test]
fn the_features_are_the_services_the_roots_register_beyond_the_defaults() {
    for (template, _, calls) in ROOTS {
        let registered: Vec<String> = calls[1..].iter().map(|call| service_of(call)).collect();
        let beyond: Vec<&str> = registered
            .iter()
            .map(String::as_str)
            .filter(|service| !DEFAULT_FEATURES.contains(service))
            .collect();
        assert_eq!(beyond, RAYX_FEATURES, "{template}");
    }
}

#[test]
fn the_app_s_readme_and_the_cli_s_docs_say_the_same() {
    for (template, _, _) in ROOTS {
        let work = Workspace::empty();
        let app = created(&work, template);
        let readme = read(&app.join("README.md")).replace("\n  ", " ");
        for needle in [
            "the defaults are the four the host requires (`platform`, `suspension`, `assets`, `fonts`)",
            "`Cargo.toml` lists the others the app uses (`theme`, `storage`, `operations`, `history`)",
            "each replaceable by the app's own, then the theme, storage, operations and history",
        ] {
            assert!(
                readme.contains(needle),
                "{template}: `{needle}` in\n{readme}"
            );
        }
    }
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let joined = |text: String| text.split_whitespace().collect::<Vec<_>>().join(" ");
    let cli_readme = joined(read(&manifest_dir.join("README.md")));
    for needle in [
        "`features = [\"theme\", \"storage\", \"operations\", \"history\"]`; the defaults are the four services the host requires",
        "`.addPlatform().addSuspension().addAssets().addFonts()` (the four the host requires, each replaceable by the app's own) then `.addTheme().addStorage().addOperations().addHistory()`",
        "(`add_platform()`, `add_suspension()`, `add_assets(..)`, `add_fonts(..)`, then `add_theme(..)`, `add_storage()`, `add_operations()`, `add_history()`)",
    ] {
        assert!(cli_readme.contains(needle), "the CLI's README: `{needle}`");
    }
    let projects = joined(read(&manifest_dir.join("../../docs/cli/projects.md")));
    for needle in [
        "`new RayXAppBuilder().addPlatform().addSuspension().addAssets().addFonts().addTheme().addStorage().addOperations().addHistory().start()`",
        "`RayXAppBuilder::new().add_platform().add_suspension().add_assets(..).add_fonts(..).add_theme(..).add_storage().add_operations().add_history().start()`",
        "Both register the four services the host requires (platform, suspension, assets and fonts) before the theme, storage and history",
        "`features = [\"theme\", \"storage\", \"operations\", \"history\"]`",
    ] {
        assert!(
            projects.contains(needle),
            "docs/cli/projects.md: `{needle}`"
        );
    }
}

#[test]
fn the_tsx_root_awaits_the_key_value_store_s_promises() {
    let work = Workspace::empty();
    let app = created(&work, "mixed");
    let root = read(&app.join("App.tsx"));
    for awaited in [
        "await store.get(\"app\", \"launches\")",
        "await store.set(\"app\", \"launches\"",
    ] {
        once(&root, awaited);
    }
    assert!(root.contains(".then(async (services) =>"), "{root}");
    let test = read(&app.join("App.test.tsx"));
    once(&test, "text:Launch 1");
}
