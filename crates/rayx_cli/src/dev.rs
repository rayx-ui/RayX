//! `rayx dev`: the prebuilt CLIs for working on NSH and RayX themselves, kept in one local folder
//! (`$RAYX_HOME/dev`, else `~/.rayx/dev`), outside every repository, so every checkout and
//! worktree on the machine runs the same ones.
//!
//! - `rayx dev install nsh|rayx [<checkout>]` builds the checkout's CLI (`nsh-cli`, `rayx-cli`)
//!   in its own `target/quick/` with [`QUICK_PROFILE`] and puts the binary in the folder in one
//!   step, beside a record of the checkout and commit it came from (`<tool>.json`). It is the only
//!   place either CLI is compiled, and it runs when someone asks: after a change to the CLI's code.
//! - A project whose NSH is a local checkout (`rayx nshx use <checkout>`) runs the folder's `nsh`
//!   ([`checkout_cli`]); `rayx app` never builds a CLI when it runs one. A change to NSH's
//!   libraries needs no new CLI; one to the CLI's own code ([`NSH_CLI_SOURCES`]) is warned of.
//! - A design tool runs the folder's `rayx` (RayX Studio: `RAYX_CLI`, else this one).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::cli::{DevCommand, DevTool};
use crate::host::{CommandSpec, HostFacts, Os, Runner};
use crate::nsh::install::{RustTools, in_nsh_checkout};
use crate::nsh::store::{Store, rfc3339};
use crate::nsh::switch::{checkout_state, local_checkout};
use crate::setup::{Machine, SystemMachine};

/// The cargo profile `rayx dev install` builds with, into the checkout's `target/quick/`:
/// release's optimizations without link-time optimization, in 16 codegen units, built
/// incrementally. After a change in NSH it rebuilds `nsh-cli` in seconds where NSH's release
/// profile (`lto = "fat"`, one codegen unit) takes minutes, and the CLI bundles as fast.
pub const QUICK_PROFILE: &str = "quick";

/// [`QUICK_PROFILE`], defined in the build's environment (`CARGO_PROFILE_QUICK_*`): no checkout
/// needs a profile of its own for it.
pub fn quick_profile_env() -> Vec<(String, String)> {
    [
        ("INHERITS", "release"),
        ("LTO", "off"),
        ("CODEGEN_UNITS", "16"),
        ("INCREMENTAL", "true"),
    ]
    .into_iter()
    .map(|(key, value)| (format!("CARGO_PROFILE_QUICK_{key}"), value.to_string()))
    .collect()
}

/// The folders of an NSH checkout whose code the CLI runs when it runs an app: a change in one of
/// them since the prebuilt `nsh`'s commit asks for `rayx dev install nsh`.
pub const NSH_CLI_SOURCES: [&str; 4] = [
    "crates/nsh-cli",
    "crates/nsh-bundler",
    "crates/nsh-dev",
    "crates/nsh-package",
];

/// A CLI the folder keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    /// NSH's CLI, from an NSH checkout (the fork's).
    Nsh,
    /// This CLI, from a RayX checkout.
    Rayx,
}

impl Tool {
    pub const ALL: [Tool; 2] = [Tool::Nsh, Tool::Rayx];

    pub fn name(self) -> &'static str {
        match self {
            Tool::Nsh => "nsh",
            Tool::Rayx => "rayx",
        }
    }

    /// The package that builds it.
    pub fn package(self) -> &'static str {
        match self {
            Tool::Nsh => "nsh-cli",
            Tool::Rayx => "rayx-cli",
        }
    }

    /// The manifest that makes a folder its checkout.
    fn marker(self) -> [&'static str; 3] {
        match self {
            Tool::Nsh => ["crates", "nsh-cli", "Cargo.toml"],
            Tool::Rayx => ["crates", "rayx_cli", "Cargo.toml"],
        }
    }

    /// Whether `dir` is a checkout of it.
    pub fn is_checkout(self, dir: &Path) -> bool {
        self.marker()
            .iter()
            .fold(dir.to_path_buf(), |path, part| path.join(part))
            .is_file()
    }

    /// Its file name on `os`.
    pub fn exe(self, os: Os) -> String {
        if os == Os::Windows {
            format!("{}.exe", self.name())
        } else {
            self.name().to_string()
        }
    }
}

/// What the folder keeps beside a CLI (`<tool>.json`): where and when it was built.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub version: u32,
    pub tool: String,
    /// The checkout it was built in.
    pub checkout: PathBuf,
    /// The checkout's commit (12 characters), when git could say.
    pub commit: Option<String>,
    /// How many files the checkout had uncommitted, when git could say.
    pub uncommitted: Option<usize>,
    pub profile: String,
    /// When it was put in the folder (RFC 3339, UTC).
    pub installed: String,
}

