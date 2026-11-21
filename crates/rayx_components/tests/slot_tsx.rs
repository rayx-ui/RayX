//! RayX's `Slot` from TSX, headless, outside design mode: the library's set declares it with its
//! name and one JSX child; a component marks its slot with it around the prop that fills it
//! (`<Slot name="footer">{props.footer ?? <Default />}</Slot>`); empty it takes no gap, holding
//! one element it lays it out as it is, the component's default content shows until its user
//! passes content, and two JSX children make a dev build warn once, naming the slot. A slot's
//! press handlers reaching the element a `Slot` holds is checked by an ignored test (see it).

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use nsh_core::element::{Registry, declarations};
use nsh_core::prelude::*;
use rayx_components::slot::warned;
use support::{Screen, Tsx};

/// A trigger the app passes, which the view makes a disclosure: presses on it flip `open`.
#[derive(Debug, Clone, PartialEq, Props)]
struct DisclosureProps {
    /// What the user presses.
    trigger: Prop<Option<Slot>>,
}

/// The trigger, given presses that flip whether it is open, and a text saying so.
#[component]
fn Disclosure(props: DisclosureProps) -> impl IntoElement {
    let open = signal(false);
    view()
        .child(
            props
                .trigger
                .place(move |trigger| trigger.on_press(move |_: &()| open.update(|o| *o = !*o))),
        )
        .child(text(move || format!("open {}", open.get())))
}

/// The app's UI sets, with a set of its own holding `Disclosure`.
fn sets() -> Vec<Registry> {
    let mut sets = support::ui_sets();
    let mut own = Registry::ui_set("test/slots");
    own.register::<Disclosure>();
    sets.push(own);
    sets
}

const TSX: &str = r#"import { mount, signal, Text, View } from "nsh";
import { Slot } from "rayx/components";
import { Disclosure } from "test/slots";

function Card(props: { footer?: JSX.Element }) {
  return (
    <View>
      <Text>card</Text>
      <Slot name="footer">{props.footer ?? <Text>Default footer</Text>}</Slot>
    </View>
  );
}

const presses = signal(0);
mount(() => (
  <View class="flex-row items-start gap-[8px]">
    <View class="gap-[10px]"><Text>A1</Text><Slot name="empty" /><Text>B1</Text></View>
    <View class="gap-[10px]"><Text>A2</Text><Text>B2</Text></View>
    <View class="gap-[10px]"><Text>A3</Text><Slot name="one"><Text>C3</Text></Slot><Text>B3</Text></View>
    <View class="gap-[10px]"><Text>A4</Text><Text>C4</Text><Text>B4</Text></View>
    <Card />
    <Card footer={<Text>Custom footer</Text>} />
    <Slot name="pair-tsx"><Text>x1</Text><Text>x2</Text></Slot>
    <Disclosure
      trigger={
        <Slot name="trigger">
          <View onPress={() => presses.set(presses() + 1)}>
            <Text>Open</Text>
          </View>
        </Slot>
      }
    />
    <Text>{`presses ${presses()}`}</Text>
  </View>
));
"#;

fn start() -> Tsx {
    let mut app = Tsx::start_with(TSX, sets());
    assert!(
        app.wait(|s| {
            [
                "B1",
                "B2",
                "C3",
                "C4",
                "x1",
                "x2",
                "Custom footer",
                "presses 0",
                "open false",
            ]
            .iter()
            .all(|label| s.text_frame(label).is_some())
                && s.pressable("Open").is_some()
        }),
        "{:?}",
        app.screen.shown()
    );
    app
}

/// Where `label` is in its column (the view around `A<n>`): x, y, width, height.
fn in_column(screen: &Screen, column: &str, label: &str) -> (f32, f32, f32, f32) {
    let around = screen.around(column).unwrap();
    let (cx, cy) = screen.absolute(around);
    let (x, y, w, h) = screen.text_frame(label).unwrap();
    (x - cx, y - cy, w, h)
}

#[test]
fn the_library_set_declares_slot_with_a_name_and_one_child() {
    let set = rayx_components::set();
    assert_eq!(set.undocumented(), Vec::<String>::new());
    let ts = declarations(&set, &[]);
    for line in [
        "export interface SlotProps",
        "  name: string;",
        "  children?: JSX.Element;",
        "export declare const Slot: ((props: SlotProps) => JSX.Element);",
    ] {
        assert!(ts.contains(line), "no `{line}` in:\n{ts}");
    }
}

#[test]
fn an_empty_slot_takes_no_gap_and_one_element_is_laid_out_as_without_it() {
    let app = start();
    let s = &app.screen;
    assert_eq!(in_column(s, "A1", "B1"), in_column(s, "A2", "B2"));
    assert!(s.text_node("empty").is_none(), "{:?}", s.shown());
    assert_eq!(in_column(s, "A3", "C3"), in_column(s, "A4", "C4"));
    assert_eq!(in_column(s, "A3", "B3"), in_column(s, "A4", "B4"));
}

#[test]
fn the_components_default_content_shows_until_its_user_passes_content() {
    let app = start();
    let shown = app.screen.shown();
    let count = |label: &str| shown.iter().filter(|t| t.as_str() == label).count();
    assert_eq!(count("card"), 2, "{shown:?}");
    assert_eq!(count("Default footer"), 1, "{shown:?}");
    assert_eq!(count("Custom footer"), 1, "{shown:?}");
}

#[test]
fn two_jsx_children_in_one_slot_warn_once_naming_it_and_both_show() {
    let app = start();
    assert!(app.screen.text_node("x1").is_some() && app.screen.text_node("x2").is_some());
    let warned = warned();
    assert_eq!(
        warned.iter().filter(|n| n.as_str() == "pair-tsx").count(),
        1,
        "{warned:?}"
    );
    for one in ["empty", "one", "footer", "trigger"] {
        assert!(!warned.iter().any(|n| n == one), "{one}: {warned:?}");
    }
}

/// A component that marks a slot with `Slot` inside another component's slot (a trigger): the
/// presses that component gives its slot reach the element the `Slot` holds, so pressing the
/// view in it, which hears presses itself, is the disclosure's press and the view's own.
///
/// NSH looks through a component whose view's root is a plain view holding only the place of its
/// children (`Core::places_children_only`): nothing else on or in the root, no style and no
/// binding. `Slot`'s root has a bound style (hidden while empty; the empty box in design mode),
/// places its children one by one (to count them) and holds its name's text in design mode, so
/// the slot's target is its host and the view alone hears the press. (With a root of
/// `view().child(props.children)` alone this test passes, and the empty slot's gap and the
/// warning fail.) It passes once NSH looks through a root whose own parts are styles only and
/// whose children are placed by `children_each`, or once `Slot` can keep a plain root because
/// NSH takes an empty such host out of the layout and `Children` gives their number.
#[test]
#[ignore = "needs NSH to look through Slot's root, which has a bound style (see the doc comment)"]
fn a_slots_press_handlers_reach_the_element_a_slot_holds() {
    let mut app = start();
    let open = app.screen.pressable("Open").unwrap();
    app.press_node(open);
    assert!(
        app.wait(|s| s.text_node("presses 1").is_some()),
        "{:?}",
        app.screen.shown()
    );
    assert!(
        app.wait(|s| s.text_node("open true").is_some()),
        "the disclosure heard no press: {:?}",
        app.screen.shown()
    );
}
