//! `rayx new <dir> [--name <name>] [--template mixed|ts|rust] [--nsh-only]`: a RayX app (or, with
//! `--nsh-only`, a plain NSH app) that names its nsh from its first commit (design §4.7, D5, D7,
//! NEW-1).
//!
//! 1. Install rayx's default pin if needed and run its `nsh init <dir> --name … --template …`.
//! 2. A RayX app (`mixed` and `rust`): `Cargo.toml` depends on `rayx` at this `rayx`'s own tag,
//!    and `src/lib.rs` (with `App.tsx`, `App.test.tsx` and the README) is RayX's: it registers
//!    RayX's TSX classes, or starts the app from the Rust root, and names the app's services
//!    with `set_app`.
//! 3. `mixed` and `rust`: write `.cargo/config.toml` with a `[patch.crates-io]` entry (git URL and
//!    tag) for every NSH crate the default pin patches (RayX's) and every other NSH crate the
//!    app's `Cargo.toml` names, then `cargo generate-lockfile`, so the app's `Cargo.lock` holds
//!    the pin.
//! 4. `ts` (`--nsh-only`): write `rayx.toml` with the default pin's `[nsh]` table.
//! 5. Print the next steps.
//!
//! `--template` takes nsh's own three names; later a name can map to a template rayx fetches,
//! run after step 3.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

use crate::cli::{NewArgs, Template};
use crate::host::{CommandSpec, HostFacts, Mode, Os, Runner};
use crate::nsh::install::{Install, RustTools, without_cargo};
use crate::nsh::pin::{RAYX_TOML, default_pin, is_nsh_crate, rayx_toml_table};
use crate::nsh::store::Store;
use crate::project::pins::{DEFAULTS, DefaultNsh};
use crate::setup::{Machine, SystemMachine};

/// Where a RayX app takes `rayx` from: RayX's repository, at a release tag.
pub const RAYX_GIT: &str = "https://github.com/rayx-ui/RayX";

/// Runs `rayx new` on this machine.
pub fn run(args: &NewArgs) -> u8 {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let host = crate::host::facts();
    let machine = SystemMachine;
    let mut runner = Runner::execute();
    match create(args, &cwd, &host, &machine, &mut runner, &mut io::stdout()) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("rayx: {error:#}");
            1
        }
    }
}

/// The `.cargo/config.toml` that patches to `default`'s git tag its NSH crates (RayX's) and
/// `also`, the other NSH crates the app names.
pub fn cargo_config(default: &DefaultNsh, also: &[String]) -> String {
    let mut text = format!(
        "# The NSH this app is built with: rayx's default pin when the app was created\n\
         # ({} at {}). Cargo.lock records its commit, which is the app's nsh pin;\n\
         # move every line together to change it.\n\n[patch.crates-io]\n",
        default.tag,
        &default.commit[..7]
    );
    let names = default.crates.iter().copied().chain(
        also.iter()
            .map(String::as_str)
            .filter(|name| !default.crates.contains(name)),
    );
    for name in names {
        text.push_str(&format!(
            "{name} = {{ git = \"{}\", tag = \"{}\" }}\n",
            default.git, default.tag
        ));
    }
    text
}

/// The `rayx` dependency of a new RayX app: RayX at this `rayx`'s own release tag.
pub fn rayx_dependency() -> String {
    format!(
        "rayx = {{ git = \"{RAYX_GIT}\", tag = \"v{}\" }}",
        env!("CARGO_PKG_VERSION")
    )
}

/// `manifest` with [`rayx_dependency`] first in its `[dependencies]`.
pub fn with_rayx_dependency(manifest: &str) -> String {
    let dependency = rayx_dependency();
    let mut out = String::with_capacity(manifest.len() + dependency.len() + 1);
    let mut added = false;
    for line in manifest.split_inclusive('\n') {
        out.push_str(line);
        if !added && line.trim() == "[dependencies]" {
            if !line.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&dependency);
            out.push('\n');
            added = true;
        }
    }
    if !added {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&format!("\n[dependencies]\n{dependency}\n"));
    }
    out
}

