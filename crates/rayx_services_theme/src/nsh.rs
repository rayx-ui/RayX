//! The theme service's bridge to NSH on its surface: everything NSH-bound. It applies the
//! service's picks (`set_modifier`) and appearances (`set_color_scheme`) on the engine thread,
//! from a task that takes them, from any thread, through a channel, and it watches NSH's
//! `modifiers()`, the only source of the snapshot's modifiers and their `selected` contexts.
//! Outside an engine there is no bridge.

use std::sync::Arc;

use futures::StreamExt as _;
use futures::channel::mpsc::unbounded;
use futures::channel::oneshot;
use nsh_core::element::{ModifierAxis, modifiers, runtime_entered, set_modifier};
use nsh_core::prelude::{Appearance, Owner, dark, effect, set_color_scheme, spawn, untracked};
use rayx_app::services::theme::{ColorModeChoice, ThemeModifier};

use crate::options::ThemeOptions;
use crate::service::{Action, Bridge, Provisional, RayxTheme};

/// Whether this runs where an engine runs Rust code (else there is no NSH to tell).
pub(crate) fn in_engine() -> bool {
    runtime_entered()
}

/// The modifiers NSH shows, as the contract has them.
fn shown(axes: Vec<ModifierAxis>) -> Vec<ThemeModifier> {
    axes.into_iter()
        .map(|axis| ThemeModifier {
            name: axis.name.into(),
            contexts: axis.contexts.into_iter().map(Into::into).collect(),
            default: axis.default.into(),
            selected: axis.selected.into(),
        })
        .collect()
}

fn appearance(choice: ColorModeChoice) -> Appearance {
    match choice {
        ColorModeChoice::System => Appearance::System,
        ColorModeChoice::Light => Appearance::Light,
        ColorModeChoice::Dark => Appearance::Dark,
    }
}

/// The builder's provisional pick, synchronous where it runs (while the root is built, or
/// before `mount` in TSX), so it reaches the app's first frame: `options`' start contexts and
/// the light/dark context of its choice (with `System`, `dark` exactly when NSH shows dark;
/// with `Light` or `Dark`, that context, and NSH's appearance set to it). A modifier or
/// context the active document lacks is skipped; outside an engine nothing is picked.
pub(crate) fn provisional_pick(options: &ThemeOptions) -> Provisional {
    let mut provisional = Provisional::default();
    if !in_engine() {
        return provisional;
    }
    let document = untracked(modifiers);
    let has = |modifier: &str, context: &str| {
        document.iter().any(|axis| {
            &*axis.name == modifier && axis.contexts.iter().any(|known| &**known == context)
        })
    };
    for (modifier, context) in options.valid_contexts() {
        if has(modifier, context) {
            set_modifier(modifier, context);
        }
    }
    let color = &options.color_mode;
    let choice = color.choice;
    if matches!(choice, ColorModeChoice::Light | ColorModeChoice::Dark) {
        set_color_scheme(appearance(choice));
        provisional.appearance = Some(choice);
    }
    let context = match choice {
        ColorModeChoice::System if untracked(dark) => &color.dark,
        ColorModeChoice::System | ColorModeChoice::Light => &color.light,
        ColorModeChoice::Dark => &color.dark,
    };
    if !color.modifier.trim().is_empty() && has(&color.modifier, context) {
        set_modifier(&color.modifier, context);
        provisional.color_pick = Some(context.clone());
    }
    provisional
}

/// Applies `action` to NSH.
fn apply(action: Action) {
    match action {
        Action::Pick { modifier, context } => set_modifier(&modifier, &context),
        Action::Appearance(choice) => set_color_scheme(appearance(choice)),
        Action::Flushed(done) => {
            let _ = done.send(());
        }
    }
}

/// Gives `theme` what NSH shows now.
fn theme_now(theme: &std::sync::Weak<RayxTheme>) {
    if let Some(theme) = theme.upgrade() {
        theme.observe(shown(untracked(modifiers)));
    }
}

/// Starts `theme`'s bridge on the surface whose engine thread this is (startup's), and resolves
/// once the start picks and color mode were applied; outside an engine it does nothing.
pub(crate) async fn start_bridge(theme: &Arc<RayxTheme>) {
    if !in_engine() {
        return;
    }
    let (actions, mut taken) = unbounded::<Action>();
    theme.connect(Bridge {
        actions: actions.clone(),
    });
    let watched = Arc::downgrade(theme);
    Owner::detached().run(move || {
        // Picks from any thread reach NSH on the engine's.
        drop(spawn(async move {
            while let Some(action) = taken.next().await {
                apply(action);
            }
        }));
        // What NSH shows now: the start picks and color mode go from it.
        theme_now(&watched);
        // NSH's selection, published as it changes: after a pick took effect.
        effect(move || {
            let shown = shown(modifiers());
            if let Some(theme) = watched.upgrade() {
                untracked(move || theme.observe(shown));
            }
        });
    });
    let (done, applied) = oneshot::channel();
    if actions.unbounded_send(Action::Flushed(done)).is_ok() {
        let _ = applied.await;
    }
}
