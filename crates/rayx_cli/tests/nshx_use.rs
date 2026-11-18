//! `rayx nshx use <checkout|tag>` (design §4.6, fork development): a project's NSH patch lines
//! point at a local NSH checkout and back at a tag, `Cargo.lock` follows, `rayx nshx status` shows
//! the checkout, and in RayX's own repository a new tag moves every file that names the default
//! pin. The checkout cases run Cargo offline on a temporary project and a fake NSH checkout; the
//! tag cases answer `cargo metadata` themselves, so nothing is fetched.

mod common;

use std::path::Path;

use common::Workspace;
use rayx_cli::host::{Arch, CommandSpec, HostFacts, Os, Outcome, Runner};
use rayx_cli::nsh::status_in;
use rayx_cli::nsh::store::Store;
use rayx_cli::nsh::switch::{
    PinText, Target, WAS, checkout_line, move_default, moved_text, rayx_default, rewrite_patch,
    tag_line, target, use_in,
};
use rayx_cli::project::pins::DEFAULTS;

const FORK: &str = "https://github.com/dXnSix/nsh";
const OLD_COMMIT: &str = "d47d7fd6dfdcb1d3ad11400d121b91a9d63bbf85";
const NEW_COMMIT: &str = "c9bf1fe9a2b14c3e5d6f708192a3b4c5d6e7f809";

/// A fake NSH checkout in `nsh/`: `crates/nsh-cli` and `nsh-core` at `version`.
fn checkout(workspace: &Workspace, version: &str) {
    workspace.write(
        "nsh/crates/nsh-cli/Cargo.toml",
        "[package]\nname = \"nsh-cli\"\nversion = \"0.5.0\"\nedition = \"2021\"\n",
    );
    workspace.write(
        "nsh/crates/nsh-core/Cargo.toml",
        &format!("[package]\nname = \"nsh-core\"\nversion = \"{version}\"\nedition = \"2021\"\n"),
    );
    workspace.write("nsh/crates/nsh-core/src/lib.rs", "pub fn core() {}\n");
}

/// The project's `Cargo.toml`, as RayX's: NSH from crates.io, patched to the fork's tag.
const MANIFEST: &str = "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
[workspace]\n\n[dependencies]\nnsh-core = \"0.5.0\"\n\n\
# NSH comes from an integration tag of the fork.\n[patch.crates-io]\n\
nsh-core = { git = \"https://github.com/dXnSix/nsh\", tag = \"nshx-0.5.0-7\" }\n";

