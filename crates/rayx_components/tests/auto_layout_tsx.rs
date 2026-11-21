//! RayX's `AutoLayout` from TSX, headless: the library's set declares it with Figma's auto layout
//! props in camel case and their types; a TSX screen lays out a row with a gap and padding role,
//! centered children, a nested `AutoLayout` that fills it, a `Button` that fills by its class
//! (`flex-1`), and a component's `Slot` holding an `AutoLayout` of several things. The expected
//! values are those RayX's default theme gives the roles.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use nsh_core::element::declarations;
use rayx_components::slot::warned;
use support::{Tsx, number};

#[test]
fn the_library_set_declares_figmas_auto_layout_props() {
    let set = rayx_components::set();
    assert_eq!(set.undocumented(), Vec::<String>::new());
    let ts = declarations(&set, &[]);
    for line in [
        r#"export type AutoLayoutDirection = "vertical" | "horizontal";"#,
        r#"export type AutoLayoutAlign = "topLeft" | "top" | "topRight" | "left" | "center" | "right" | "bottomLeft" | "bottom" | "bottomRight";"#,
        r#"export type AutoLayoutSizing = "hug" | "fill" | "fixed";"#,
        r#"export type SpacingGap = "none" | "gap1" | "gap2" | "gap3" | "gap4" | "gap5" | "gap6" | "gap7" | "gap8";"#,
        r#"export type SpacingInset = "none" | "inset1" | "inset2" | "inset3" | "inset4" | "inset5" | "inset6" | "inset7" | "inset8";"#,
        "  direction?: AutoLayoutDirection;",
        "  gap?: SpacingGap;",
        "  spaceBetween?: boolean;",
        "  padding?: SpacingInset;",
        "  paddingTop?: SpacingInset",
        "  paddingRight?: SpacingInset",
        "  paddingBottom?: SpacingInset",
        "  paddingLeft?: SpacingInset",
        "  align?: AutoLayoutAlign;",
        "  wrap?: boolean;",
        "  wrapGap?: SpacingGap",
        "  width?: AutoLayoutSizing;",
        "  height?: AutoLayoutSizing;",
        "  fixedWidth?: number",
        "  fixedHeight?: number",
        "  children?: JSX.Element;",
        "export declare const AutoLayout: ((props: AutoLayoutProps) => JSX.Element);",
    ] {
        assert!(ts.contains(line), "no `{line}` in:\n{ts}");
    }
}

const TSX: &str = r#"import { mount, Text, View } from "nsh";
import { AutoLayout, Button, Slot } from "rayx/components";

function Panel(props: { actions?: JSX.Element }) {
  return (
    <AutoLayout gap="gap2">
      <Text>Panel</Text>
      <Slot name="actions">{props.actions}</Slot>
    </AutoLayout>
  );
}

mount(() => (
  <View class="items-start gap-[20px]">
    <AutoLayout direction="horizontal" gap="gap4" padding="inset3" align="center" width="fixed" fixedWidth={300} height="fixed" fixedHeight={80}>
      <Text>One</Text>
      <AutoLayout width="fill" height="fill"><Text>Fills</Text></AutoLayout>
      <Text>Two</Text>
    </AutoLayout>
    <AutoLayout direction="horizontal" width="fixed" fixedWidth={300}>
      <Button text="Wide" class="flex-1" />
      <Text>Z</Text>
    </AutoLayout>
    <Panel
      actions={
        <AutoLayout direction="horizontal" gap="gap1">
          <Button text="Cancel" />
          <Button text="Save" variant="primary" />
        </AutoLayout>
      }
    />
  </View>
));
"#;

#[test]
fn a_tsx_screen_lays_out_rows_with_roles_and_fills() {
    let mut app = Tsx::start(TSX);
    assert!(
        app.wait(|s| ["One", "Fills", "Two", "Z", "Panel", "Save"]
            .iter()
            .all(|label| s.text_frame(label).is_some())),
        "{:?}",
        app.screen.shown()
    );
    let s = &app.screen;
    let (gap4, inset3) = (number("spacing-gap4"), number("spacing-inset3"));
    let row = s.around("One").unwrap();
    assert_eq!(s.frames[&row], (300.0, 80.0));
    let at = |label: &str| {
        let id = s.text_node(label).unwrap();
        (s.origins[&id], s.frames[&id])
    };
    let ((x1, y1), (w1, h1)) = at("One");
    assert_eq!(x1, inset3, "padding inset3");
    assert!((y1 - (80.0 - h1) / 2.0).abs() <= 1.0, "centered: {y1}");
    // The nested AutoLayout fills the row between the texts, its whole height less the padding.
    let fills = s.around("Fills").unwrap();
    let ((x2, _), (w2, _)) = at("Two");
    assert_eq!(s.origins[&fills].0, x1 + w1 + gap4);
    assert_eq!(
        s.frames[&fills].0,
        300.0 - 2.0 * inset3 - w1 - w2 - 2.0 * gap4
    );
    assert_eq!(s.frames[&fills].1, 80.0 - 2.0 * inset3);
    assert_eq!(x2, 300.0 - inset3 - w2);

    // `class="flex-1"` fills a `Button` along the row.
    let wide = s.button("Wide").unwrap();
    let (_, (wz, _)) = at("Z");
    assert_eq!(s.frames[&wide].0, 300.0 - wz);

    // An `AutoLayout` of two buttons in a component's slot: both show, and the slot warns not.
    let cancel = s.button("Cancel").unwrap();
    let save = s.button("Save").unwrap();
    assert_eq!(s.parents[&cancel], s.parents[&save], "in one AutoLayout");
    assert!(!warned().iter().any(|n| n == "actions"), "{:?}", warned());
}
