//! RayX's `KeymapEditor` in a Rust view, headless: renders toolbar, header, rows, and footer;
//! clicking a row's keystroke cell emits `StartRecord`, clicking source filters emits
//! `SourceFilterChange`, and the recording overlay emits `CommitRecord`/`CancelRecord`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use nsh_core::element::Root;
use nsh_core::prelude::*;
use nsh_engine::App;
use rayx_components::keymap_editor::{
    KeymapRecording, KeymapRow, KeymapSource, KeymapSourceFilter, keymap_editor,
};
use support::Running;

fn sample_rows() -> Vec<KeymapRow> {
    let mut grid = KeymapRow::new(
        "view.toggleGrid::canvas::alt-g",
        "view.toggleGrid",
        "Show Grid",
    );
    grid.category = "View".into();
    grid.sequence = "alt-g".into();
    grid.display_keystrokes = "Alt-G".into();
    grid.context = "canvas".into();
    grid.context_label = "Canvas".into();
    grid.source = KeymapSource::User;
    grid.has_default = true;
    grid.default_sequence = "mod-g".into();
    grid.is_custom = true;

    let mut undo = KeymapRow::new("edit.undo::::mod-z", "edit.undo", "Undo");
    undo.category = "Edit".into();
    undo.sequence = "mod-z".into();
    undo.display_keystrokes = "Ctrl-Z".into();
    undo.context_label = "Global".into();
    undo.source = KeymapSource::Default;
    undo.has_default = true;
    undo.default_sequence = "mod-z".into();

    vec![grid, undo]
}

#[component]
fn EditorHarness() -> impl IntoElement {
    let filter = signal(KeymapSourceFilter::All);
    let recording = signal(Option::<KeymapRecording>::None);
    let last_event = signal(SharedString::new_static("none"));

    view()
        .flex_1()
        .child(
            keymap_editor()
                .rows(sample_rows())
                .source_filter(filter)
                .total_count(2u32)
                .user_count(1u32)
                .user_binding_color("intent-alerts-warning-accent")
                .recording(recording)
                .on_source_filter_change(move |next: &KeymapSourceFilter| {
                    filter.set(*next);
                    last_event.set(SharedString::from(format!("filter:{next:?}")));
                })
                .on_start_record(move |row_id: &SharedString| {
                    let mut rec =
                        KeymapRecording::new(row_id.clone(), "view.toggleGrid", "Show Grid");
                    rec.context = "canvas".into();
                    rec.context_label = "Canvas".into();
                    rec.captured_sequence = Some("shift-g".into());
                    rec.captured_display = "Shift-G".into();
                    recording.set(Some(rec));
                    last_event.set(SharedString::from(format!("record:{row_id}")));
                })
                .on_commit_record(move || {
                    recording.set(None);
                    last_event.set(SharedString::new_static("committed"));
                })
                .on_reset_row(move |row_id: &SharedString| {
                    last_event.set(SharedString::from(format!("reset:{row_id}")));
                }),
        )
        .child(text(last_event))
}

#[test]
fn keymap_editor_renders_rows_and_emits_interaction_events() {
    let mut app = Running::start(App::Rust(Root::of::<EditorHarness>()));
    assert!(
        app.wait(|s| s.text_node("Show Grid").is_some() && s.text_node("Undo").is_some()),
        "{:?}",
        app.screen.shown()
    );
    assert!(app.screen.text_node("Alt-G").is_some());
    assert!(app.screen.text_node("Ctrl-Z").is_some());
    assert!(app.screen.text_node("Canvas").is_some());
    assert!(app.screen.text_node("Global").is_some());

    // Clicking the keystroke pill starts recording and mounts the overlay.
    app.press("Alt-G");
    assert!(
        app.wait(|s| s.text_node("Record Shortcut").is_some() && s.text_node("Shift-G").is_some()),
        "{:?}",
        app.screen.shown()
    );

    // Saving closes the overlay and emits CommitRecord.
    app.press("Save");
    assert!(
        app.wait(|s| s.text_node("committed").is_some() && s.text_node("Record Shortcut").is_none()),
        "{:?}",
        app.screen.shown()
    );

    // Clicking Reset emits ResetRow.
    app.press("Reset");
    assert!(
        app.wait(|s| s
            .text_node("reset:view.toggleGrid::canvas::alt-g")
            .is_some()),
        "{:?}",
        app.screen.shown()
    );
}
