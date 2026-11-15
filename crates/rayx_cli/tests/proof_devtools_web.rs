//! A web dev build of a RayX app says `hello` to a test hub from a headless browser (AGENT-1),
//! dialling again until the hub, which comes up after the page, answers; then it answers tree,
//! find, node and pick, and refuses act, input and screenshot as unsupported on the web, on one
//! connection (AGENT-3); ignored by default: it installs rayx's default nsh into a temporary `RAYX_HOME`, builds the
//! app made by `rayx new` against this checkout (a test-only path override written into the
//! generated app), serves it with `rayx app run --target web` and opens it in headless Chrome
//! (`CHROME`, else Chrome's macOS path), which needs the network.
//!
//! ```text
//! cargo test -p rayx-cli --test proof_devtools_web -- --ignored --test-threads=1
//! ```

mod common;

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use serde_json::{Value, json};

use common::Workspace;
use common::hub::{DEFAULT_PORT, Hub};
use common::proof::{Running, free_port, override_rayx, rayx, rayx_ok, wait_for_ok};

fn chrome() -> Option<PathBuf> {
    std::env::var_os("CHROME")
        .map(PathBuf::from)
        .or_else(|| {
            Some(PathBuf::from(
                "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            ))
        })
        .filter(|path| path.is_file())
}

#[test]
#[ignore = "installs nsh, builds a RayX app for the web and opens it in Chrome (network, many minutes)"]
fn a_web_dev_build_says_hello_from_a_headless_browser() {
    let Some(chrome) = chrome() else {
        eprintln!("skipped: no Chrome (set CHROME)");
        return;
    };
    let home = Workspace::empty();
    let work = Workspace::empty();
    rayx_ok(&home.root, &work.root, &["new", "hello"]);
    let app = work.path("hello");
    override_rayx(&app);

    let port = free_port();
    let port_text = port.to_string();
    let mut server = Running::start(
        rayx(
            &home.root,
            &app,
            &["app", "run", "--target", "web", "--port", &port_text],
        ),
        "rayx app run --target web",
    );
    wait_for_ok(&mut server, port, "/nsh.version", Duration::from_secs(1800));

    let profile = Workspace::empty();
    let mut browser = Running::start(
        {
            let mut command = Command::new(chrome);
            command.args([
                "--headless=new",
                "--disable-gpu",
                "--no-first-run",
                &format!("--user-data-dir={}", profile.root.display()),
                &format!("http://127.0.0.1:{port}/"),
            ]);
            command
        },
        "headless Chrome",
    );
    // The hub comes up once the page has loaded and its first dials have failed: the page's
    // agent dials again with its backoff.
    std::thread::sleep(Duration::from_secs(10));
    browser.assert_running();
    let mut hub = Hub::listen(DEFAULT_PORT);
    hub.accept(Duration::from_secs(120), || {
        browser.assert_running();
        true
    });
    let hello = hub.hello.clone().expect("hello");
    assert_eq!(hello["platform"], "web", "{hello}");
    assert_eq!(hello["protocolVersion"], "1.0");
    assert!(
        !hello["buildId"].as_str().unwrap_or("").is_empty(),
        "{hello}"
    );

    // Reading works on the web: the tree, the template's text, the node in full, a pick.
    let deadline = std::time::Instant::now() + Duration::from_secs(120);
    let ready = loop {
        let found = hub.ask("find", json!({ "text": "Ready" })).expect("find");
        if let Some(node) = found["nodes"]
            .as_array()
            .and_then(|nodes| nodes.first())
            .cloned()
        {
            break node;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the page never showed Ready"
        );
        std::thread::sleep(Duration::from_millis(500));
    };
    let tree = hub.ask("tree", json!({})).expect("tree");
    assert!(
        tree["revision"].as_u64().is_some_and(|r| r >= 1)
            && tree["nodes"].as_array().is_some_and(|n| !n.is_empty()),
        "{tree}"
    );
    let node = hub.ask("node", json!({ "id": ready["id"] })).expect("node");
    assert_eq!(node["node"]["id"], ready["id"], "{node}");
    // A text run has no frame of its own: pick at the middle of the nearest laid-out node holding
    // it.
    let lines = tree["nodes"].as_array().expect("lines");
    let line = |id: &Value| lines.iter().find(|line| line["id"] == *id).cloned();
    let mut shown = line(&ready["id"]).expect("Ready is in the tree");
    while shown["frame"]["width"].as_f64().unwrap_or(0.0) <= 0.0 {
        shown = line(&shown["parent"]).expect("a laid-out node holds Ready");
    }
    let frame = &shown["frame"];
    let middle = |at: &str, size: &str| {
        frame[at].as_f64().unwrap_or(0.0) + frame[size].as_f64().unwrap_or(0.0) / 2.0
    };
    let picked = hub
        .ask(
            "pick",
            json!({ "x": middle("x", "width"), "y": middle("y", "height") }),
        )
        .expect("pick");
    assert!(
        picked["nodes"]
            .as_array()
            .is_some_and(|nodes| nodes.contains(&shown["id"])),
        "{picked} at {frame}"
    );

    // Driving is not on the web: each is refused as unsupported, and the connection stays.
    for (method, params) in [
        ("act", json!({ "action": "press", "node": ready["id"] })),
        ("input", json!({ "input": "key", "keys": "mod+s" })),
        ("screenshot", json!({})),
    ] {
        let refused = hub.ask(method, params).expect_err(method);
        assert_eq!(refused["code"], -32004, "{method}: {refused}");
        assert_eq!(
            refused["data"]["kind"], "unsupported_on_target",
            "{method}: {refused}"
        );
    }
    hub.ask("tree", json!({ "depth": 1 }))
        .expect("the connection stays");
    drop(browser);
    drop(server);
}
