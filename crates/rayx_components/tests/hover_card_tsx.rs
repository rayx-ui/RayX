//! RayX's `HoverCard` from TSX, headless: verifies generated TypeScript declarations and mounts
//! TSX `<HoverCard>` instances testing `openDelay={0}`, `disabled`, lazy child mounting, trigger
//! events while disabled, and controlled open/close.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use nsh_core::ApproxTextEngine;
use nsh_core::element::declarations;
use nsh_core::platform::{Environment, Idiom, Os};
use nsh_core::{EventKind, EventPayload, MouseEvent, NodeId};
use nsh_engine::{App, Bundle, Engine, EngineConfig};
use support::{ChannelSink, Screen, Tsx, described_of, ui_sets};

fn start_desktop_tsx(source: &str) -> Tsx {
    static APPS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let sets = ui_sets();
    let dir = std::env::temp_dir().join(format!(
        "rayx-components-hover-card-tsx-{}-{}",
        std::process::id(),
        APPS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("App.tsx");
    std::fs::write(&entry, source).unwrap();
    let described = described_of(&sets);
    let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        entry,
        ui_sets: described,
        ..Default::default()
    })
    .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let (tx, rx) = crossbeam_channel::unbounded();
    let config = EngineConfig {
        ui_sets: sets,
        environment: Environment {
            os: Os::Windows,
            idiom: Idiom::Desktop,
            ..Environment::default()
        },
        ..EngineConfig::new(App::Js(Bundle::source(bundle.code)))
    };
    let engine = Engine::start(config, || ApproxTextEngine, ChannelSink(tx)).unwrap();
    Tsx {
        engine,
        rx,
        screen: Screen::default(),
    }
}

fn find_mouse_enter_ancestor(screen: &Screen, label: &str) -> NodeId {
    let mut current = screen
        .text_node(label)
        .unwrap_or_else(|| panic!("no text node `{label}`: {:?}", screen.shown()));
    loop {
        if screen
            .events
            .get(&current)
            .is_some_and(|mask| mask.contains(EventKind::MouseEnter))
        {
            return current;
        }
        current = *screen
            .parents
            .get(&current)
            .unwrap_or_else(|| panic!("no MouseEnter ancestor for `{label}`"));
    }
}

#[test]
fn the_library_set_declares_hover_card_props_and_events() {
    let set = rayx_components::set();
    assert_eq!(set.specifier(), Some("rayx/components"));
    assert_eq!(set.undocumented(), Vec::<String>::new());
    let ts = declarations(&set, &[]);
    for line in [
        r#"export interface HoverCardProps extends Omit<ComponentHostProps, "class"> {"#,
        "  open?: boolean | null;",
        "  defaultOpen?: boolean;",
        "  openDelay?: number;",
        "  closeDelay?: number;",
        "  placement?: Placement;",
        "  align?: Align;",
        "  offset?: number;",
        "  margin?: number;",
        "  disabled?: boolean;",
        "  class?: string | null;",
        "  popupClass?: string | null;",
        "  popupStyle?: StyleProp<ViewStyle>;",
        "  trigger?: JSX.Element;",
        "  onOpenChange?: (event: { open: boolean; reason: OpenChangeReason }) => void;",
        "  onOpen?: () => void;",
        "  onClose?: () => void;",
        "  onPosition?: (event: { placement: Placement; bounds: Bounds; available: Size }) => void;",
        "export declare const HoverCard: ((props: HoverCardProps) => JSX.Element);",
    ] {
        assert!(ts.contains(line), "no `{line}` in:\n{ts}");
    }
}

