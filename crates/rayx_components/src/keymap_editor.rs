//! `KeymapEditor`: RayX's standardized keyboard shortcut editor component.
//!
//! A dense, theme-aware table of commands and their keybindings with search filtering,
//! source filters (`All`, `User`, `Default`), per-row Edit/Reset/Unbind actions, `Reset All`,
//! and a focused live keystroke capture overlay with inline conflict warnings.
//!
//! Every visual property comes from RayX's theme roles (`theme::roles`), so light/dark modes,
//! accents, density, and custom themes apply automatically. The component is pure and controlled
//! via props and events so any RayX app can embed it in a window, modal, tab, or panel.

use nsh_core::element::{Token, Typography, each, element_ref, input, scroll};
use nsh_core::keyboard::{KeyCapture, KeyEvent};
use nsh_core::prelude::*;

use crate::button::{ButtonVariant, button};
use crate::theme::roles::{
    BORDER_WIDTH_OUTLINE, INTENT_ALERTS_WARNING_ACCENT, INTENT_ALERTS_WARNING_BG_TRANSLUCENT,
    INTENT_DANGER_BG, INTENT_DANGER_BG_TRANSLUCENT, INTENT_NEUTRAL_BG, INTENT_NEUTRAL_BG_HOVER,
    INTENT_PRIMARY_ACCENT, INTENT_PRIMARY_BG_TRANSLUCENT, RADIUS_DEFAULT, RADIUS_SMALL,
    RADIUS_SMALL_INNER, SPACING_GAP1, SPACING_GAP2, SPACING_GAP3, SPACING_INSET1, SPACING_INSET2,
    SPACING_INSET3, SPACING_INSET4, SPACING_INSET6, SPACING_SMALL_LVL5_HEIGHT,
    SPACING_SMALL_LVL6_HEIGHT, SURFACE_ALTERNATE_ROW_BG, SURFACE_APP, SURFACE_BORDER,
    SURFACE_BORDER_MUTED, SURFACE_OVERLAY, SURFACE_PANEL1, SURFACE_PANEL2, SURFACE_SCRIM,
    SURFACE_TEXT, SURFACE_TEXT_DANGER, SURFACE_TEXT_MUTED, SURFACE_TEXT_SUBTLE,
    SURFACE_TEXT_WARNING, TEXT_DEFAULT_CODE, TEXT_DEFAULT_TITLE1_BOLD, TEXT_SMALL_CODE,
    TEXT_SMALL_REGULAR, TEXT_SMALL_TITLE1_BOLD,
};
use crate::theme::{ComponentSize, value};

/// Where an effective keybinding originates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PropEnum)]
pub enum KeymapSource {
    /// Inherited from the application or framework default keymap.
    #[default]
    Default,
    /// Customized or explicitly unbound in the user's keymap layer.
    User,
}

/// Which source rows are visible in the keymap table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PropEnum)]
pub enum KeymapSourceFilter {
    /// Show all rows regardless of source.
    #[default]
    All,
    /// Show only user-customized rows.
    User,
    /// Show only default rows.
    Default,
}

/// A single row displayed in [`KeymapEditor`].
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
#[non_exhaustive]
pub struct KeymapRow {
    /// Stable row identifier unique within the table.
    pub id: SharedString,
    /// Command/action identifier (for example `"view.toggleGrid"`).
    pub action: SharedString,
    /// Human-readable command title (for example `"Show Grid"`).
    pub command_name: SharedString,
    /// Command category (for example `"View"`).
    #[prop(default)]
    pub category: SharedString,
    /// Canonical key sequence (for example `"mod-g"`), or empty when unbound.
    #[prop(default)]
    pub sequence: SharedString,
    /// OS-aware display string (for example `"Ctrl-G"` or `"Cmd-G"`), or empty when unbound.
    #[prop(default)]
    pub display_keystrokes: SharedString,
    /// Context identifier (`""` for global).
    #[prop(default)]
    pub context: SharedString,
    /// Human-readable context label (for example `"Global"`, `"Canvas"`).
    #[prop(default)]
    pub context_label: SharedString,
    /// Whether this binding comes from the user layer or defaults.
    #[prop(default)]
    pub source: KeymapSource,
    /// Whether a framework or application default exists for this action and context.
    #[prop(default)]
    pub has_default: bool,
    /// Default canonical sequence, if one exists.
    #[prop(default)]
    pub default_sequence: SharedString,
    /// Whether this command currently has no bound keystroke.
    #[prop(default)]
    pub is_unbound: bool,
    /// Whether the user customized or explicitly unbound this row.
    #[prop(default)]
    pub is_custom: bool,
}

impl KeymapRow {
    /// Creates a row with `id`, `action`, and `command_name`.
    pub fn new(
        id: impl Into<SharedString>,
        action: impl Into<SharedString>,
        command_name: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            action: action.into(),
            command_name: command_name.into(),
            ..Self::default()
        }
    }
}

