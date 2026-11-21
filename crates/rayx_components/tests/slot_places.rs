//! RayX's `Slot` in a design tool's layers (NSH's `design.layers`), headless, from TSX: a `Slot`
//! the app's code writes and one `Button`'s Rust view draws both report the slot they are the
//! place of (`place`, the JS name of the prop that fills it), so a design tool finds a slot the
//! same way in a TSX component and in a Rust one; the Rust one is listed under `Button`, with the
//! slot's holder and the app's content under it.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use serde_json::Value;
use support::Tsx;

const TSX: &str = r#"import { mount, signal, Text, View } from "nsh";
import { design } from "nsh/design";
import { Button, Slot } from "rayx/components";

const shown = signal("waiting");

mount(() => (
  <View>
    <View testID="board:written">
      <Slot name="footer"><Text>F</Text></Slot>
    </View>
    <View testID="board:button">
      <Button text="Inbox" showSlotLeft slotLeft={<Text>3</Text>} />
    </View>
    <Text>{shown()}</Text>
  </View>
));

setTimeout(async () => {
  const layers = await design.layers("board:");
  shown.set("read " + JSON.stringify(layers));
}, 0);
"#;

/// The layers the app read, by board.
fn read() -> Value {
    let mut app = Tsx::start(TSX);
    assert!(
        app.wait(|s| s.shown().iter().any(|t| t.starts_with("read "))),
        "{:?}",
        app.screen.shown()
    );
    let text = app
        .screen
        .shown()
        .into_iter()
        .find(|t| t.starts_with("read "))
        .unwrap();
    serde_json::from_str(text.strip_prefix("read ").unwrap()).unwrap()
}

/// Board `root`'s layers as `kind name [slot] {place}` lines, indented under their parents
/// (component instances left out).
fn outline(layers: &Value, root: &str) -> Vec<String> {
    let nodes = layers["roots"][root]["nodes"].as_array().unwrap();
    let layer = |id: &Value| nodes.iter().find(|n| &n["id"] == id);
    let above = |node: &Value| {
        let mut parent = layer(&node["parent"]);
        while let Some(p) = parent.filter(|p| p["kind"] == "component") {
            parent = layer(&p["parent"]);
        }
        parent
    };
    nodes
        .iter()
        .filter(|n| n["kind"] != "component")
        .map(|node| {
            let mut depth = 0;
            let mut at = above(node);
            while let Some(p) = at {
                depth += 1;
                at = above(p);
            }
            let mut line = format!(
                "{}{} {}",
                "  ".repeat(depth),
                node["kind"].as_str().unwrap(),
                node["name"].as_str().unwrap()
            );
            if let Some(slot) = node["slot"].as_str() {
                line.push_str(&format!(" [{slot}]"));
            }
            if let Some(place) = node["place"].as_str() {
                line.push_str(&format!(" {{{place}}}"));
            }
            line
        })
        .collect()
}

#[test]
fn a_slot_written_in_tsx_and_one_a_rust_view_draws_report_their_places() {
    let layers = read();
    assert_eq!(
        outline(&layers, "written"),
        ["host Slot {footer}", "  text Text"]
    );
    assert_eq!(
        outline(&layers, "button"),
        [
            "host Button",
            "  host Slot {slotLeft}",
            "    view View [slotLeft]",
            "      text Text",
        ]
    );
}
