//! RayX's `SegmentedControl`: Rust headless view rendering, selection, disabled items, sizes,
//! plus TypeScript declarations (including `IconSrc`, `IllustrationSrc`, `AssetRef`) and TSX
//! mounting & interaction.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use nsh_core::element::{Root, declarations};
use nsh_core::prelude::*;
use nsh_engine::App;
use rayx_components::segmented_control::{SegmentedItem, segmented_control};
use rayx_components::theme::ComponentSize;
use support::{Running, Tsx};

#[component]
fn SegmentedHarness() -> impl IntoElement {
    let selected = signal(SharedString::new_static("all"));
    let changes = signal(0_u32);

    view()
        .items_start()
        .gap_2()
        .child(
            segmented_control()
                .accessibility_label(Some(SharedString::new_static("Filter assets")))
                .items(vec![
                    SegmentedItem::new("all", "All").badge("12"),
                    SegmentedItem::new("icons", "Icons").badge("8"),
                    SegmentedItem::new("fonts", "Fonts"),
                    SegmentedItem::new("themes", "Themes").disabled(true),
                ])
                .value(selected)
                .on_change(move |next: &SharedString| {
                    selected.set(next.clone());
                    changes.update(|n| *n += 1);
                }),
        )
        .child(
            segmented_control()
                .size(ComponentSize::Small)
                .items(vec![
                    SegmentedItem::new("grid", "Grid"),
                    SegmentedItem::new("list", "List"),
                ])
                .value(SharedString::new_static("grid")),
        )
        .child(
            segmented_control()
                .disabled(true)
                .items(vec![
                    SegmentedItem::new("locked-a", "Locked A"),
                    SegmentedItem::new("locked-b", "Locked B"),
                ])
                .value(SharedString::new_static("locked-a"))
                .on_change(move |_: &SharedString| {
                    changes.update(|n| *n += 100);
                }),
        )
        .child(text(move || {
            format!("selected={} changes={}", selected.get(), changes.get())
        }))
}

#[test]
fn segmented_control_renders_segments_and_handles_selection_and_disabled() {
    let mut app = Running::start(App::Rust(Root::of::<SegmentedHarness>()));
    assert!(
        app.wait(|s| {
            s.text_node("All").is_some()
                && s.text_node("Icons").is_some()
                && s.text_node("12").is_some()
                && s.text_node("selected=all changes=0").is_some()
        }),
        "{:?}",
        app.screen.shown()
    );

    // The active tab draws its own opaque pill fill behind its label, while any non-tab
    // indicator sibling keeps a transparent fill so it can never paint over active text.
    let transparent = nsh_core::Color::from(nsh_core::color::Hsla::default());
    let all_tab = app.screen.pressable("All").unwrap();
    let icons_tab = app.screen.pressable("Icons").unwrap();
    let tabs_parent = *app.screen.parents.get(&all_tab).unwrap();
    let all_bg = app.screen.styles.get(&all_tab).unwrap().background_color;
    let icons_bg = app.screen.styles.get(&icons_tab).unwrap().background_color;
    assert_ne!(
        all_bg, transparent,
        "active segment draws its own pill fill: {all_bg:?}"
    );
    assert_eq!(
        icons_bg, transparent,
        "inactive segment has no pill fill: {icons_bg:?}"
    );
    for sibling in app.screen.children(tabs_parent) {
        if !app.screen.events.contains_key(&sibling)
            && let Some(style) = app.screen.styles.get(&sibling)
        {
            assert_eq!(
                style.background_color, transparent,
                "indicator sibling must stay transparent so it never occludes active tab text"
            );
        }
    }

    // Pressing an unselected segment updates value and emits onChange.
    app.press("Icons");
    assert!(
        app.wait(|s| {
            s.text_node("selected=icons changes=1").is_some()
                && s.styles
                    .get(&icons_tab)
                    .is_some_and(|st| st.background_color != transparent)
                && s.styles
                    .get(&all_tab)
                    .is_some_and(|st| st.background_color == transparent)
        }),
        "{:?}",
        app.screen.shown()
    );

    // Pressing the already-selected segment does not re-emit onChange.
    app.press("Icons");
    app.press("Fonts");
    assert!(
        app.wait(|s| s.text_node("selected=fonts changes=2").is_some()),
        "{:?}",
        app.screen.shown()
    );

    // Individually disabled segment is not pressable.
    assert!(app.screen.pressable("Themes").is_none());
    // Segments in a disabled control are not pressable.
    assert!(app.screen.pressable("Locked B").is_none());
}

#[test]
fn the_library_set_declares_segmented_control_and_asset_prop_aliases() {
    let set = rayx_components::set();
    assert_eq!(set.undocumented(), Vec::<String>::new());
    let ts = declarations(&set, &[]);
    for line in [
        "export interface SegmentedItem {",
        "export interface SegmentedControlProps extends ComponentHostProps {",
        "  onChange?: (event: { value: string }) => void;",
        "export declare const SegmentedControl: ((props: SegmentedControlProps) => JSX.Element);",
        "export type IconSrc = string;",
        "export type IllustrationSrc = string;",
        "export type AssetRef<K extends string = string> = string;",
    ] {
        assert!(ts.contains(line), "no `{line}` in:\n{ts}");
    }
}

#[test]
fn segmented_control_mounts_and_changes_selection_from_tsx() {
    let mut app = Tsx::start(
        r#"import { mount, signal, Text, View } from "nsh";
import {
  SegmentedControl,
  type SegmentedItem,
  type IconSrc,
  type IllustrationSrc,
  type AssetRef,
} from "rayx/components";

const _icon: IconSrc = "asset:/@app/icons/star.svg";
const _art: IllustrationSrc = "asset:/@app/illustrations/empty.svg";
const _theme: AssetRef<"theme"> = "asset:/@app/themes/dark.json";

const value = signal("all");
const items: SegmentedItem[] = [
  { value: "all", label: "All" },
  { value: "components", label: "Components" },
  { value: "icons", label: "Icons" },
];

mount(() => (
  <View class="flex-1">
    <SegmentedControl
      items={items}
      value={value()}
      size="small"
      onChange={({ value: next }) => value.set(next)}
    />
    <Text>{`active:${value()}`}</Text>
  </View>
));
"#,
    );

    assert!(
        app.wait(|s| s.text_node("active:all").is_some() && s.pressable("Icons").is_some()),
        "{:?}",
        app.screen.shown()
    );

    let icons = app.screen.pressable("Icons").unwrap();
    app.press_node(icons);
    assert!(
        app.wait(|s| s.text_node("active:icons").is_some()),
        "{:?}",
        app.screen.shown()
    );
}