/// Active live keystroke recording state displayed in [`KeymapEditor`]'s capture overlay.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
#[non_exhaustive]
pub struct KeymapRecording {
    /// Identifier of the row currently being edited.
    pub row_id: SharedString,
    /// Action identifier being bound.
    pub action: SharedString,
    /// Display title of the command being bound.
    pub command_name: SharedString,
    /// Context identifier where the binding applies.
    #[prop(default)]
    pub context: SharedString,
    /// Human-readable context label.
    #[prop(default)]
    pub context_label: SharedString,
    /// Canonical sequence captured so far, or `None` while waiting for a keystroke.
    #[prop(skip_none)]
    pub captured_sequence: Option<SharedString>,
    /// Human-readable display label of the captured shortcut.
    #[prop(default)]
    pub captured_display: SharedString,
    /// Inline conflict warning message if another command in the same context uses the shortcut.
    #[prop(skip_none)]
    pub conflict_message: Option<SharedString>,
}

impl KeymapRecording {
    /// Creates a recording state for `row_id`, `action`, and `command_name`.
    pub fn new(
        row_id: impl Into<SharedString>,
        action: impl Into<SharedString>,
        command_name: impl Into<SharedString>,
    ) -> Self {
        Self {
            row_id: row_id.into(),
            action: action.into(),
            command_name: command_name.into(),
            ..Self::default()
        }
    }
}

/// The props of [`KeymapEditor`].
#[derive(Debug, Clone, PartialEq, Props)]
pub struct KeymapEditorProps {
    /// Visible rows to render in the table (already filtered and sorted by the host or view model).
    #[prop(default)]
    pub rows: Prop<Vec<KeymapRow>>,
    /// Current search query shown in the filter field.
    #[prop(default)]
    pub query: Prop<SharedString>,
    /// Active source filter pill (`"all"`, `"user"`, or `"default"`).
    #[prop(default)]
    pub source_filter: Prop<KeymapSourceFilter>,
    /// Total unfiltered binding count (when `0`, falls back to `rows.len()`).
    #[prop(default)]
    pub total_count: Prop<u32>,
    /// Number of user-customized bindings; disables `Reset All` when `0`.
    #[prop(default)]
    pub user_count: Prop<u32>,
    /// Currently selected row identifier, if any.
    pub selected_row_id: Prop<Option<SharedString>>,
    /// Active live keystroke recording state; when `Some`, shows the focused capture overlay.
    pub recording: Prop<Option<KeymapRecording>>,
    /// Error message banner shown above the table, if any.
    pub error: Prop<Option<SharedString>>,
    /// Whether an asynchronous persistence operation is currently in progress.
    #[prop(default)]
    pub saving: Prop<bool>,
    /// Placeholder text displayed in the filter field.
    #[prop(default = SharedString::new_static("Filter commands, keystrokes, or context…"))]
    pub filter_placeholder: Prop<SharedString>,
    /// Theme color role or token name used to highlight text on user-rebound (`User`) rows.
    /// Accepts any theme role (for example `"intent-alerts-warning-accent"`,
    /// `"surface-text-warning"`, `"surface-link"`) or color token name (for example
    /// `"theme-colors-palettes-orange-600"`).
    #[prop(default = SharedString::new_static("intent-alerts-warning-accent"))]
    pub user_binding_color: Prop<SharedString>,
}

/// The events emitted by [`KeymapEditor`].
#[derive(Debug, Clone, PartialEq, Event)]
pub enum KeymapEditorEvent {
    /// The user edited the search filter input.
    #[args(query)]
    QueryChange(SharedString),
    /// The user clicked a source filter pill (`All`, `User`, or `Default`).
    #[args(filter)]
    SourceFilterChange(KeymapSourceFilter),
    /// The user clicked a row to select it.
    #[args(row_id)]
    SelectRow(SharedString),
    /// The user clicked a row's keystroke pill to start recording a new shortcut.
    #[args(row_id)]
    StartRecord(SharedString),
    /// A hardware key went down while the live keystroke recorder was focused.
    #[args(key, code, control, alt, shift, meta, repeat)]
    RecordKeyDown(SharedString, SharedString, bool, bool, bool, bool, bool),
    /// The user confirmed saving the captured shortcut.
    CommitRecord,
    /// The user cleared the currently captured shortcut in the recorder.
    ClearRecord,
    /// The user cancelled recording.
    CancelRecord,
    /// The user clicked `Reset` on a customized row to restore its default binding.
    #[args(row_id)]
    ResetRow(SharedString),
    /// The user clicked `Unbind` on a bound row to remove its shortcut.
    #[args(row_id)]
    UnbindRow(SharedString),
    /// The user clicked `Reset All` to clear every user override.
    ResetAll,
}