/// The NSH crates (nsh's own list) a `Cargo.toml` depends on, in any dependency table, sorted.
pub fn nsh_dependencies(manifest: &str) -> Result<Vec<String>> {
    let manifest: toml::Table = manifest.parse().context("Cargo.toml does not parse")?;
    let mut tables: Vec<&toml::Table> = Vec::new();
    for kind in ["dependencies", "dev-dependencies", "build-dependencies"] {
        tables.extend(manifest.get(kind).and_then(toml::Value::as_table));
        if let Some(targets) = manifest.get("target").and_then(toml::Value::as_table) {
            tables.extend(
                targets
                    .values()
                    .filter_map(|target| target.get(kind).and_then(toml::Value::as_table)),
            );
        }
    }
    let mut names: Vec<String> = tables
        .into_iter()
        .flat_map(|table| {
            table.iter().map(|(key, spec)| {
                spec.get("package")
                    .and_then(toml::Value::as_str)
                    .unwrap_or(key)
                    .to_string()
            })
        })
        .filter(|name| is_nsh_crate(name))
        .collect();
    names.sort();
    names.dedup();
    Ok(names)
}

/// The import specifier `nsh init` gave the app's crate: the `#[native_module("…")]` of its
/// `src/lib.rs`.
pub fn native_module_specifier(lib: &str) -> Option<&str> {
    let (_, rest) = lib.split_once("#[native_module(\"")?;
    let (specifier, _) = rest.split_once('"')?;
    Some(specifier)
}

/// RayX's templates, filled for one app.
struct Templates {
    specifier: String,
    name: String,
}

impl Templates {
    fn fill(&self, text: &str) -> String {
        text.replace("__SPECIFIER__", &self.specifier)
            .replace("__APP_NAME__", &self.name)
            .replace("__RAYX_TAG__", &format!("v{}", env!("CARGO_PKG_VERSION")))
    }

    fn readme(&self, about: &str) -> String {
        self.fill(&include_str!("new/README.md.in").replace("__TEMPLATE__", about))
    }
}

/// Makes the app `nsh init` wrote in `dir` a RayX app: `rayx` in its `Cargo.toml`, and RayX's
/// `src/lib.rs`, tests and README in place of nsh's.
fn make_rayx_app(dir: &Path, template: Template, runner: &Runner) -> Result<()> {
    if runner.mode() == Mode::Print {
        return Ok(());
    }
    let read = |file: &str| {
        std::fs::read_to_string(dir.join(file))
            .with_context(|| format!("nsh init wrote no readable {file} in {}", dir.display()))
    };
    let lib = read("src/lib.rs")?;
    let specifier = native_module_specifier(&lib)
        .ok_or_else(|| anyhow!("nsh init's src/lib.rs names no #[native_module(\"…\")]"))?
        .to_string();
    let name = serde_json::from_str::<serde_json::Value>(&read("nsh.json")?)
        .ok()
        .and_then(|config| config["name"].as_str().map(str::to_string))
        .unwrap_or_else(|| specifier.trim_end_matches("/native").to_string());
    let templates = Templates { specifier, name };
    write(
        &dir.join("Cargo.toml"),
        &with_rayx_dependency(&read("Cargo.toml")?),
        runner,
    )?;
    let files: Vec<(&str, String)> =
        match template {
            Template::Mixed => {
                vec![
            ("src/lib.rs", templates.fill(include_str!("new/mixed_lib.rs.in"))),
            ("App.tsx", templates.fill(include_str!("new/mixed_App.tsx.in"))),
            (
                "App.test.tsx",
                templates.fill(include_str!("new/mixed_App.test.tsx.in")),
            ),
            (
                "README.md",
                templates.readme(&format!(
                    "`App.tsx` is the app; the directory is also its Rust crate (`Cargo.toml`, \
                     `src/lib.rs`), whose `#[native_module(\"{}\")]` registers RayX's TSX classes \
                     (`AppServices`, `ServiceScope`, `I18n`) and names the app's services.",
                    templates.specifier
                )),
            ),
        ]
            }
            Template::Rust => vec![
                (
                    "src/lib.rs",
                    templates.fill(include_str!("new/rust_lib.rs.in")),
                ),
                (
                    "App.test.tsx",
                    templates.fill(include_str!("new/rust_App.test.tsx.in")),
                ),
                (
                    "README.md",
                    templates.readme(
                        "The app is Rust: `src/lib.rs`'s root component `App` starts the app's \
                     services and shows them, and `cargo run` runs it on this machine's desktop.",
                    ),
                ),
            ],
            Template::Ts => {
                bail!("a RayX app has a Rust crate; `--template ts` needs `--nsh-only`")
            }
        };
    for (file, text) in files {
        write(&dir.join(file), &text, runner)?;
    }
    Ok(())
}

