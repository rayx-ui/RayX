//! RayX's `Menu`, `ContextMenu`, `MenuItem`, and `MenuSeparator` from TSX, headless:
//! verifies generated TypeScript declarations (including `MenuItemVariant` and
//! `onSelect?: (event: { modifiers: Modifiers }) => void`) and mounts + interacts with a TSX
//! screen importing all four components from `rayx/components`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use nsh_core::element::declarations;
use nsh_core::keymap::Modifiers;
use nsh_core::{ContextMenuEvent, EventKind, EventPayload, PressEvent};
use support::Tsx;

#[test]
fn the_library_set_declares_menu_components_and_modifier_select_signature() {
    let set = rayx_components::set();
    assert_eq!(set.specifier(), Some("rayx/components"));
    assert_eq!(set.undocumented(), Vec::<String>::new());
    let ts = declarations(&set, &[]);
    for line in [
        r#"export type MenuItemVariant = "default" | "danger";"#,
        "export interface MenuProps extends ComponentHostProps {",
        "  open?: boolean | null;",
        "  defaultOpen?: boolean;",
        "  trigger?: JSX.Element;",
        "  onOpenChange?: (event: { open: boolean; reason: OpenChangeReason }) => void;",
        "export declare const Menu: ((props: MenuProps) => JSX.Element);",
        "export interface ContextMenuProps extends ComponentHostProps {",
        "export declare const ContextMenu: ((props: ContextMenuProps) => JSX.Element);",
        "export interface MenuItemProps extends ComponentHostProps {",
        "  label?: string;",
        "  shortcut?: string | null;",
        "  icon?: JSX.Element;",
        "  variant?: MenuItemVariant;",
        "  checked?: boolean | null;",
        "  disabled?: boolean;",
        "  closeOnSelect?: boolean;",
        "  onSelect?: (event: { modifiers: Modifiers }) => void;",
        "export declare const MenuItem: ((props: MenuItemProps) => JSX.Element);",
        "export interface MenuSeparatorProps extends ComponentHostProps {",
        "export declare const MenuSeparator: ((props: MenuSeparatorProps) => JSX.Element);",
    ] {
        assert!(ts.contains(line), "no `{line}` in:\n{ts}");
    }
}

#[test]
fn a_tsx_screen_mounts_menu_and_context_menu_and_receives_modifiers() {
    let mut app = Tsx::start(
        r#"import { mount, signal, Text, View } from "nsh";
import { ContextMenu, Menu, MenuItem, MenuSeparator } from "rayx/components";

const selected = signal("none");
const ctxSelected = signal("ctx-none");

mount(() => (
  <View class="flex-1 p-gap4 gap-gap2 items-start">
    <Menu trigger={<View><Text>Open Menu</Text></View>}>
      <MenuItem
        label="Expand"
        shortcut="Alt+Click"
        onSelect={(e) => selected.set(`expand:${e.modifiers.alt ? "alt" : "plain"}`)}
      />
      <MenuSeparator />
      <MenuItem
        label="Delete"
        variant="danger"
        onSelect={() => selected.set("delete")}
      />
    </Menu>
    <ContextMenu trigger={<View><Text>Context Target</Text></View>}>
      <MenuItem
        label="Duplicate"
        shortcut="Ctrl+D"
        onSelect={(e) => ctxSelected.set(`dup:${e.modifiers.alt ? "alt" : "plain"}`)}
      />
    </ContextMenu>
    <Text>{`selected:${selected()}`}</Text>
    <Text>{`ctx:${ctxSelected()}`}</Text>
  </View>
));
"#,
    );

    assert!(
        app.wait(|s| {
            s.pressable("Open Menu").is_some()
                && s.text_node("Context Target").is_some()
                && s.text_node("selected:none").is_some()
        }),
        "{:?}",
        app.screen.shown()
    );

    // Open Menu and Alt+Click "Expand".
    let trigger = app.screen.pressable("Open Menu").unwrap();
    app.press_node(trigger);
    assert!(
        app.wait(|s| s.pressable("Expand").is_some() && s.pressable("Delete").is_some()),
        "{:?}",
        app.screen.shown()
    );

    let expand = app.screen.pressable("Expand").unwrap();
    let alt_press = EventPayload::Press(PressEvent {
        modifiers: Modifiers {
            alt: true,
            ..Modifiers::default()
        },
        clicks: 1,
        ..PressEvent::default()
    });
    for kind in [EventKind::PressDown, EventKind::PressUp, EventKind::Press] {
        app.engine
            .control()
            .dispatch_event(expand, kind, alt_press.clone());
    }
    assert!(
        app.wait(|s| s.text_node("selected:expand:alt").is_some()),
        "{:?}",
        app.screen.shown()
    );

    // Open ContextMenu via ContextMenu event and select "Duplicate".
    let mut target = app.screen.text_node("Context Target").unwrap();
    let ctx_node = loop {
        if app
            .screen
            .events
            .get(&target)
            .is_some_and(|e| e.contains(EventKind::ContextMenu))
        {
            break target;
        }
        target = *app
            .screen
            .parents
            .get(&target)
            .expect("context menu listener ancestor");
    };
    app.engine.control().dispatch_event(
        ctx_node,
        EventKind::ContextMenu,
        EventPayload::ContextMenu(ContextMenuEvent::default()),
    );
    assert!(
        app.wait(|s| s.pressable("Duplicate").is_some()),
        "{:?}",
        app.screen.shown()
    );

    let duplicate = app.screen.pressable("Duplicate").unwrap();
    app.press_node(duplicate);
    assert!(
        app.wait(|s| s.text_node("ctx:dup:plain").is_some()),
        "{:?}",
        app.screen.shown()
    );
}
