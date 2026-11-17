//! The dev tools' agent answers the hub's tree, node, find and pick requests on the engine's
//! thread from NSH's hooks, on a hot TSX app: each node line carries its id, kind, role, text,
//! test id, source stamp and component chain; component instances stand in the tree as
//! pseudo-nodes; the answers are bounded (depth, root, interactive only, characters with
//! `truncated`); the layer's subtree is in none of them; a node that is gone is an error naming
//! a subtree to look at again. Pay on connect: with no hub watching, a commit costs the agent no
//! hook call.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use std::time::Duration;

use rayx_devtools_protocol::{NodeLine, TreeResult, codes};
use serde_json::{Value, json};
use support::app::{serial, start};
use support::hub::Hub;

fn tree(hub: &mut Hub, params: Value) -> TreeResult {
    serde_json::from_value(hub.ask("tree", params).unwrap()).unwrap()
}

fn by_test_id<'a>(lines: &'a [NodeLine], id: &str) -> &'a NodeLine {
    lines
        .iter()
        .find(|line| line.test_id.as_deref() == Some(id))
        .unwrap_or_else(|| panic!("no `{id}` in {lines:#?}"))
}

#[test]
fn the_agent_answers_tree_node_find_and_pick() {
    let _serial = serial();
    let mut hub = Hub::listen();
    let (engine, dir) = start(&hub);
    hub.accept(Duration::from_secs(20));

    // The whole tree, as authored.
    let whole = tree(&mut hub, json!({}));
    assert!(whole.revision >= 1);
    let card = by_test_id(&whole.nodes, "card");
    let source = card
        .source
        .as_ref()
        .expect("a hot bundle stamps its elements");
    assert_eq!(
        (source.module.as_str(), source.anchor.as_str()),
        ("App.tsx", "Card")
    );
    assert_eq!(source.line, 5);
    let names: Vec<&str> = card.components.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["App.tsx:Card", "App.tsx:App"], "innermost first");
    // Card's instance stands between the root view and the card.
    let instance = whole
        .nodes
        .iter()
        .find(|line| line.kind == "instance" && line.text.as_deref() == Some("App.tsx:Card"))
        .expect("Card's instance pseudo-node");
    assert_eq!(card.parent, Some(instance.id));
    assert!(
        instance.id > u64::from(u32::MAX),
        "a pseudo-node never names a node"
    );
    assert_eq!(card.depth, instance.depth + 1);
    // The layer is in no answer.
    assert!(
        !whole.nodes.iter().any(
            |line| line.test_id.as_deref() == Some("rayx-devtools-layer")
                || line.text.as_deref() == Some("the layer")
        ),
        "{:#?}",
        whole.nodes
    );

    // Bounds: depth, root, interactive only, characters.
    let shallow = tree(&mut hub, json!({ "depth": 1 }));
    assert!(shallow.nodes.iter().all(|line| line.depth <= 1));
    assert!(shallow.nodes.len() < whole.nodes.len());
    let root = by_test_id(&whole.nodes, "save").id;
    let save = tree(&mut hub, json!({ "root": root }));
    assert_eq!(save.root, root);
    assert!(
        save.nodes
            .iter()
            .any(|line| line.text.as_deref() == Some("Save"))
    );
    assert!(
        !save
            .nodes
            .iter()
            .any(|line| line.test_id.as_deref() == Some("card"))
    );
    let interactive = tree(&mut hub, json!({ "interactiveOnly": true }));
    assert!(
        interactive
            .nodes
            .iter()
            .any(|line| line.test_id.as_deref() == Some("save"))
    );
    assert!(
        !interactive
            .nodes
            .iter()
            .any(|line| line.test_id.as_deref() == Some("card"))
    );
    let cut = tree(&mut hub, json!({ "maxChars": 300 }));
    assert!(cut.truncated && cut.nodes.len() < whole.nodes.len());
    assert!(serde_json::to_string(&cut.nodes).unwrap().len() <= 300 + 2 * cut.nodes.len());

    // A node in full.
    let node = hub.ask("node", json!({ "id": root })).unwrap();
    assert_eq!(node["node"]["testId"], "save");
    assert!(
        node["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e == "press"),
        "{node}"
    );

    // Find: by test id, and the innermost node reading a text.
    let found = hub.ask("find", json!({ "testId": "save" })).unwrap();
    assert_eq!(found["nodes"].as_array().unwrap().len(), 1);
    let found = hub.ask("find", json!({ "text": "Hello" })).unwrap();
    let found = found["nodes"].as_array().unwrap();
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0]["text"], "Hello");

    // Pick: the nodes under the save button's middle, innermost first.
    let frame = by_test_id(&whole.nodes, "save").frame;
    let picked = hub
        .ask(
            "pick",
            json!({ "x": frame.x + frame.width / 2.0, "y": frame.y + frame.height / 2.0 }),
        )
        .unwrap();
    let picked: Vec<u64> = serde_json::from_value(picked["nodes"].clone()).unwrap();
    assert!(picked.contains(&root), "{picked:?} under {frame:?}");

    // A node that is gone: an error that says to look at a subtree again.
    let gone = hub.ask("node", json!({ "id": 999_999 })).unwrap_err();
    assert_eq!(gone.code, codes::STALE_NODE);
    assert!(
        gone.message.contains("take a new snapshot"),
        "{}",
        gone.message
    );

    drop(engine);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn with_no_hub_watching_a_commit_costs_no_hook_call() {
    let _serial = serial();
    let mut hub = Hub::listen();
    let (engine, dir) = start(&hub);
    hub.accept(Duration::from_secs(20));
    let first = tree(&mut hub, json!({})).revision;

    // The app commits every 30 ms; nothing watches: no hook call.
    let before = rayx_app_nsh::devtools_hook_calls();
    hub.listen_for(Duration::from_millis(600));
    assert_eq!(
        rayx_app_nsh::devtools_hook_calls(),
        before,
        "work per commit without a watcher"
    );

    // Watched: the agent looks again and notifies each change with a newer revision.
    hub.ask("tree.watch", json!({ "on": true })).unwrap();
    hub.listen_for(Duration::from_millis(800));
    assert!(rayx_app_nsh::devtools_hook_calls() > before);
    let changes: Vec<u64> = hub
        .notifications
        .iter()
        .filter(|(method, _)| method == "tree.changed")
        .map(|(_, params)| params["revision"].as_u64().unwrap())
        .collect();
    assert!(changes.len() >= 2, "{:?}", hub.notifications);
    assert!(changes.windows(2).all(|pair| pair[0] < pair[1]) && changes[0] > first);

    // Off and on again quickly, three times: one loop still looks, not four.
    let calls_in = |hub: &mut Hub, span: Duration| {
        let start = rayx_app_nsh::devtools_hook_calls();
        hub.listen_for(span);
        rayx_app_nsh::devtools_hook_calls() - start
    };
    let one_loop = calls_in(&mut hub, Duration::from_millis(1000));
    for _ in 0..3 {
        hub.ask("tree.watch", json!({ "on": false })).unwrap();
        hub.ask("tree.watch", json!({ "on": true })).unwrap();
    }
    hub.listen_for(Duration::from_millis(200));
    let after_toggles = calls_in(&mut hub, Duration::from_millis(1000));
    assert!(
        after_toggles * 2 < one_loop * 3,
        "{after_toggles} hook calls a second after quick toggles, {one_loop} with one loop"
    );

    // Unwatched again: no more calls once the last look is done.
    hub.ask("tree.watch", json!({ "on": false })).unwrap();
    hub.listen_for(Duration::from_millis(300));
    let after = rayx_app_nsh::devtools_hook_calls();
    hub.listen_for(Duration::from_millis(600));
    assert_eq!(rayx_app_nsh::devtools_hook_calls(), after);

    drop(engine);
    let _ = std::fs::remove_dir_all(dir);
}