/// RayX's standardized keyboard shortcut editor: searchable, filterable table of commands,
/// keystrokes, contexts, and sources, with per-row Reset/Unbind controls and a live keystroke
/// capture dialog with inline conflict detection.
///
/// The component is pure and theme-aware via RayX's semantic roles; embed it inside any window,
/// modal, tab, or panel and wire its events to your keymap view model or `InputMapService`.
#[component]
pub fn KeymapEditor(props: KeymapEditorProps, cx: Scope<KeymapEditorEvent>) -> impl IntoElement {
    let look = computed(Look::of);
    let user_color = computed({
        let color_token = props.user_binding_color;
        move || {
            let name = color_token.get();
            resolve_color_token(name.as_str(), look.with(|l| l.warning_accent))
        }
    });

    let toolbar = build_toolbar(props.clone(), cx, look);
    let error_banner = build_error_banner(props.clone(), look);
    let table_header = build_table_header(look);
    let table_body = build_table_body(props.clone(), cx, look, user_color);
    let footer = build_footer(props.clone(), look);
    let recorder = build_recorder(props, cx, look);

    view()
        .test_id("keymap-editor")
        .flex_1()
        .relative()
        .bg(move || look.with(|l| l.surface_app))
        .child(toolbar)
        .child(error_banner)
        .child(table_header)
        .child(table_body)
        .child(footer)
        .child(recorder)
}

fn build_toolbar(
    props: KeymapEditorProps,
    cx: Scope<KeymapEditorEvent>,
    look: Memo<Look>,
) -> impl IntoElement {
    let source_pill = move |filter: KeymapSourceFilter,
                            test_id_str: &'static str,
                            label: Box<dyn Fn() -> SharedString>| {
        button()
            .test_id(test_id_str)
            .text(label)
            .size(ComponentSize::Small)
            .variant(move || {
                if props.source_filter.get() == filter {
                    ButtonVariant::Primary
                } else {
                    ButtonVariant::Ghost
                }
            })
            .on_press(move || cx.emit(KeymapEditorEvent::SourceFilterChange(filter)))
    };

    let all_pill = source_pill(
        KeymapSourceFilter::All,
        "keymap-source-all",
        Box::new(|| SharedString::new_static("All")),
    );
    let user_pill = source_pill(
        KeymapSourceFilter::User,
        "keymap-source-user",
        Box::new(move || {
            let count = props.user_count.get();
            if count > 0 {
                SharedString::from(format!("User ({count})"))
            } else {
                SharedString::new_static("User")
            }
        }),
    );
    let default_pill = source_pill(
        KeymapSourceFilter::Default,
        "keymap-source-default",
        Box::new(|| SharedString::new_static("Default")),
    );

    view()
        .flex_row()
        .items_center()
        .flex_shrink_0()
        .bg(move || look.with(|l| l.surface_panel1))
        .border_b(move || px(look.with(|l| l.outline_width)))
        .border_color(move || look.with(|l| l.border_muted))
        .px(move || px(look.with(|l| l.inset3)))
        .py(move || px(look.with(|l| l.inset2)))
        .gap(move || px(look.with(|l| l.gap2)))
        .child(
            view()
                .flex_1()
                .flex_row()
                .items_center()
                .h(move || px(look.with(|l| l.control_height)))
                .px(move || px(look.with(|l| l.inset2)))
                .rounded(move || px(look.with(|l| l.radius_small)))
                .bg(move || look.with(|l| l.surface_panel2))
                .border(move || px(look.with(|l| l.outline_width)))
                .border_color(move || look.with(|l| l.border))
                .child(
                    input()
                        .test_id("keymap-filter")
                        .label("Filter keyboard shortcuts")
                        .flex_1()
                        .h_full()
                        .value(move || props.query.get().to_string())
                        .placeholder(move || props.filter_placeholder.get().to_string())
                        .placeholder_color(move || look.with(|l| l.text_subtle))
                        .text_color(move || look.with(|l| l.text))
                        .typography(move || look.with(|l| l.text_small.clone()))
                        .on_change(move |val: &str| {
                            cx.emit(KeymapEditorEvent::QueryChange(SharedString::from(
                                val.to_owned(),
                            )));
                        }),
                ),
        )
        .child(
            view()
                .flex_row()
                .items_center()
                .flex_shrink_0()
                .gap(move || px(look.with(|l| l.gap1)))
                .child(all_pill)
                .child(user_pill)
                .child(default_pill),
        )
        .child(
            button()
                .test_id("keymap-reset-all")
                .text("Reset All")
                .size(ComponentSize::Small)
                .variant(ButtonVariant::Ghost)
                .outlined(true)
                .disabled(move || props.user_count.get() == 0 || props.saving.get())
                .on_press(move || cx.emit(KeymapEditorEvent::ResetAll)),
        )
}

