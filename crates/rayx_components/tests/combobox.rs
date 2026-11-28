//! RayX's `Combobox`: Rust headless rendering, single/multi selection, disabled state,
//! TypeScript declarations, and TSX mounting & interaction.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use nsh_core::element::{Root, declarations};
use nsh_core::prelude::*;
use nsh_engine::App;
use rayx_components::combobox::{ComboboxItem, combobox};
use rayx_components::theme::ComponentSize;
use support::{Running, Tsx};

#[component]
fn ComboboxHarness() -> impl IntoElement {
    let selected = signal(vec![SharedString::new_static("128")]);
    let changes = signal(0_u32);

    view()
        .items_start()
        .gap_2()
        .p_4()
        .child(
            combobox()
                .test_id("size-combo")
                .input_test_id("size-combo-input")
                .accessibility_label(Some(SharedString::new_static("Thumbnail size")))
                .size(ComponentSize::Small)
                .items(vec![
                    ComboboxItem::new("128", "128 px"),
                    ComboboxItem::new("192", "192 px"),
                    ComboboxItem::new("256", "256 px"),
                    ComboboxItem::new("512", "512 px").disabled(true),
                ])
                .value(move || Some(selected.get()))
                .on_change(move |next: &Vec<SharedString>| {
                    selected.set(next.clone());
                    changes.update(|n| *n += 1);
                })
                .w(px(140.0)),
        )
        .child(text(move || {
            let joined = selected.get().join(",");
            format!("selected={joined} changes={}", changes.get())
        }))
}

#[test]
fn combobox_opens_and_selects_option_in_rust() {
    let mut app = Running::start(App::Rust(Root::of::<ComboboxHarness>()));
    assert!(
        app.wait(|s| s.text_node("selected=128 changes=0").is_some()),
        "{:?}",
        app.screen.shown()
    );

    // Pressing the trigger button opens the popup list.
    app.press("▾");
    assert!(
        app.wait(|s| s.text_node("256 px").is_some()),
        "{:?}",
        app.screen.shown()
    );

    // Choosing an option updates the value and emits onChange.
    app.press("256 px");
    assert!(
        app.wait(|s| s.text_node("selected=256 changes=1").is_some()),
        "{:?}",
        app.screen.shown()
    );
}

#[test]
fn the_library_set_declares_combobox() {
    let set = rayx_components::set();
    assert_eq!(set.undocumented(), Vec::<String>::new());
    let ts = declarations(&set, &[]);
    for line in [
        "export interface ComboboxItem {",
        "export interface ComboboxProps extends ComponentHostProps {",
        "  onChange?: (event: { value: (string)[] }) => void;",
        "export declare const Combobox: ((props: ComboboxProps) => JSX.Element);",
    ] {
        assert!(ts.contains(line), "no `{line}` in:\n{ts}");
    }
}

#[test]
fn combobox_mounts_and_changes_selection_from_tsx() {
    let mut app = Tsx::start(
        r#"import { mount, signal, Text, View } from "nsh";
import { Combobox, type ComboboxItem } from "rayx/components";

const value = signal<string[]>(["128"]);
const items: ComboboxItem[] = [
  { value: "128", label: "128 px" },
  { value: "256", label: "256 px" },
  { value: "512", label: "512 px" },
];

mount(() => (
  <View class="flex-1 p-4 gap-2">
    <Combobox
      testID="thumb-size"
      inputTestID="thumb-size-input"
      size="small"
      items={items}
      value={value()}
      onChange={({ value: next }) => value.set(next)}
      class="w-[120px]"
    />
    <Text>{`size:${value()[0] ?? "none"}`}</Text>
  </View>
));
"#,
    );

    assert!(
        app.wait(|s| s.text_node("size:128").is_some() && s.pressable("▾").is_some()),
        "{:?}",
        app.screen.shown()
    );

    let trigger = app.screen.pressable("▾").unwrap();
    app.press_node(trigger);
    assert!(
        app.wait(|s| s.pressable("512 px").is_some()),
        "{:?}",
        app.screen.shown()
    );
    let option = app.screen.pressable("512 px").unwrap();
    app.press_node(option);
    assert!(
        app.wait(|s| s.text_node("size:512").is_some()),
        "{:?}",
        app.screen.shown()
    );
}
