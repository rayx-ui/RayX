//! The dev tools are in a dev build and nowhere else (AGENT-5), ignored by default: each test
//! installs rayx's default nsh into a temporary `RAYX_HOME` and builds a RayX app made by
//! `rayx new` against this checkout (a test-only path override written into the generated app),
//! which needs the network.
//!
//! - A release package (the web's, and the macOS app's on a Mac) holds no agent, no layer and no
//!   devtools marker string, in its binary or its bundle.
//! - A dev build on macOS dials a test hub on the agent's default port, says `hello`, answers the
//!   tree and takes a press through the dev session's driver.
//!
//! ```text
//! cargo test -p rayx-cli --test proof_devtools_release -- --ignored --test-threads=1
//! ```

mod common;

use std::path::Path;
use std::time::Duration;

use common::Workspace;
use common::hub::{DEFAULT_PORT, Hub};
use common::proof::{Running, override_rayx, rayx, rayx_ok};
use serde_json::{Value, json};

/// Strings only the dev tools put into a build: the layer's test ids and toolbar, the agent's
/// thread names and the protocol's method names.
const MARKERS: &[&str] = &[
    "rayx-devtools",
    "layer.pickModeRequested",
    "tree.changed",
    "rayx_devtools",
    "DEP_NSH_CORE_INSPECT",
];

/// The markers found in the files under `dir`.
fn markers_under(dir: &Path, found: &mut Vec<(String, &'static str)>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            markers_under(&path, found);
        } else if let Ok(bytes) = std::fs::read(&path) {
            for marker in MARKERS {
                if bytes
                    .windows(marker.len())
                    .any(|window| window == marker.as_bytes())
                {
                    found.push((path.display().to_string(), marker));
                }
            }
        }
    }
}

fn new_app(home: &Workspace, work: &Workspace) -> std::path::PathBuf {
    rayx_ok(&home.root, &work.root, &["new", "hello"]);
    let app = work.path("hello");
    override_rayx(&app);
    app
}

#[test]
#[ignore = "installs nsh and packages a RayX app (network, many minutes)"]
fn a_release_package_carries_no_dev_tools() {
    let home = Workspace::empty();
    let work = Workspace::empty();
    let app = new_app(&home, &work);
    let mut targets = vec!["web"];
    if cfg!(target_os = "macos") {
        targets.push("macos");
    }
    for target in targets {
        let out = Workspace::empty();
        let out_text = out.root.display().to_string();
        rayx_ok(
            &home.root,
            &app,
            &["app", "package", "--target", target, "--out", &out_text],
        );
        let mut found = Vec::new();
        markers_under(&out.root, &mut found);
        assert!(
            found.is_empty(),
            "the {target} package carries dev tools: {found:?}"
        );
    }
}

#[test]
#[ignore = "installs nsh and runs a RayX app on macOS (network, many minutes)"]
fn a_dev_build_answers_a_hub_and_takes_a_press() {
    if !cfg!(target_os = "macos") {
        eprintln!("skipped: the dev build runs on macOS");
        return;
    }
    let home = Workspace::empty();
    let work = Workspace::empty();
    let app = new_app(&home, &work);
    // Something to press: the template's status, in a view that says it was pressed.
    let entry = app.join("App.tsx");
    let tsx = std::fs::read_to_string(&entry).expect("App.tsx");
    let status = r#"<Text style={{ fontSize: 22 }}>{status()}</Text>"#;
    assert!(tsx.contains(status), "{tsx}");
    let pressable = format!(
        r#"<View testID="go" onPress={{() => status.set("Pressed")}} style={{{{ padding: 12 }}}}>{status}</View>"#
    );
    std::fs::write(&entry, tsx.replace(status, &pressable)).expect("App.tsx");
    let mut hub = Hub::listen(DEFAULT_PORT);
    let mut running = Running::start(rayx(&home.root, &app, &["app", "run"]), "rayx app run");
    hub.accept(Duration::from_secs(1800), || {
        running.assert_running();
        true
    });
    let hello = hub.hello.clone().expect("hello");
    assert_eq!(hello["platform"], "macos", "{hello}");
    assert_eq!(hello["protocolVersion"], "1.0");
    assert!(
        hello["root"]
            .as_str()
            .is_some_and(|root| Path::new(root).join("nsh.json").is_file()),
        "{hello}"
    );

    // The tree, once the app shows its state; then a press on its button, through the driver.
    let find = |hub: &mut Hub, query: Value| {
        let deadline = std::time::Instant::now() + Duration::from_secs(120);
        loop {
            let found = hub.ask("find", query.clone()).expect("find");
            if let Some(node) = found["nodes"]
                .as_array()
                .and_then(|nodes| nodes.first())
                .cloned()
            {
                return node;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the app never showed {query}"
            );
            std::thread::sleep(Duration::from_millis(500));
        }
    };
    find(&mut hub, json!({ "text": "Ready" }));
    let node = find(&mut hub, json!({ "testId": "go" }));
    let tree = hub.ask("tree", json!({ "depth": 3 })).expect("tree");
    assert!(
        tree["revision"]
            .as_u64()
            .is_some_and(|revision| revision >= 1),
        "{tree}"
    );
    let done = hub
        .ask("act", json!({ "action": "press", "node": node["id"] }))
        .expect("the press");
    assert!(done["revision"].as_u64().is_some(), "{done}");
    find(&mut hub, json!({ "text": "Pressed" }));
    drop(running);
}
