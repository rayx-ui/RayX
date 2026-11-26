//! RayX's `KeymapEditor` from TSX, headless: verifies TypeScript declarations exported by
//! `rayx/components` and full TSX mounting & interaction.

#![allow(unsafe_code, clippy::expect_used, clippy::unwrap_used)]

mod support;

use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use nsh_core::element::declarations;
use nsh_core::{ApproxTextEngine, MountBatch};
use nsh_engine::{App, Bundle, Engine, EngineConfig};
use support::{ChannelSink, Screen, described, ui_sets};

const TSX: &str = r#"import { mount, signal, Text, View } from "nsh";
import { KeymapEditor, type KeymapRow, type KeymapSourceFilter } from "rayx/components";

const filter = signal<KeymapSourceFilter>("all");
const status = signal("idle");
const rows: KeymapRow[] = [
  {
    id: "view.toggleGrid::canvas::mod-g",
    action: "view.toggleGrid",
    commandName: "Show Grid",
    category: "View",
    sequence: "mod-g",
    displayKeystrokes: "Ctrl-G",
    context: "canvas",
    contextLabel: "Canvas",
    source: "default",
    hasDefault: true,
    defaultSequence: "mod-g",
    isUnbound: false,
    isCustom: false,
  },
];

mount(() => (
  <View class="flex-1">
    <KeymapEditor
      rows={rows}
      query=""
      sourceFilter={filter()}
      totalCount={1}
      userCount={0}
      userBindingColor="intent-alerts-warning-accent"
      onSourceFilterChange={({ filter: next }) => {
        filter.set(next);
        status.set(`filter:${next}`);
      }}
      onStartRecord={({ rowId }) => status.set(`record:${rowId}`)}
    />
    <Text>{status()}</Text>
  </View>
));
"#;

#[test]
fn the_library_set_declares_keymap_editor_types_and_events_for_tsx() {
    let set = rayx_components::set();
    assert_eq!(set.undocumented(), Vec::<String>::new());
    let ts = declarations(&set, &[]);
    for line in [
        r#"export type KeymapSource = "default" | "user";"#,
        r#"export type KeymapSourceFilter = "all" | "user" | "default";"#,
        "export interface KeymapRow {",
        "export interface KeymapRecording {",
        "export interface KeymapEditorProps extends ComponentHostProps {",
        "  userBindingColor?: string;",
        "  onQueryChange?: (event: { query: string }) => void;",
        "  onSourceFilterChange?: (event: { filter: KeymapSourceFilter }) => void;",
        "  onStartRecord?: (event: { rowId: string }) => void;",
        "  onCommitRecord?: () => void;",
        "  onClearRecord?: () => void;",
        "  onCancelRecord?: () => void;",
        "  onResetAll?: () => void;",
        "export declare const KeymapEditor: ((props: KeymapEditorProps) => JSX.Element);",
    ] {
        assert!(ts.contains(line), "no `{line}` in:\n{ts}");
    }
}

#[test]
fn a_tsx_screen_mounts_and_interacts_with_keymap_editor() {
    let dir = std::env::temp_dir().join(format!("rayx-keymap-editor-tsx-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("App.tsx");
    std::fs::write(&entry, TSX).unwrap();
    let sets = ui_sets();
    let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        development: false,
        app: nsh_bundler::AppInfo::default(),
        asset_root_from: None,
        app_dir: None,
        entry,
        minify: false,
        split: false,
        ui_sets: described(),
    })
    .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let bytecode = nsh_js::compile_bytecode_with(&bundle.code, false, &described()).unwrap();
    let (tx, rx): (_, Receiver<MountBatch>) = crossbeam_channel::unbounded();
    // SAFETY: compiled just now by this runtime.
    let app = App::Js(unsafe { Bundle::bytecode(bytecode) });
    let config = EngineConfig {
        ui_sets: sets,
        ..EngineConfig::new(app)
    };
    let engine = Engine::start(config, || ApproxTextEngine, ChannelSink(tx)).unwrap();
    let mut screen = Screen::default();

    let wait = |screen: &mut Screen, done: &dyn Fn(&Screen) -> bool| {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done(screen) {
            match rx.recv_deadline(deadline) {
                Ok(batch) => screen.apply(&batch),
                Err(_) => return false,
            }
        }
        while let Ok(batch) = rx.recv_timeout(Duration::from_millis(50)) {
            screen.apply(&batch);
        }
        true
    };

    assert!(
        wait(&mut screen, &|s| s.text_node("Show Grid").is_some()
            && s.pressable("Ctrl-G").is_some()),
        "{:?}",
        screen.shown()
    );

    let id = screen.pressable("Ctrl-G").unwrap();
    for kind in [
        nsh_core::EventKind::PressDown,
        nsh_core::EventKind::PressUp,
        nsh_core::EventKind::Press,
    ] {
        engine
            .control()
            .dispatch_event(id, kind, nsh_core::EventPayload::None);
    }
    assert!(
        wait(&mut screen, &|s| {
            s.text_node("record:view.toggleGrid::canvas::mod-g")
                .is_some()
        }),
        "{:?}",
        screen.shown()
    );
    drop(engine);
}