fn build_error_banner(props: KeymapEditorProps, look: Memo<Look>) -> impl IntoElement {
    when(
        move || props.error.with(|e| e.is_some()),
        move || {
            view()
                .test_id("keymap-error")
                .flex_shrink_0()
                .px(move || px(look.with(|l| l.inset3)))
                .py(move || px(look.with(|l| l.inset1)))
                .bg(move || look.with(|l| l.danger_translucent))
                .border_b(move || px(look.with(|l| l.outline_width)))
                .border_color(move || look.with(|l| l.danger_bg))
                .child(
                    text(move || props.error.get().unwrap_or_default())
                        .typography(move || look.with(|l| l.text_small.clone()))
                        .text_color(move || look.with(|l| l.text_danger)),
                )
        },
    )
}

fn build_table_header(look: Memo<Look>) -> impl IntoElement {
    let col_label = move |label: &'static str| {
        text(label)
            .number_of_lines(1u32)
            .typography(move || look.with(|l| l.text_small_bold.clone()))
            .text_color(move || look.with(|l| l.text_muted))
    };

    view()
        .role(AccessibilityRole::Row)
        .flex_row()
        .items_center()
        .flex_shrink_0()
        .h(px(28.0))
        .px(move || px(look.with(|l| l.inset3)))
        .gap(move || px(look.with(|l| l.gap2)))
        .bg(move || look.with(|l| l.surface_panel1))
        .border_b(move || px(look.with(|l| l.outline_width)))
        .border_color(move || look.with(|l| l.border_muted))
        .child(view().flex_1().min_w(px(200.0)).child(col_label("Command")))
        .child(
            view()
                .w(px(180.0))
                .flex_shrink_0()
                .child(col_label("Keystrokes")),
        )
        .child(
            view()
                .w(px(160.0))
                .flex_shrink_0()
                .child(col_label("Context")),
        )
        .child(
            view()
                .w(px(176.0))
                .flex_shrink_0()
                .child(col_label("Source")),
        )
}

