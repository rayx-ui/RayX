//! An existing app through rayx, ignored by default. It reads the NSH app at `RAYX_PROOF_APP` (the
//! directory with its `nsh.json`, inside a git checkout) and changes none of its tracked files:
//! rayx shows the pin from the app's project, installs it into a temporary `RAYX_HOME` (a source
//! build of several minutes while the fork has no release), type checks the app, packages it for
//! the web and serves the package.
//!
//! Generated files the app needs to build (its configuration, for one) must exist before the
//! proof runs; it never writes them.
//!
//! ```text
//! RAYX_PROOF_APP=<app> cargo test -p rayx-cli --test proof_existing_app -- --ignored --test-threads=1
//! ```

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use common::Workspace;
use common::proof::{Running, free_port, get, rayx, rayx_ok, rayx_output, wait_for_ok};

fn app_dir() -> PathBuf {
    let dir = std::env::var_os("RAYX_PROOF_APP")
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set RAYX_PROOF_APP to the directory of an NSH app"));
    std::fs::canonicalize(&dir)
        .unwrap_or_else(|_| panic!("no app at {}: set RAYX_PROOF_APP", dir.display()))
}

/// The app's tracked files as git sees them: the status of tracked paths and the diff against
/// `HEAD`, so work in progress there is compared, not required to be committed.
fn tracked_state(dir: &Path) -> (String, Vec<u8>) {
    let git = |args: &[&str]| {
        // `--no-optional-locks`: a status refreshes the app's index otherwise, under the owner's
        // feet.
        let output = Command::new("git")
            .arg("--no-optional-locks")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .expect("git");
        assert!(output.status.success(), "git {args:?} in {}", dir.display());
        output.stdout
    };
    let status = String::from_utf8(git(&["status", "--porcelain=v1", "--untracked-files=no"]))
        .expect("UTF-8");
    (status, git(&["diff", "--binary", "HEAD"]))
}

/// The first file under `dir` whose name ends with `suffix`, relative to `dir`.
fn find(dir: &Path, suffix: &str) -> Option<PathBuf> {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        for entry in std::fs::read_dir(&next).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.to_string_lossy().ends_with(suffix) {
                return path.strip_prefix(dir).ok().map(Path::to_path_buf);
            }
        }
    }
    None
}

#[test]
#[ignore = "installs nsh and packages an existing app for the web (network, many minutes)"]
fn an_existing_app_shows_its_pin_checks_packages_and_serves_through_rayx_unchanged() {
    let app = app_dir();
    let before = tracked_state(&app);
    let home = Workspace::empty();

    let status = rayx_output(&home.root, &app, &["nsh", "status"]);
    let status_text = String::from_utf8_lossy(&status.stdout).into_owned();
    assert!(status.status.success(), "{status_text}");
    assert!(
        status_text.contains("project"),
        "the pin comes from the app's project: {status_text}"
    );

    rayx_ok(&home.root, &app, &["app", "check", "."]);

    let out = Workspace::empty();
    let out_text = out.root.display().to_string();
    rayx_ok(
        &home.root,
        &app,
        &["app", "package", ".", "--target", "web", "--out", &out_text],
    );
    let index = find(&out.root, "index.html").expect("the package has an index.html");
    let site = out.root.join(
        index
            .parent()
            .expect("index.html is in the package's directory"),
    );
    let wasm = find(&site, ".wasm").expect("the package has a .wasm");

    let port = free_port();
    let port_text = port.to_string();
    let site_text = site.display().to_string();
    let mut serve = Running::start(
        rayx(
            &home.root,
            &out.root,
            &["serve", "--dir", &site_text, "--port", &port_text],
        ),
        "rayx serve",
    );
    wait_for_ok(&mut serve, port, "/health", Duration::from_secs(30));
    let page = get(port, "/").expect("the page");
    assert_eq!(page.status, 200);
    assert_eq!(
        page.body,
        std::fs::read(site.join("index.html")).expect("index.html")
    );
    let url = format!(
        "/{}",
        wasm.components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/")
    );
    let module = get(port, &url).expect("the module");
    assert_eq!(module.status, 200, "{url}");
    assert_eq!(
        module.header("content-type"),
        Some("application/wasm"),
        "{url}"
    );
    assert_eq!(
        module.body.len() as u64,
        std::fs::metadata(site.join(&wasm)).expect("wasm").len()
    );
    drop(serve);

    let after = tracked_state(&app);
    assert_eq!(before.0, after.0, "the status of the app's tracked files");
    assert!(before.1 == after.1, "the app's tracked files changed");
}