#[test]
fn a_tsx_screen_opens_hover_card_immediately_and_respects_disabled_and_lazy_children() {
    let mut app = start_desktop_tsx(
        r#"import { mount, signal, Text, View } from "nsh";
import { Button, HoverCard } from "rayx/components";

const disabled = signal(false);
const controlledOpen = signal(false);
const triggerPresses = signal(0);
const lazyMounts = signal(0);
const lastEvent = signal("none");

function LazyBody() {
  lazyMounts.set(lazyMounts() + 1);
  return <Text>Preview content</Text>;
}

mount(() => (
  <View class="flex-1 p-gap4 gap-gap2 items-start">
    <HoverCard
      openDelay={0}
      closeDelay={0}
      placement="bottom"
      align="start"
      offset={4}
      disabled={disabled()}
      class="h-full"
      onOpenChange={({ open, reason }) => lastEvent.set(`${open}:${reason}`)}
      trigger={
        <View onPress={() => triggerPresses.set(triggerPresses() + 1)}>
          <Text>Hover Target</Text>
        </View>
      }
    >
      <LazyBody />
    </HoverCard>

    <HoverCard
      open={controlledOpen()}
      trigger={<View><Text>Controlled Target</Text></View>}
    >
      <Text>Controlled Body</Text>
    </HoverCard>

    <Button text="Toggle Disabled" onPress={() => disabled.set(!disabled())} />
    <Button text="Toggle Controlled" onPress={() => controlledOpen.set(!controlledOpen())} />
    <Text>{`presses:${triggerPresses()}`}</Text>
    <Text>{`mounts:${lazyMounts()}`}</Text>
    <Text>{`event:${lastEvent()}`}</Text>
  </View>
));
"#,
    );

    assert!(
        app.wait(|s| {
            s.text_node("Hover Target").is_some()
                && s.text_node("mounts:0").is_some()
                && s.text_node("presses:0").is_some()
        }),
        "{:?}",
        app.screen.shown()
    );
    // Lazy children do not mount before the card opens.
    assert!(app.screen.text_node("Preview content").is_none());
    assert!(app.screen.text_node("Controlled Body").is_none());

    // Hovering with openDelay={0} opens immediately and mounts lazy children.
    let target = find_mouse_enter_ancestor(&app.screen, "Hover Target");
    app.engine.control().dispatch_event(
        target,
        EventKind::MouseEnter,
        EventPayload::Mouse(MouseEvent::default()),
    );
    assert!(
        app.wait(|s| {
            s.text_node("Preview content").is_some()
                && s.text_node("mounts:1").is_some()
                && s.text_node("event:true:hover").is_some()
        }),
        "{:?}",
        app.screen.shown()
    );

    // Disabling closes the open card immediately while keeping the trigger pressable.
    let toggle_disabled = app.screen.pressable("Toggle Disabled").unwrap();
    app.press_node(toggle_disabled);
    assert!(
        app.wait(|s| s.text_node("Preview content").is_none()),
        "{:?}",
        app.screen.shown()
    );

    // Trigger press still fires while HoverCard is disabled.
    let hover_trigger_press = app.screen.pressable("Hover Target").unwrap();
    app.press_node(hover_trigger_press);
    assert!(
        app.wait(|s| s.text_node("presses:1").is_some()),
        "{:?}",
        app.screen.shown()
    );

    // Hovering while disabled does not open or mount children.
    app.engine.control().dispatch_event(
        target,
        EventKind::MouseLeave,
        EventPayload::Mouse(MouseEvent::default()),
    );
    app.engine.control().dispatch_event(
        target,
        EventKind::MouseEnter,
        EventPayload::Mouse(MouseEvent::default()),
    );
    assert!(
        app.wait(|s| s.text_node("mounts:1").is_some() && s.text_node("Preview content").is_none()),
        "{:?}",
        app.screen.shown()
    );

    // Controlled open/close works independently.
    let toggle_controlled = app.screen.pressable("Toggle Controlled").unwrap();
    app.press_node(toggle_controlled);
    assert!(
        app.wait(|s| s.text_node("Controlled Body").is_some()),
        "{:?}",
        app.screen.shown()
    );
    app.press_node(toggle_controlled);
    assert!(
        app.wait(|s| s.text_node("Controlled Body").is_none()),
        "{:?}",
        app.screen.shown()
    );
}