fn build_table_body(
    props: KeymapEditorProps,
    cx: Scope<KeymapEditorEvent>,
    look: Memo<Look>,
    user_color: Memo<Hsla>,
) -> impl IntoElement {
    scroll()
        .flex_1()
        .child(when(
            move || props.rows.with(|rows| rows.is_empty()),
            move || {
                view()
                    .test_id("keymap-empty")
                    .items_center()
                    .justify_center()
                    .py(move || px(look.with(|l| l.inset6)))
                    .px(move || px(look.with(|l| l.inset4)))
                    .child(
                        text("No matching keybindings")
                            .typography(move || look.with(|l| l.text_small.clone()))
                            .text_color(move || look.with(|l| l.text_subtle)),
                    )
            },
        ))
        .child(each(
            props.rows,
            |row| row.id.clone(),
            move |row, index| {
                let initial = row.get_untracked();
                let ctx_slug = if initial.context.is_empty() {
                    "global"
                } else {
                    initial.context.as_str()
                };
                let row_test_id = format!("keymap-row-{}-{ctx_slug}", initial.action);
                let edit_test_id = format!("keymap-edit-{}", initial.action);
                let reset_test_id = format!("keymap-reset-{}", initial.action);
                let unbind_test_id = format!("keymap-unbind-{}", initial.action);

                let is_selected = move || {
                    props.selected_row_id.with(|sel| {
                        sel.as_ref()
                            .is_some_and(|id| row.with(|r| r.id.as_str() == id.as_str()))
                    })
                };

                view()
                    .test_id(&row_test_id)
                    .role(AccessibilityRole::Row)
                    .flex_row()
                    .items_center()
                    .min_h(move || px(look.with(|l| l.row_height.max(28.0))))
                    .px(move || px(look.with(|l| l.inset3)))
                    .py(px(3.0))
                    .gap(move || px(look.with(|l| l.gap2)))
                    .bg(move || {
                        if is_selected() {
                            look.with(|l| l.primary_translucent)
                        } else if index.get() % 2 == 1 {
                            look.with(|l| l.alternate_row)
                        } else {
                            Hsla::default()
                        }
                    })
                    .hover(|s| s.bg(move || look.with(|l| l.neutral_hover)))
                    .on_press(move || {
                        cx.emit(KeymapEditorEvent::SelectRow(
                            row.with_untracked(|r| r.id.clone()),
                        ));
                    })
                    // Column 1: Command title + muted action ID
                    .child(
                        view()
                            .flex_1()
                            .min_w(px(200.0))
                            .flex_row()
                            .items_center()
                            .gap(move || px(look.with(|l| l.gap2)))
                            .overflow_hidden()
                            .child(
                                text(move || row.with(|r| r.command_name.clone()))
                                    .number_of_lines(1u32)
                                    .typography(move || look.with(|l| l.text_small.clone()))
                                    .text_color(move || {
                                        if row.with(|r| r.source == KeymapSource::User) {
                                            user_color.get()
                                        } else {
                                            look.with(|l| l.text)
                                        }
                                    }),
                            )
                            .child(
                                text(move || row.with(|r| r.action.clone()))
                                    .number_of_lines(1u32)
                                    .typography(move || look.with(|l| l.text_small_code.clone()))
                                    .text_color(move || look.with(|l| l.text_subtle)),
                            ),
                    )
                    // Column 2: Clickable Keystrokes pill
                    .child(
                        view()
                            .w(px(180.0))
                            .flex_shrink_0()
                            .flex_row()
                            .items_center()
                            .child(
                                nsh_base::button()
                                    .test_id(&edit_test_id)
                                    .accessibility_label(move || {
                                        Some(SharedString::from(row.with(|r| {
                                            if r.is_unbound || r.display_keystrokes.is_empty() {
                                                format!("Bind shortcut for {}", r.command_name)
                                            } else {
                                                format!(
                                                    "Edit shortcut {} for {}",
                                                    r.display_keystrokes, r.command_name
                                                )
                                            }
                                        })))
                                    })
                                    .h(px(22.0))
                                    .px(px(8.0))
                                    .items_center()
                                    .justify_center()
                                    .rounded(move || px(look.with(|l| l.radius_small_inner)))
                                    .border(move || px(look.with(|l| l.outline_width)))
                                    .border_color(move || look.with(|l| l.border_muted))
                                    .bg(move || {
                                        if row.with(|r| {
                                            r.is_unbound || r.display_keystrokes.is_empty()
                                        }) {
                                            Hsla::default()
                                        } else {
                                            look.with(|l| l.neutral_bg)
                                        }
                                    })
                                    .hover(|s| {
                                        s.bg(move || look.with(|l| l.neutral_hover))
                                            .border_color(move || look.with(|l| l.primary_accent))
                                    })
                                    .on_press(move || {
                                        cx.emit(KeymapEditorEvent::StartRecord(
                                            row.with_untracked(|r| r.id.clone()),
                                        ));
                                    })
                                    .child(
                                        text(move || {
                                            row.with(|r| {
                                                if r.is_unbound || r.display_keystrokes.is_empty() {
                                                    SharedString::new_static("Unbound")
                                                } else {
                                                    r.display_keystrokes.clone()
                                                }
                                            })
                                        })
                                        .number_of_lines(1u32)
                                        .typography(move || {
                                            if row.with(|r| {
                                                r.is_unbound || r.display_keystrokes.is_empty()
                                            }) {
                                                look.with(|l| l.text_small.clone())
                                            } else {
                                                look.with(|l| l.text_small_code.clone())
                                            }
                                        })
                                        .text_color(
                                            move || {
                                                if row.with(|r| r.source == KeymapSource::User) {
                                                    user_color.get()
                                                } else if row.with(|r| {
                                                    r.is_unbound || r.display_keystrokes.is_empty()
                                                }) {
                                                    look.with(|l| l.text_subtle)
                                                } else {
                                                    look.with(|l| l.text)
                                                }
                                            },
                                        ),
                                    ),
                            ),
                    )
                    // Column 3: Context label
                    .child(
                        view().w(px(160.0)).flex_shrink_0().overflow_hidden().child(
                            text(move || {
                                row.with(|r| {
                                    if r.context_label.is_empty() {
                                        SharedString::new_static("Global")
                                    } else {
                                        r.context_label.clone()
                                    }
                                })
                            })
                            .number_of_lines(1u32)
                            .typography(move || look.with(|l| l.text_small.clone()))
                            .text_color(move || look.with(|l| l.text_muted)),
                        ),
                    )
                    // Column 4: Source + per-row Reset / Unbind actions
                    .child(
                        view()
                            .w(px(176.0))
                            .flex_shrink_0()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .gap(px(4.0))
                            .child(
                                text(move || {
                                    if row.with(|r| r.source == KeymapSource::User) {
                                        "User"
                                    } else {
                                        "Default"
                                    }
                                })
                                .number_of_lines(1u32)
                                .typography(move || {
                                    if row.with(|r| r.source == KeymapSource::User) {
                                        look.with(|l| l.text_small_bold.clone())
                                    } else {
                                        look.with(|l| l.text_small.clone())
                                    }
                                })
                                .text_color(move || {
                                    if row.with(|r| r.source == KeymapSource::User) {
                                        user_color.get()
                                    } else {
                                        look.with(|l| l.text_subtle)
                                    }
                                }),
                            )
                            .child(
                                view()
                                    .flex_row()
                                    .items_center()
                                    .gap(px(4.0))
                                    .child(when(
                                        move || row.with(|r| r.is_custom),
                                        move || {
                                            nsh_base::button()
                                                .test_id(&reset_test_id)
                                                .disabled(props.saving)
                                                .h(px(20.0))
                                                .px(px(6.0))
                                                .items_center()
                                                .justify_center()
                                                .rounded(move || {
                                                    px(look.with(|l| l.radius_small_inner))
                                                })
                                                .hover(|s| {
                                                    s.bg(move || look.with(|l| l.neutral_hover))
                                                })
                                                .on_press(move || {
                                                    cx.emit(KeymapEditorEvent::ResetRow(
                                                        row.with_untracked(|r| r.id.clone()),
                                                    ));
                                                })
                                                .child(
                                                    text("Reset")
                                                        .number_of_lines(1u32)
                                                        .typography(move || {
                                                            look.with(|l| l.text_small.clone())
                                                        })
                                                        .text_color(move || {
                                                            look.with(|l| l.text_muted)
                                                        }),
                                                )
                                        },
                                    ))
                                    .child(when(
                                        move || row.with(|r| !r.is_unbound),
                                        move || {
                                            nsh_base::button()
                                                .test_id(&unbind_test_id)
                                                .disabled(props.saving)
                                                .h(px(20.0))
                                                .px(px(6.0))
                                                .items_center()
                                                .justify_center()
                                                .rounded(move || {
                                                    px(look.with(|l| l.radius_small_inner))
                                                })
                                                .hover(|s| {
                                                    s.bg(move || {
                                                        look.with(|l| l.danger_translucent)
                                                    })
                                                })
                                                .on_press(move || {
                                                    cx.emit(KeymapEditorEvent::UnbindRow(
                                                        row.with_untracked(|r| r.id.clone()),
                                                    ));
                                                })
                                                .child(
                                                    text("Unbind")
                                                        .number_of_lines(1u32)
                                                        .typography(move || {
                                                            look.with(|l| l.text_small.clone())
                                                        })
                                                        .text_color(move || {
                                                            look.with(|l| l.text_subtle)
                                                        }),
                                                )
                                        },
                                    )),
                            ),
                    )
            },
        ))
}

