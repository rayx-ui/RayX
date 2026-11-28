//! RayX's `Dialog` from TSX, headless: verifies generated TypeScript declarations and mounts a
//! TSX `<Dialog role="alertdialog">` with title, description, body, footer actions, and backdrop
//! dismissal.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use nsh_core::EventKind;
use nsh_core::element::declarations;
use support::Tsx;

#[test]
fn the_library_set_declares_dialog_props_and_events() {
    let set = rayx_components::set();
    assert_eq!(set.specifier(), Some("rayx/components"));
    assert_eq!(set.undocumented(), Vec::<String>::new());
    let ts = declarations(&set, &[]);
    for line in [
        "export interface DialogProps extends ComponentHostProps {",
        "  open?: boolean;",
        "  title?: string | null;",
        "  description?: string | null;",
        "  role?: AccessibilityRole | null;",
        "  animation?: ModalAnimation;",
        "  closeOnEscape?: boolean;",
        "  closeOnBackdropPress?: boolean;",
        "  footer?: JSX.Element;",
        "  accessibilityLabel?: string | null;",
        "  accessibilityDescription?: string | null;",
        "  onOpenChange?: (event: { open: boolean; reason: OpenChangeReason }) => void;",
        "  onOpen?: () => void;",
        "  onClose?: () => void;",
        "  onCancel?: () => void;",
        "export declare const Dialog: ((props: DialogProps) => JSX.Element);",
    ] {
        assert!(ts.contains(line), "no `{line}` in:\n{ts}");
    }
}

#[test]
fn a_tsx_screen_opens_confirms_and_dismisses_alert_dialog() {
    let mut app = Tsx::start(
        r#"import { mount, signal, Text, View } from "nsh";
import { Button, Dialog } from "rayx/components";

const open = signal(false);
const status = signal("idle");

mount(() => (
  <View class="flex-1 p-gap4 gap-gap2 items-start">
    <Button text="Delete Item" variant="danger" onPress={() => open.set(true)} />
    <Dialog
      open={open()}
      role="alertdialog"
      title="Move to trash?"
      description="You can restore this item later."
      onOpenChange={({ open: next, reason }) => {
        open.set(next);
        if (!next) status.set(`dismissed:${reason}`);
      }}
      footer={
        <View class="flex-row gap-gap2">
          <Button text="Cancel" onPress={() => { open.set(false); status.set("cancelled"); }} />
          <Button text="Move to Trash" variant="danger" onPress={() => { open.set(false); status.set("trashed"); }} />
        </View>
      }
    >
      <Text>Extra details</Text>
    </Dialog>
    <Text>{`status:${status()}`}</Text>
  </View>
));
"#,
    );

    assert!(
        app.wait(|s| s.pressable("Delete Item").is_some() && s.text_node("status:idle").is_some()),
        "{:?}",
        app.screen.shown()
    );
    assert!(app.screen.text_node("Move to trash?").is_none());

    // Open dialog from TSX button.
    let trigger = app.screen.pressable("Delete Item").unwrap();
    app.press_node(trigger);
    assert!(
        app.wait(|s| {
            s.text_node("Move to trash?").is_some()
                && s.text_node("You can restore this item later.").is_some()
                && s.text_node("Extra details").is_some()
                && s.pressable("Move to Trash").is_some()
        }),
        "{:?}",
        app.screen.shown()
    );

    // Confirm action via footer button.
    let confirm = app.screen.pressable("Move to Trash").unwrap();
    app.press_node(confirm);
    assert!(
        app.wait(|s| {
            s.text_node("status:trashed").is_some() && s.text_node("Move to trash?").is_none()
        }),
        "{:?}",
        app.screen.shown()
    );

    // Re-open and dismiss via scrim backdrop press.
    let trigger = app.screen.pressable("Delete Item").unwrap();
    app.press_node(trigger);
    assert!(app.wait(|s| s.text_node("Move to trash?").is_some()));

    let backdrop = app
        .screen
        .events
        .iter()
        .find(|(id, mask)| {
            mask.contains(EventKind::Press)
                && app
                    .screen
                    .styles
                    .get(id)
                    .is_some_and(|st| st.background_color.0 == support::color("surface-scrim"))
        })
        .map(|(id, _)| *id)
        .expect("scrim backdrop pressable");
    app.press_node(backdrop);
    assert!(
        app.wait(|s| {
            s.text_node("status:dismissed:backdropPress").is_some()
                && s.text_node("Move to trash?").is_none()
        }),
        "{:?}",
        app.screen.shown()
    );
}