/// The folder of prebuilt CLIs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dev {
    pub root: PathBuf,
}

impl Dev {
    /// `$RAYX_HOME/dev`, else `~/.rayx/dev`: beside the nsh store (`nsh::store::Store::locate`).
    pub fn locate(machine: &dyn Machine) -> Option<Dev> {
        let home = machine
            .env("RAYX_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| machine.home().map(|home| home.join(".rayx")))?;
        Some(Dev::at(home.join("dev")))
    }

    pub fn at(root: impl Into<PathBuf>) -> Dev {
        Dev { root: root.into() }
    }

    /// The folder beside the nsh store `store` (`<home>/nsh` and `<home>/dev`).
    pub fn beside(store: &Store) -> Dev {
        Dev::at(
            store
                .root
                .parent()
                .map_or_else(|| store.root.join("dev"), |home| home.join("dev")),
        )
    }

    /// The prebuilt CLI of `tool` (it may be missing).
    pub fn cli(&self, tool: Tool, os: Os) -> PathBuf {
        self.root.join(tool.exe(os))
    }

    fn record_path(&self, tool: Tool) -> PathBuf {
        self.root.join(format!("{}.json", tool.name()))
    }

    /// The record of `tool`'s CLI; `None` when there is none or it cannot be read.
    pub fn record(&self, tool: Tool) -> Option<Record> {
        let text = fs::read_to_string(self.record_path(tool)).ok()?;
        serde_json::from_str(&text).ok()
    }
}

impl From<DevTool> for Tool {
    fn from(tool: DevTool) -> Tool {
        match tool {
            DevTool::Nsh => Tool::Nsh,
            DevTool::Rayx => Tool::Rayx,
        }
    }
}

/// Runs `rayx dev` on this machine and returns the exit code.
pub fn run(command: &DevCommand) -> u8 {
    let host = crate::host::facts();
    let machine = SystemMachine;
    let Some(dev) = Dev::locate(&machine) else {
        eprintln!("rayx: no home directory for the prebuilt CLIs (set RAYX_HOME)");
        return 1;
    };
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut out = std::io::stdout();
    let result = match command {
        DevCommand::Install { tool, checkout } => {
            let tool = Tool::from(*tool);
            let checkout = match checkout {
                Some(given) => Ok(cwd.join(given)),
                None => find_checkout(tool, &cwd),
            };
            checkout.and_then(|checkout| {
                install(
                    tool,
                    &checkout,
                    &dev,
                    &mut Runner::execute(),
                    &machine,
                    &host,
                    &mut out,
                )
                .map(|_| 0)
            })
        }
        DevCommand::Status { json } => status(&dev, &host, *json, &mut out),
    };
    match result {
        Ok(code) => code,
        Err(error) => {
            eprintln!("rayx: {error:#}");
            1
        }
    }
}

/// The checkout `rayx dev install <tool>` builds when none is named: `start` or the nearest
/// folder above it that is one; for nsh then the checkout `rayx nshx use local` takes.
pub fn find_checkout(tool: Tool, start: &Path) -> Result<PathBuf> {
    if let Some(found) = start.ancestors().find(|dir| tool.is_checkout(dir)) {
        return Ok(found.to_path_buf());
    }
    match tool {
        Tool::Nsh => local_checkout(start),
        Tool::Rayx => bail!(
            "{} is in no RayX checkout (a folder holding crates/rayx_cli); name the checkout",
            start.display()
        ),
    }
}

/// `rayx dev install <tool> <checkout>`: builds the checkout's CLI with [`QUICK_PROFILE`] and
/// puts it in `dev` with its record, replacing the one there in one rename (a running copy keeps
/// running).
pub fn install(
    tool: Tool,
    checkout: &Path,
    dev: &Dev,
    runner: &mut Runner,
    machine: &dyn Machine,
    host: &HostFacts,
    out: &mut dyn Write,
) -> Result<Record> {
    if !tool.is_checkout(checkout) {
        bail!(
            "{} is not a checkout of {}: it has no {}",
            checkout.display(),
            tool.name(),
            tool.marker().join("/")
        );
    }
    let checkout = fs::canonicalize(checkout).unwrap_or_else(|_| checkout.to_path_buf());
    let _ = writeln!(
        out,
        "rayx: building {} in {} ({QUICK_PROFILE} build)",
        tool.package(),
        checkout.display()
    );
    let cargo = RustTools::find(machine, host.os).cargo;
    // The checkout's own toolchain (`rust-toolchain.toml`), without the cargo that runs rayx.
    let mut build = in_nsh_checkout(CommandSpec::new(cargo.as_str()).args([
        "build",
        "--profile",
        QUICK_PROFILE,
        "-p",
        tool.package(),
        "--target-dir",
        "target",
    ]));
    for (key, value) in quick_profile_env() {
        build = build.env(key, value);
    }
    runner
        .run_checked(&build.cwd(&checkout).interactive())
        .with_context(|| format!("building {} in {}", tool.package(), checkout.display()))?;
    let built = checkout
        .join("target")
        .join(QUICK_PROFILE)
        .join(tool.exe(host.os));
    if !built.is_file() {
        bail!("the build made no {}", built.display());
    }
    fs::create_dir_all(&dev.root).with_context(|| format!("creating {}", dev.root.display()))?;
    let cli = dev.cli(tool, host.os);
    let staged = dev.root.join(format!(".{}.new", tool.exe(host.os)));
    fs::copy(&built, &staged).with_context(|| format!("copying {}", built.display()))?;
    fs::rename(&staged, &cli).with_context(|| format!("placing {}", cli.display()))?;
    let state = checkout_state(runner, &checkout);
    let record = Record {
        version: 1,
        tool: tool.name().to_string(),
        checkout: checkout.clone(),
        commit: state.commit.clone(),
        uncommitted: state.uncommitted,
        profile: QUICK_PROFILE.to_string(),
        installed: rfc3339(SystemTime::now()),
    };
    let text = serde_json::to_string_pretty(&record).expect("a record serializes");
    let staged = dev.root.join(format!(".{}.json.new", tool.name()));
    fs::write(&staged, text + "\n").with_context(|| format!("writing {}", staged.display()))?;
    fs::rename(&staged, dev.record_path(tool))
        .with_context(|| format!("placing {}", dev.record_path(tool).display()))?;
    let _ = writeln!(
        out,
        "rayx: {} is now {}, built from {}{}",
        tool.name(),
        cli.display(),
        checkout.display(),
        state.describe()
    );
    Ok(record)
}

/// `rayx dev status`: each CLI the folder keeps, with where it came from.
pub fn status(dev: &Dev, host: &HostFacts, json: bool, out: &mut dyn Write) -> Result<u8> {
    if json {
        let tools: Vec<_> = Tool::ALL
            .iter()
            .map(|&tool| {
                let cli = dev.cli(tool, host.os);
                json!({
                    "tool": tool.name(),
                    "cli": cli.display().to_string(),
                    "present": cli.is_file(),
                    "record": dev.record(tool),
                })
            })
            .collect();
        let value = json!({ "folder": dev.root.display().to_string(), "tools": tools });
        writeln!(out, "{}", serde_json::to_string_pretty(&value)?)?;
        return Ok(0);
    }
    writeln!(out, "prebuilt CLIs in {}", dev.root.display())?;
    for tool in Tool::ALL {
        let cli = dev.cli(tool, host.os);
        if !cli.is_file() {
            writeln!(
                out,
                "  {:<5} none: `rayx dev install {}` builds it",
                tool.name(),
                tool.name()
            )?;
            continue;
        }
        let from = match dev.record(tool) {
            Some(record) => format!(
                "from {}{}, installed {}",
                record.checkout.display(),
                at(&record),
                record.installed
            ),
            None => "no record of where it came from".to_string(),
        };
        writeln!(out, "  {:<5} {} ({from})", tool.name(), cli.display())?;
    }
    Ok(0)
}

/// ` at <commit>`, with its uncommitted files when it had any.
fn at(record: &Record) -> String {
    match (&record.commit, record.uncommitted) {
        (Some(commit), Some(0) | None) => format!(" at {commit}"),
        (Some(commit), Some(n)) => format!(
            " at {commit} with {n} uncommitted file{}",
            if n == 1 { "" } else { "s" }
        ),
        (None, _) => String::new(),
    }
}

/// The CLI `rayx` runs for a project whose NSH is the local checkout `checkout`: the folder's
/// prebuilt `nsh`, never a build. A missing one is an error naming `rayx dev install nsh`; one built
/// from another checkout, or before a change to the CLI's code ([`NSH_CLI_SOURCES`]), is
/// warned of on `err`.
pub fn checkout_cli(
    checkout: &Path,
    dev: &Dev,
    runner: &mut Runner,
    host: &HostFacts,
    err: &mut dyn Write,
) -> Result<PathBuf> {
    let cli = dev.cli(Tool::Nsh, host.os);
    if !cli.is_file() {
        return Err(anyhow!(
            "no prebuilt nsh in {} for the NSH checkout {}: build it once with \
             `rayx dev install nsh {}` (rayx never builds a CLI when it runs one)",
            dev.root.display(),
            checkout.display(),
            checkout.display()
        ));
    }
    let Some(record) = dev.record(Tool::Nsh) else {
        return Ok(cli);
    };
    let canonical = |path: &Path| fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if canonical(&record.checkout) != canonical(checkout) {
        let _ = writeln!(
            err,
            "rayx: the prebuilt nsh was built from {}, not from the project's NSH checkout {}",
            record.checkout.display(),
            checkout.display()
        );
        return Ok(cli);
    }
    if let Some(commit) = &record.commit {
        // Exit 1: the CLI's code differs between that commit and the checkout as it is now.
        let diff = CommandSpec::new("git")
            .args(["diff", "--quiet", commit.as_str(), "--"])
            .args(NSH_CLI_SOURCES)
            .cwd(checkout);
        if runner
            .query(&diff)
            .is_ok_and(|outcome| outcome.code == Some(1))
        {
            let _ = writeln!(
                err,
                "rayx: NSH's CLI code changed since the prebuilt nsh ({commit}); \
                 `rayx dev install nsh {}` brings it up to date",
                checkout.display()
            );
        }
    }
    Ok(cli)
}

/// Set on the prebuilt `rayx` a hand-over starts ([`hand_over_to`]), so it never hands over again.
pub const HANDED_OVER: &str = "RAYX_HANDED_OVER";

/// `1` keeps the installed `rayx` from handing over: it runs itself.
pub const NO_HANDOVER: &str = "RAYX_NO_DEV_HANDOVER";

/// The prebuilt `rayx` the running one hands its command line over to, if any.
///
/// The installed `rayx` (`<home>/bin/rayx`, which the release installers put first on `PATH`)
/// may be older than the prebuilt one in `<home>/dev` (`rayx dev install rayx`), and an older one
/// builds NSH's CLI from a local checkout where the prebuilt one runs its `nsh`. So the installed
/// `rayx` runs the prebuilt one in its place whenever there is one: a stale release on `PATH`
/// never decides what a checkout's commands build. It does not hand over `rayx self …` (which
/// updates the installed binary), when [`NO_HANDOVER`] is `1`, when it was itself started by a
/// hand-over ([`HANDED_OVER`]), or when it is the prebuilt one (a link to it included).
/// `canonical` resolves links (`fs::canonicalize`; the path itself when that fails).
pub fn hand_over_to(
    machine: &dyn Machine,
    os: Os,
    current: &Path,
    first_arg: Option<&std::ffi::OsStr>,
    canonical: impl Fn(&Path) -> PathBuf,
) -> Option<PathBuf> {
    let set = |name: &str| machine.env(name).filter(|value| !value.is_empty());
    if set(HANDED_OVER).is_some() || set(NO_HANDOVER).as_deref() == Some("1") {
        return None;
    }
    if first_arg.is_some_and(|arg| arg == "self") {
        return None;
    }
    let home = set("RAYX_HOME")
        .map(PathBuf::from)
        .or_else(|| machine.home().map(|home| home.join(".rayx")))?;
    let installed = home.join("bin").join(Tool::Rayx.exe(os));
    let prebuilt = Dev::at(home.join("dev")).cli(Tool::Rayx, os);
    let current = canonical(current);
    if current != canonical(&installed) || !machine.exists(&prebuilt) {
        return None;
    }
    (canonical(&prebuilt) != current).then_some(prebuilt)
}

/// Runs the prebuilt `rayx` in place of this process when [`hand_over_to`] says so, with this
/// command line, and returns its exit code; `None` when this process runs the command itself.
pub fn hand_over() -> Option<std::process::ExitCode> {
    let current = std::env::current_exe().ok()?;
    let mut args = std::env::args_os().skip(1);
    let first = args.next();
    let os = if cfg!(windows) {
        Os::Windows
    } else if cfg!(target_os = "macos") {
        Os::MacOs
    } else {
        Os::Linux
    };
    let canonical = |path: &Path| fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let prebuilt = hand_over_to(&SystemMachine, os, &current, first.as_deref(), canonical)?;
    let mut command = std::process::Command::new(&prebuilt);
    command
        .args(first.into_iter().chain(args))
        .env(HANDED_OVER, "1");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // `exec` returns only when it failed: then this `rayx` runs the command itself.
        let error = command.exec();
        eprintln!("rayx: could not run {}: {error}", prebuilt.display());
        None
    }
    #[cfg(not(unix))]
    {
        match command.status() {
            Ok(status) => Some(std::process::ExitCode::from(
                u8::try_from(status.code().unwrap_or(1)).unwrap_or(1),
            )),
            Err(error) => {
                eprintln!("rayx: could not run {}: {error}", prebuilt.display());
                None
            }
        }
    }
}