fn build_footer(props: KeymapEditorProps, look: Memo<Look>) -> impl IntoElement {
    view()
        .flex_row()
        .items_center()
        .justify_between()
        .flex_shrink_0()
        .px(move || px(look.with(|l| l.inset3)))
        .py(move || px(look.with(|l| l.inset1)))
        .bg(move || look.with(|l| l.surface_panel1))
        .border_t(move || px(look.with(|l| l.outline_width)))
        .border_color(move || look.with(|l| l.border_muted))
        .child(
            text(move || {
                let shown = props.rows.with(Vec::len);
                let total = {
                    let t = props.total_count.get() as usize;
                    if t == 0 { shown } else { t }
                };
                let user = props.user_count.get();
                if shown == total {
                    format!("{total} bindings · {user} user")
                } else {
                    format!("Showing {shown} of {total} bindings · {user} user")
                }
            })
            .number_of_lines(1u32)
            .typography(move || look.with(|l| l.text_small.clone()))
            .text_color(move || look.with(|l| l.text_subtle)),
        )
        .child(
            text("Click any shortcut to record a new binding")
                .number_of_lines(1u32)
                .typography(move || look.with(|l| l.text_small.clone()))
                .text_color(move || look.with(|l| l.text_subtle)),
        )
}

fn build_recorder(
    props: KeymapEditorProps,
    cx: Scope<KeymapEditorEvent>,
    look: Memo<Look>,
) -> impl IntoElement {
    when(
        move || props.recording.with(|r| r.is_some()),
        move || {
            let recorder_ref = element_ref();
            let focus_recorder = recorder_ref;
            let clear_focus_recorder = recorder_ref;

            let has_captured = move || {
                props.recording.with(|r| {
                    r.as_ref()
                        .and_then(|rec| rec.captured_sequence.as_ref())
                        .is_some()
                })
            };

            view()
                .absolute()
                .size_full()
                .items_center()
                .justify_center()
                .z_index(10)
                // Scrim backdrop; clicking outside cancels recording.
                .child(
                    view()
                        .absolute()
                        .size_full()
                        .bg(move || look.with(|l| l.scrim))
                        .on_press(move || cx.emit(KeymapEditorEvent::CancelRecord)),
                )
                // Focused recorder dialog card.
                .child(
                    view()
                        .element_ref(&recorder_ref)
                        .test_id("keymap-recorder")
                        .role(AccessibilityRole::Dialog)
                        .label("Record keyboard shortcut")
                        .focusable(true)
                        .auto_focus(true)
                        .capture_keys(KeyCapture::All)
                        .on_key_down(move |e: &KeyEvent| {
                            if !e.repeat {
                                cx.emit(KeymapEditorEvent::RecordKeyDown(
                                    e.key.clone(),
                                    e.code.clone(),
                                    e.modifiers.ctrl,
                                    e.modifiers.alt,
                                    e.modifiers.shift,
                                    e.modifiers.cmd,
                                    e.repeat,
                                ));
                            }
                        })
                        .w(px(440.0))
                        .p(move || px(look.with(|l| l.inset4)))
                        .gap(move || px(look.with(|l| l.gap3)))
                        .rounded(move || px(look.with(|l| l.radius_default)))
                        .bg(move || look.with(|l| l.surface_overlay))
                        .border(move || px(look.with(|l| l.outline_width)))
                        .border_color(move || look.with(|l| l.primary_accent))
                        // Header
                        .child(
                            view()
                                .gap(move || px(look.with(|l| l.gap1)))
                                .child(
                                    text("Record Shortcut")
                                        .typography(move || {
                                            look.with(|l| l.text_default_bold.clone())
                                        })
                                        .text_color(move || look.with(|l| l.text)),
                                )
                                .child(
                                    view()
                                        .flex_row()
                                        .items_center()
                                        .gap(move || px(look.with(|l| l.gap2)))
                                        .child(
                                            text(move || {
                                                props.recording.with(|r| {
                                                    r.as_ref()
                                                        .map(|rec| rec.command_name.clone())
                                                        .unwrap_or_default()
                                                })
                                            })
                                            .typography(move || {
                                                look.with(|l| l.text_small_bold.clone())
                                            })
                                            .text_color(move || look.with(|l| l.text_muted)),
                                        )
                                        .child(
                                            text(move || {
                                                props.recording.with(|r| {
                                                    r.as_ref()
                                                        .map(|rec| {
                                                            if rec.context_label.is_empty() {
                                                                SharedString::new_static("Global")
                                                            } else {
                                                                rec.context_label.clone()
                                                            }
                                                        })
                                                        .unwrap_or_default()
                                                })
                                            })
                                            .typography(move || look.with(|l| l.text_small.clone()))
                                            .text_color(move || look.with(|l| l.text_subtle)),
                                        ),
                                ),
                        )
                        // Live keystroke capture box
                        .child(
                            view()
                                .test_id("keymap-captured")
                                .h(px(48.0))
                                .px(move || px(look.with(|l| l.inset3)))
                                .items_center()
                                .justify_center()
                                .rounded(move || px(look.with(|l| l.radius_small)))
                                .bg(move || look.with(|l| l.surface_app))
                                .border(move || px(look.with(|l| l.outline_width)))
                                .border_color(move || look.with(|l| l.primary_accent))
                                .on_press(move || focus_recorder.focus())
                                .child(
                                    text(move || {
                                        props.recording.with(|r| {
                                            r.as_ref()
                                                .filter(|rec| !rec.captured_display.is_empty())
                                                .map(|rec| rec.captured_display.clone())
                                                .unwrap_or_else(|| {
                                                    SharedString::new_static(
                                                        "Press desired shortcut…",
                                                    )
                                                })
                                        })
                                    })
                                    .typography(move || {
                                        if has_captured() {
                                            look.with(|l| l.text_default_code.clone())
                                        } else {
                                            look.with(|l| l.text_small.clone())
                                        }
                                    })
                                    .text_color(move || {
                                        if has_captured() {
                                            look.with(|l| l.text)
                                        } else {
                                            look.with(|l| l.text_subtle)
                                        }
                                    }),
                                ),
                        )
                        // Inline conflict warning
                        .child(when(
                            move || {
                                props.recording.with(|r| {
                                    r.as_ref()
                                        .and_then(|rec| rec.conflict_message.as_ref())
                                        .is_some()
                                })
                            },
                            move || {
                                view()
                                    .test_id("keymap-conflict")
                                    .px(move || px(look.with(|l| l.inset2)))
                                    .py(move || px(look.with(|l| l.inset2)))
                                    .rounded(move || px(look.with(|l| l.radius_small)))
                                    .bg(move || look.with(|l| l.warning_translucent))
                                    .border(move || px(look.with(|l| l.outline_width)))
                                    .border_color(move || look.with(|l| l.warning_accent))
                                    .child(
                                        text(move || {
                                            props.recording.with(|r| {
                                                r.as_ref()
                                                    .and_then(|rec| rec.conflict_message.clone())
                                                    .unwrap_or_default()
                                            })
                                        })
                                        .typography(move || look.with(|l| l.text_small.clone()))
                                        .text_color(move || look.with(|l| l.text_warning)),
                                    )
                            },
                        ))
                        // Action buttons
                        .child(
                            view()
                                .flex_row()
                                .items_center()
                                .justify_end()
                                .gap(move || px(look.with(|l| l.gap2)))
                                .child(
                                    button()
                                        .test_id("keymap-record-clear")
                                        .text("Clear")
                                        .size(ComponentSize::Small)
                                        .variant(ButtonVariant::Ghost)
                                        .disabled(move || !has_captured() || props.saving.get())
                                        .on_press(move || {
                                            cx.emit(KeymapEditorEvent::ClearRecord);
                                            clear_focus_recorder.focus();
                                        }),
                                )
                                .child(
                                    button()
                                        .test_id("keymap-record-cancel")
                                        .text("Cancel")
                                        .size(ComponentSize::Small)
                                        .variant(ButtonVariant::Ghost)
                                        .on_press(move || cx.emit(KeymapEditorEvent::CancelRecord)),
                                )
                                .child(
                                    button()
                                        .test_id("keymap-record-save")
                                        .text("Save")
                                        .size(ComponentSize::Small)
                                        .variant(ButtonVariant::Primary)
                                        .disabled(move || !has_captured() || props.saving.get())
                                        .on_press(move || cx.emit(KeymapEditorEvent::CommitRecord)),
                                ),
                        ),
                )
        },
    )
}

