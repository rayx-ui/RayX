//! Keystroke conversion and active keybinding resolution between `InputMapState` and NSH's
//! `+`-delimited keystroke syntax (`nsh_core::keymap::Keystroke`).

use std::collections::HashSet;

use nsh_core::keymap::Keystroke;
use rayx_app::services::commands::ActiveKeyBinding;
use rayx_app::services::input_map::{InputBinding, InputMapDocument, InputMapState, InputWhen};

/// Converts an `InputMap` keyboard sequence (either `-`-delimited like `"mod-shift-z"`,
/// `"cmd-k"`, `"ctrl--"` or already `+`-delimited like `"mod+z"`) into canonical NSH `+`-delimited
/// keystroke syntax validated by [`Keystroke::parse_with`].
///
/// Multi-stroke chord sequences (containing whitespace) and sequences with modifiers unsupported
/// by NSH keymaps (such as `fn`) return `None`.
pub fn input_sequence_to_nsh_keystroke(sequence: &str) -> Option<String> {
    let trimmed = sequence.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_whitespace) {
        return None;
    }

    let (modifier_parts, raw_key): (Vec<&str>, &str) = split_stroke(trimmed)?;
    if raw_key.is_empty() {
        return None;
    }

    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;
    let mut cmd = false;
    let mut mod_key = false;

    for part in modifier_parts {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => ctrl = true,
            "alt" | "option" | "opt" => alt = true,
            "shift" => shift = true,
            "cmd" | "command" | "meta" | "super" | "win" => cmd = true,
            "mod" | "cmdorctrl" | "commandorcontrol" | "platform" => mod_key = true,
            _ => return None,
        }
    }

    let key = canonical_nsh_key(raw_key)?;

    let mut parts: Vec<&str> = Vec::with_capacity(6);
    if mod_key {
        parts.push("mod");
    }
    if ctrl {
        parts.push("ctrl");
    }
    if alt {
        parts.push("alt");
    }
    if shift {
        parts.push("shift");
    }
    if cmd {
        parts.push("cmd");
    }
    parts.push(&key);

    let candidate = parts.join("+");
    // Ensure the resulting keystroke is accepted by NSH on both Apple and non-Apple conventions.
    if Keystroke::parse_with(&candidate, true).is_ok()
        && Keystroke::parse_with(&candidate, false).is_ok()
    {
        Some(candidate)
    } else {
        None
    }
}

/// Splits a single stroke written with either `+` or `-` into `(modifiers, key)`.
fn split_stroke(stroke: &str) -> Option<(Vec<&str>, &str)> {
    if stroke == "-" {
        return Some((Vec::new(), "-"));
    }
    if stroke == "+" {
        return Some((Vec::new(), "plus"));
    }

    // If the stroke uses `+` as delimiter (already NSH syntax, e.g. `"mod+shift+k"` or `"mod+-"`),
    // parse it by `+` unless it's `"-+"` (InputMap syntax for modifiers + plus key, e.g. `"mod-+"`).
    if stroke.contains('+') && !stroke.ends_with("-+") {
        if let Some(rest) = stroke.strip_suffix("++") {
            let mods = if rest.is_empty() {
                Vec::new()
            } else {
                rest.split('+').collect()
            };
            return Some((mods, "plus"));
        }
        if let Some((mods, key)) = stroke.rsplit_once('+') {
            let mods = if mods.is_empty() {
                Vec::new()
            } else {
                mods.split('+').collect()
            };
            return Some((mods, key));
        }
    }

    // InputMap `-`-delimited syntax (`"cmd-shift-p"`, `"mod--"`, `"mod-+"`, `"escape"`).
    if let Some(rest) = stroke.strip_suffix("--") {
        let mods = if rest.is_empty() {
            Vec::new()
        } else {
            rest.split('-').collect()
        };
        return Some((mods, "-"));
    }
    if let Some((mods, key)) = stroke.rsplit_once('-') {
        let mods = if mods.is_empty() {
            Vec::new()
        } else {
            mods.split('-').collect()
        };
        return Some((mods, key));
    }

    Some((Vec::new(), stroke))
}