/// A project in `app/` beside the checkout, whose Cargo never goes to the network.
fn project(workspace: &Workspace) {
    workspace.write("app/Cargo.toml", MANIFEST);
    workspace.write("app/src/lib.rs", "");
    workspace.write("app/.cargo/config.toml", "[net]\noffline = true\n");
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

fn slashed(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[test]
fn a_folder_is_a_checkout_and_anything_else_a_tag() {
    let workspace = Workspace::empty();
    checkout(&workspace, "0.5.0");
    workspace.write("other/README.md", "not NSH\n");
    assert_eq!(
        target("nsh", &workspace.root).expect("a checkout"),
        Target::Checkout(workspace.path("nsh"))
    );
    let error = target("other", &workspace.root).expect_err("no crates/nsh-cli");
    assert!(
        error.to_string().contains("is not an NSH checkout"),
        "{error}"
    );
    assert_eq!(
        target("nshx-0.5.0-8", &workspace.root).expect("a tag"),
        Target::Tag("nshx-0.5.0-8".into())
    );
    let error = target("../no/such/folder", &workspace.root).expect_err("neither");
    assert!(
        error
            .to_string()
            .contains("neither an NSH checkout folder nor a tag"),
        "{error}"
    );
}

#[test]
fn only_nsh_patch_lines_move_and_a_round_trip_gives_the_same_text() {
    let text = "[dependencies]\n\
        nsh-core = { git = \"https://github.com/dXnSix/nsh\", tag = \"nshx-0.5.0-7\" }\n\n\
        [patch.crates-io]\n\
        nsh-core = { git = \"https://github.com/dXnSix/nsh\", tag = \"nshx-0.5.0-7\" }\r\n\
        nsh-app-ui = { path = \"ui\" }\n\
        serde = { git = \"https://github.com/serde-rs/serde\", tag = \"v1\" }\n";
    let dir = Path::new("/work/nsh/crates/nsh-core");
    let there = rewrite_patch(text, |line| Some(checkout_line(line, dir)));
    assert_eq!((there.seen, there.changed), (1, 1), "{}", there.text);
    let patched = format!(
        "nsh-core = {{ path = \"{}\" }} {WAS}{{ git = \"{FORK}\", tag = \"nshx-0.5.0-7\" }}\r\n",
        slashed(dir)
    );
    assert!(there.text.contains(&patched), "{}", there.text);
    assert!(
        there.text.starts_with(
            "[dependencies]\nnsh-core = { git = \"https://github.com/dXnSix/nsh\", tag = \"nshx-0.5.0-7\" }\n"
        ),
        "a dependency outside [patch.crates-io] stays: {}",
        there.text
    );
    assert!(there.text.contains("nsh-app-ui = { path = \"ui\" }\n"));
    assert!(
        there
            .text
            .contains("serde = { git = \"https://github.com/serde-rs/serde\"")
    );

    let back = rewrite_patch(&there.text, |line| tag_line(line, "nshx-0.5.0-7"));
    assert_eq!(
        back.text, text,
        "back at the same tag, the text is as it was"
    );
    let moved = rewrite_patch(&there.text, |line| tag_line(line, "nshx-0.5.0-8"));
    assert!(
        moved.text.contains(
            "nsh-core = { git = \"https://github.com/dXnSix/nsh\", tag = \"nshx-0.5.0-8\" }\r\n"
        ),
        "{}",
        moved.text
    );
}

#[test]
fn use_points_the_project_at_the_checkout_and_status_shows_it() {
    let workspace = Workspace::empty();
    checkout(&workspace, "0.5.0");
    project(&workspace);
    let mut out = Vec::new();
    let code = use_in(
        &mut Runner::execute(),
        &workspace.path("app"),
        &workspace.root,
        "nsh",
        &mut out,
    )
    .expect("use the checkout");
    assert_eq!(code, 0);
    let said = String::from_utf8(out).expect("UTF-8");
    assert!(said.contains("builds NSH from"), "{said}");
    assert!(
        said.contains("`rayx nshx use nshx-0.5.0-7` moves them back"),
        "{said}"
    );

    let manifest = workspace.read("app/Cargo.toml");
    let line = format!(
        "nsh-core = {{ path = \"{}\" }} {WAS}{{ git = \"{FORK}\", tag = \"nshx-0.5.0-7\" }}",
        slashed(&workspace.path("nsh/crates/nsh-core"))
    );
    assert!(manifest.contains(&line), "{manifest}");
    let lock = workspace.read("app/Cargo.lock");
    let core = lock
        .split("[[package]]")
        .find(|package| package.contains("name = \"nsh-core\""))
        .expect("nsh-core is locked");
    assert!(!core.contains("source ="), "locked by path: {core}");

    let mut status = Vec::new();
    status_in(
        &mut Runner::print(),
        &Store::at(workspace.path("rayx-home/nsh")),
        &mac(),
        &workspace.path("app"),
        false,
        &mut status,
    )
    .expect("status");
    let status = String::from_utf8(status).expect("UTF-8");
    assert!(status.contains("  checkout   "), "{status}");
    assert!(
        status.contains("  back       rayx nshx use nshx-0.5.0-7"),
        "{status}"
    );
}

#[test]
fn a_checkout_cargo_refuses_leaves_the_project_as_it_was() {
    let workspace = Workspace::empty();
    checkout(&workspace, "0.6.0");
    project(&workspace);
    let error = use_in(
        &mut Runner::execute(),
        &workspace.path("app"),
        &workspace.root,
        "nsh",
        &mut Vec::new(),
    )
    .expect_err("nsh-core 0.6.0 does not meet the project's 0.5.0");
    assert!(
        format!("{error:#}").contains("Cargo cannot build NSH from the checkout"),
        "{error:#}"
    );
    assert_eq!(workspace.read("app/Cargo.toml"), MANIFEST);
}

#[test]
fn a_checkout_without_a_patched_crate_is_refused_before_anything_changes() {
    let workspace = Workspace::empty();
    checkout(&workspace, "0.5.0");
    project(&workspace);
    let manifest = format!(
        "{MANIFEST}nsh-base = {{ git = \"https://github.com/dXnSix/nsh\", tag = \"nshx-0.5.0-7\" }}\n"
    );
    workspace.write("app/Cargo.toml", &manifest);
    let error = use_in(
        &mut Runner::execute(),
        &workspace.path("app"),
        &workspace.root,
        "nsh",
        &mut Vec::new(),
    )
    .expect_err("the checkout has no nsh-base");
    assert!(
        error.to_string().contains("has none of nsh-base"),
        "{error}"
    );
    assert_eq!(workspace.read("app/Cargo.toml"), manifest);
}

/// A runner that runs `cargo locate-project`, `cargo metadata --no-deps` and `git ls-files` for
/// real and answers the `cargo metadata` that locks as Cargo would after locking NSH at `tag`: by
/// writing `Cargo.lock`.
fn locking_at(tag: &'static str, lock: std::path::PathBuf) -> Runner {
    Runner::record().responder(move |spec: &CommandSpec| {
        let first = spec.args.first().map(String::as_str);
        let resolves = !spec.args.iter().any(|arg| arg == "--no-deps");
        match (spec.program.as_str(), first) {
            ("cargo", Some("metadata")) if resolves => {
                let text = format!(
                    "version = 4\n\n[[package]]\nname = \"nsh-core\"\nversion = \"0.5.0\"\n\
                     source = \"git+{FORK}?tag={tag}#{NEW_COMMIT}\"\n"
                );
                std::fs::write(&lock, text).expect("write the lock");
                Some(Outcome::success().with_stdout("{}"))
            }
            ("cargo", Some("locate-project" | "metadata")) | ("git", Some("ls-files")) => {
                let mut command = std::process::Command::new(&spec.program);
                command.args(&spec.args);
                if let Some(cwd) = &spec.cwd {
                    command.current_dir(cwd);
                }
                let output = command.output().expect("run");
                Some(Outcome {
                    code: output.status.code(),
                    stdout: String::from_utf8_lossy(&output.stdout).into(),
                    stderr: String::from_utf8_lossy(&output.stderr).into(),
                })
            }
            _ => None,
        }
    })
}

#[test]
fn a_tag_puts_the_git_lines_back_at_that_tag() {
    let workspace = Workspace::empty();
    checkout(&workspace, "0.5.0");
    project(&workspace);
    use_in(
        &mut Runner::execute(),
        &workspace.path("app"),
        &workspace.root,
        "nsh",
        &mut Vec::new(),
    )
    .expect("use the checkout");
    let mut out = Vec::new();
    use_in(
        &mut locking_at("nshx-0.5.0-8", workspace.path("app/Cargo.lock")),
        &workspace.path("app"),
        &workspace.root,
        "nshx-0.5.0-8",
        &mut out,
    )
    .expect("use the tag");
    assert_eq!(
        workspace.read("app/Cargo.toml"),
        MANIFEST.replace("nshx-0.5.0-7", "nshx-0.5.0-8")
    );
    let said = String::from_utf8(out).expect("UTF-8");
    assert!(
        said.contains("builds NSH from nshx-0.5.0-8 (c9bf1fe9a2b1)"),
        "{said}"
    );
}

#[test]
fn rayx_s_default_pin_is_read_from_its_pins_source() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    assert_eq!(
        rayx_default(&root).expect("readable"),
        Some(PinText {
            tag: DEFAULTS.nsh.tag.into(),
            commit: DEFAULTS.nsh.commit.into(),
            version: DEFAULTS.nsh.version.into(),
        })
    );
    assert_eq!(
        rayx_default(Path::new("/no/such/project")).expect("none"),
        None
    );
}

#[test]
fn a_new_tag_moves_the_tag_and_every_form_of_the_commit_standing_alone() {
    let old = PinText {
        tag: "nshx-0.5.0-7".into(),
        commit: OLD_COMMIT.into(),
        version: "0.5.0".into(),
    };
    let new = PinText {
        tag: "nshx-0.5.0-8".into(),
        commit: NEW_COMMIT.into(),
        version: "0.5.0".into(),
    };
    let text = format!(
        "`nshx-0.5.0-7` at `d47d7fd`, id `d47d7fd6dfdc`, {OLD_COMMIT}, d47d7fd6d.\n\
         keep: nshx-0.5.0-70, d47d7fd6dfdcb1, ad47d7fd\n"
    );
    assert_eq!(
        moved_text(&text, &old, &new),
        format!(
            "`nshx-0.5.0-8` at `c9bf1fe`, id `c9bf1fe9a2b1`, {NEW_COMMIT}, c9bf1fe9a.\n\
             keep: nshx-0.5.0-70, d47d7fd6dfdcb1, ad47d7fd\n"
        )
    );
}

#[test]
fn rayx_s_repository_moves_every_tracked_file_but_its_history() {
    let workspace = Workspace::empty();
    let pins = format!(
        "pub const DEFAULTS: Defaults = Defaults {{\n    nsh: DefaultNsh {{\n        \
         tag: \"nshx-0.5.0-7\",\n        commit: \"{OLD_COMMIT}\",\n        version: \"0.5.0\",\n    }},\n}};\n"
    );
    workspace.write("crates/rayx_cli/src/project/pins.rs", &pins);
    workspace.write("README.md", "The default: `nshx-0.5.0-7` at `d47d7fd`.\n");
    workspace.write("CHANGELOG.md", "- Pin NSH to nshx-0.5.0-7\n");
    workspace.write(".agentx/memory/state.md", "nshx-0.5.0-7\n");
    workspace.write("untracked.md", "nshx-0.5.0-7\n");
    let git = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(&workspace.root)
            .status()
            .expect("git");
        assert!(status.success(), "git {args:?}");
    };
    git(&["init", "-q"]);
    git(&[
        "add",
        "crates/rayx_cli/src/project/pins.rs",
        "README.md",
        "CHANGELOG.md",
        ".agentx/memory/state.md",
    ]);
    let old = rayx_default(&workspace.root)
        .expect("readable")
        .expect("RayX's pins");
    let new = PinText {
        tag: "nshx-0.6.0-1".into(),
        commit: NEW_COMMIT.into(),
        version: "0.6.0".into(),
    };
    let mut moved = move_default(
        &mut locking_at("nshx-0.6.0-1", workspace.path("Cargo.lock")),
        &workspace.root,
        &old,
        &new,
    )
    .expect("moved");
    moved.sort();
    assert_eq!(moved, ["README.md", "crates/rayx_cli/src/project/pins.rs"]);
    assert_eq!(
        rayx_default(&workspace.root).expect("readable"),
        Some(new),
        "pins.rs names the new tag, commit and version"
    );
    assert_eq!(
        workspace.read("README.md"),
        "The default: `nshx-0.6.0-1` at `c9bf1fe`.\n"
    );
    for kept in ["CHANGELOG.md", ".agentx/memory/state.md", "untracked.md"] {
        assert!(
            workspace.read(kept).contains("nshx-0.5.0-7"),
            "{kept} keeps its text"
        );
    }
}