/// Resolves a theme color role name (e.g. `"intent-alerts-warning-accent"`,
/// `"surface-text-warning"`) or direct theme color token name (e.g.
/// `"theme-colors-palettes-orange-600"`) reactively against the active theme, falling back to
/// `fallback` if empty or unknown.
fn resolve_color_token(name: &str, fallback: Hsla) -> Hsla {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return fallback;
    }
    let tokens = nsh_core::element::tokens();
    if let Some(Token::Color(color)) = tokens.role_by_name(trimmed) {
        return color;
    }
    if let Some(color) = tokens.color(trimmed) {
        return color;
    }
    fallback
}

/// Resolved theme tokens for [`KeymapEditor`], updated reactively whenever the active theme or
/// modifiers change.
#[derive(Debug, Clone, PartialEq)]
struct Look {
    surface_app: Hsla,
    surface_panel1: Hsla,
    surface_panel2: Hsla,
    surface_overlay: Hsla,
    alternate_row: Hsla,
    scrim: Hsla,
    border: Hsla,
    border_muted: Hsla,
    outline_width: f32,
    text: Hsla,
    text_muted: Hsla,
    text_subtle: Hsla,
    text_danger: Hsla,
    text_warning: Hsla,
    neutral_bg: Hsla,
    neutral_hover: Hsla,
    primary_accent: Hsla,
    primary_translucent: Hsla,
    danger_bg: Hsla,
    danger_translucent: Hsla,
    warning_accent: Hsla,
    warning_translucent: Hsla,
    gap1: f32,
    gap2: f32,
    gap3: f32,
    inset1: f32,
    inset2: f32,
    inset3: f32,
    inset4: f32,
    inset6: f32,
    control_height: f32,
    row_height: f32,
    radius_small: f32,
    radius_small_inner: f32,
    radius_default: f32,
    text_small: Typography,
    text_small_bold: Typography,
    text_small_code: Typography,
    text_default_bold: Typography,
    text_default_code: Typography,
}