fn canonical_nsh_key(raw_key: &str) -> Option<String> {
    let lower = raw_key.to_ascii_lowercase();
    let mapped = match lower.as_str() {
        "+" | "plus" => "plus".to_owned(),
        "-" | "minus" => "-".to_owned(),
        "esc" | "escape" => "escape".to_owned(),
        "return" | "enter" => "enter".to_owned(),
        "del" | "delete" => "delete".to_owned(),
        "arrowup" | "up" => "up".to_owned(),
        "arrowdown" | "down" => "down".to_owned(),
        "arrowleft" | "left" => "left".to_owned(),
        "arrowright" | "right" => "right".to_owned(),
        _ => lower,
    };
    if mapped.is_empty() {
        None
    } else {
        Some(mapped)
    }
}

/// Layers of an [`InputMapState`], highest precedence first.
fn layers(state: &InputMapState) -> [&InputMapDocument; 3] {
    [&state.user, &state.application, &state.framework]
}

/// Computes the deduplicated active NSH keymap bindings for `active_contexts` (innermost first;
/// global `""` is always tried last) against `input_map` (user > application > framework).
///
/// Within each context, higher layers win over lower layers, and `unbind` removes any lower-layer
/// binding for the same keystroke. Across contexts, inner contexts win over outer contexts (and
/// global `""`), so each NSH keystroke appears at most once in the returned list.
pub fn resolve_active_bindings(
    active_contexts: &[String],
    input_map: &InputMapState,
) -> Vec<ActiveKeyBinding> {
    let mut contexts: Vec<&str> = active_contexts.iter().map(String::as_str).collect();
    if !contexts.contains(&"") {
        contexts.push("");
    }

    let mut seen_keystrokes: HashSet<String> = HashSet::new();
    let mut bindings = Vec::new();

    for context in contexts {
        for document in layers(input_map) {
            for entry in &document.entries {
                if entry.context != context {
                    continue;
                }
                let InputBinding::Keyboard(sequence) = &entry.binding else {
                    continue;
                };
                let Some(keystroke) = input_sequence_to_nsh_keystroke(sequence) else {
                    continue;
                };
                if !seen_keystrokes.insert(keystroke.clone()) {
                    continue;
                }
                if entry.unbind {
                    continue;
                }
                let when = match entry.when {
                    InputWhen::Auto => None,
                    InputWhen::Always => Some("always".to_owned()),
                    InputWhen::NotEditing => Some("notEditing".to_owned()),
                };
                bindings.push(ActiveKeyBinding {
                    keystroke,
                    action: entry.action.clone(),
                    arguments: entry.arguments.clone(),
                    context: entry.context.clone(),
                    when,
                    repeat: entry.repeat,
                    pass_through: entry.pass_through,
                });
            }
        }
    }

    bindings
}

/// Resolves the preferred NSH `+`-syntax shortcut for `action` given `active_contexts` and
/// `input_map`.
///
/// Prefers an active binding for `action` in the current contexts (innermost first, then `""`),
/// falling back to any non-unbound keyboard binding for `action` in `input_map`.
pub fn shortcut_for(
    action: &str,
    active_contexts: &[String],
    input_map: &InputMapState,
) -> Option<String> {
    if let Some(binding) = resolve_active_bindings(active_contexts, input_map)
        .into_iter()
        .find(|binding| binding.action == action)
    {
        return Some(binding.keystroke);
    }

    input_map
        .bindings_for(action)
        .into_iter()
        .find_map(|entry| match entry.binding {
            InputBinding::Keyboard(sequence) => input_sequence_to_nsh_keystroke(&sequence),
            InputBinding::Pointer(_) => None,
        })
}