/// The `rayx.toml` of a `ts` app.
pub fn rayx_toml(default: &DefaultNsh) -> String {
    format!(
        "# The nsh this app runs with: it has no Rust crates, so no Cargo.lock names it.\n{}",
        rayx_toml_table(default)
    )
}

/// The body of `rayx new` over injectable collaborators; progress and the next steps on `out`.
pub fn create(
    args: &NewArgs,
    cwd: &Path,
    host: &HostFacts,
    machine: &dyn Machine,
    runner: &mut Runner,
    out: &mut dyn Write,
) -> Result<()> {
    let dir = cwd.join(&args.dir);
    if args.template == Template::Ts && !args.nsh_only {
        bail!(
            "a RayX app has a Rust crate that registers RayX's TSX classes, which `--template ts` \
             has not: use `--template mixed` (the default) or `rust`, or `--nsh-only` for an NSH \
             app without RayX"
        );
    }
    // rayx pins the app in one file; one that is already there is the user's, and nsh's own list
    // of conflicting files does not name it, so nothing starts.
    let pin_file = match args.template {
        Template::Mixed | Template::Rust => dir.join(".cargo").join("config.toml"),
        Template::Ts => dir.join(RAYX_TOML),
    };
    if pin_file.exists() {
        bail!(
            "{} exists, and `rayx new` would replace it with the app's nsh pin; move it away first",
            pin_file.display()
        );
    }
    let store = Store::locate(machine)
        .ok_or_else(|| anyhow::anyhow!("no home directory for the nsh store (set RAYX_HOME)"))?;
    let pin = default_pin(cwd);
    // As setup's Rust steps find them: a shell opened before `rayx setup` installed Rust has no
    // `~/.cargo/bin` on its PATH yet.
    let tools = RustTools::find(machine, host.os);
    let installed = Install {
        runner,
        store: &store,
        host,
        tools: &tools,
        out,
    }
    .run(&pin)?;

    // nsh refuses a directory with files in it, and then nothing below is written.
    let mut init = CommandSpec::new(installed.cli.display().to_string())
        .arg("init")
        .arg(dir.display().to_string())
        .args(["--template", args.template.name()]);
    if let Some(name) = &args.name {
        init = init.args(["--name", name.as_str()]);
    }
    runner
        .run_checked(&without_cargo(init).cwd(cwd).interactive())
        .context("nsh init")?;

    match args.template {
        Template::Mixed | Template::Rust => {
            if !dir.join("Cargo.toml").is_file() && runner.mode() != Mode::Print {
                bail!(
                    "nsh init wrote no Cargo.toml in {}, so the pin cannot go into a Cargo.lock",
                    dir.display()
                );
            }
            if !args.nsh_only {
                make_rayx_app(&dir, args.template, runner)?;
            }
            // A dry run has no manifest to read: it patches the default's crates.
            let also = match std::fs::read_to_string(dir.join("Cargo.toml")) {
                Ok(manifest) => nsh_dependencies(&manifest)?,
                Err(_) => Vec::new(),
            };
            write(
                &dir.join(".cargo").join("config.toml"),
                &cargo_config(&DEFAULTS.nsh, &also),
                runner,
            )?;
            runner
                .run_checked(
                    &without_cargo(CommandSpec::new(tools.cargo.as_str()).arg("generate-lockfile"))
                        .cwd(&dir)
                        .interactive(),
                )
                .context("cargo generate-lockfile")?;
        }
        Template::Ts => write(&dir.join(RAYX_TOML), &rayx_toml(&DEFAULTS.nsh), runner)?,
    }

    let shown = args.dir.display();
    let kind = if args.nsh_only {
        ",".to_string()
    } else {
        format!(", a RayX app on rayx v{},", env!("CARGO_PKG_VERSION"))
    };
    let _ = writeln!(
        out,
        "\nCreated {shown}{kind} pinned to nsh {}. Next:",
        pin.label()
    );
    let _ = writeln!(out, "  cd {shown}");
    let web = if host.os == Os::MacOs {
        "  rayx setup --web --ios"
    } else {
        "  rayx setup --web"
    };
    let _ = writeln!(out, "{web}");
    let _ = writeln!(out, "  rayx app run --target web");
    Ok(())
}

/// Writes a file unless this is a dry run.
fn write(path: &Path, text: &str, runner: &Runner) -> Result<()> {
    if runner.mode() == Mode::Print {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}