impl Look {
    fn of() -> Self {
        Self {
            surface_app: value(SURFACE_APP),
            surface_panel1: value(SURFACE_PANEL1),
            surface_panel2: value(SURFACE_PANEL2),
            surface_overlay: value(SURFACE_OVERLAY),
            alternate_row: value(SURFACE_ALTERNATE_ROW_BG),
            scrim: value(SURFACE_SCRIM),
            border: value(SURFACE_BORDER),
            border_muted: value(SURFACE_BORDER_MUTED),
            outline_width: value(BORDER_WIDTH_OUTLINE).max(1.0),
            text: value(SURFACE_TEXT),
            text_muted: value(SURFACE_TEXT_MUTED),
            text_subtle: value(SURFACE_TEXT_SUBTLE),
            text_danger: value(SURFACE_TEXT_DANGER),
            text_warning: value(SURFACE_TEXT_WARNING),
            neutral_bg: value(INTENT_NEUTRAL_BG),
            neutral_hover: value(INTENT_NEUTRAL_BG_HOVER),
            primary_accent: value(INTENT_PRIMARY_ACCENT),
            primary_translucent: value(INTENT_PRIMARY_BG_TRANSLUCENT),
            danger_bg: value(INTENT_DANGER_BG),
            danger_translucent: value(INTENT_DANGER_BG_TRANSLUCENT),
            warning_accent: value(INTENT_ALERTS_WARNING_ACCENT),
            warning_translucent: value(INTENT_ALERTS_WARNING_BG_TRANSLUCENT),
            gap1: value(SPACING_GAP1),
            gap2: value(SPACING_GAP2),
            gap3: value(SPACING_GAP3),
            inset1: value(SPACING_INSET1),
            inset2: value(SPACING_INSET2),
            inset3: value(SPACING_INSET3),
            inset4: value(SPACING_INSET4),
            inset6: value(SPACING_INSET6),
            control_height: value(SPACING_SMALL_LVL5_HEIGHT).max(24.0),
            row_height: value(SPACING_SMALL_LVL6_HEIGHT).max(28.0),
            radius_small: value(RADIUS_SMALL),
            radius_small_inner: value(RADIUS_SMALL_INNER),
            radius_default: value(RADIUS_DEFAULT),
            text_small: value(TEXT_SMALL_REGULAR),
            text_small_bold: value(TEXT_SMALL_TITLE1_BOLD),
            text_small_code: value(TEXT_SMALL_CODE),
            text_default_bold: value(TEXT_DEFAULT_TITLE1_BOLD),
            text_default_code: value(TEXT_DEFAULT_CODE),
        }
    }
}
