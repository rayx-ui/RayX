//! The theme: which context each of the active theme document's modifiers shows (`accent`,
//! `density`, the light/dark modifier), and whether light/dark follows the shown color scheme
//! or the app's choice, published by a [`ThemeService`] implementation as [`ThemeSnapshot`].
//!
//! RayX's implementation is `rayx_services_theme`'s (`add_theme(..)`), which applies the picks
//! to NSH's theme and keeps the light/dark modifier in step with the color scheme the app's
//! `dyn PlatformInfoService` reports; an app may register its own, in Rust or in TS. Reading the
//! snapshot ([`ThemeSnapshot::modifier`], [`ThemeSnapshot::selected`]) is synchronous for any
//! implementation, and so are the same helpers on `dyn ThemeService`.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::ContractFailure;
use crate::published::StateCell;

/// One of the active theme document's modifiers: its contexts, its default and the context it
/// shows.
#[derive(Clone, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeModifier {
    /// Its name (`accent`).
    pub name: String,
    /// Its contexts (`blue`, `cyan`), in the document's order.
    pub contexts: Vec<String>,
    /// The context it shows when nothing else is picked.
    pub default: String,
    /// The context it shows: the one picked, else its default.
    pub selected: String,
}

impl ThemeModifier {
    /// Whether `context` is one of its contexts.
    pub fn has_context(&self, context: &str) -> bool {
        self.contexts.iter().any(|known| known == context)
    }
}

/// What the light/dark modifier shows: the color scheme shown (the app's appearance, else the
/// system's), or one of its two contexts whatever the scheme.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum ColorModeChoice {
    /// Follows the color scheme shown.
    #[default]
    System,
    /// Always light.
    Light,
    /// Always dark.
    Dark,
}

/// The theme now: the active document's modifiers with the context each shows, the light/dark
/// choice, and a revision that counts the snapshots published.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeSnapshot {
    /// The active document's modifiers, in its order (empty outside an engine, or for a document
    /// without modifiers).
    pub modifiers: Vec<ThemeModifier>,
    /// Whether the light/dark modifier follows the color scheme shown or shows one context.
    pub color_mode: ColorModeChoice,
    /// Increases by one with every snapshot published.
    pub revision: u64,
}

impl ThemeSnapshot {
    /// The modifier `name`, when the active document has it.
    pub fn modifier(&self, name: &str) -> Option<&ThemeModifier> {
        self.modifiers.iter().find(|modifier| modifier.name == name)
    }

    /// The context the modifier `name` shows, when the active document has it.
    pub fn selected(&self, name: &str) -> Option<&str> {
        self.modifier(name)
            .map(|modifier| modifier.selected.as_str())
    }
}

/// Why a theme command failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ThemeError {
    /// The active document has no modifier `modifier`.
    UnknownModifier { modifier: String },
    /// The modifier `modifier` has no context `context` (or, for the light/dark modifier, it is
    /// neither of its configured light and dark contexts).
    UnknownContext { modifier: String, context: String },
    /// The call failed (a TS implementation threw, the engine is gone).
    Failed(ContractFailure),
}

impl fmt::Display for ThemeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownModifier { modifier } => {
                write!(f, "the theme has no modifier `{modifier}`")
            }
            Self::UnknownContext { modifier, context } => {
                write!(
                    f,
                    "the theme's modifier `{modifier}` has no context `{context}`"
                )
            }
            Self::Failed(failure) => failure.fmt(f),
        }
    }
}

impl std::error::Error for ThemeError {}

impl From<ContractFailure> for ThemeError {
    fn from(failure: ContractFailure) -> Self {
        Self::Failed(failure)
    }
}

/// The theme service: the context each of the active theme document's modifiers shows and the
/// light/dark choice, published as [`ThemeSnapshot`], and the commands that pick them. A
/// snapshot's `selected` is what the theme shows: a pick is published once it took effect.
#[rayx_app_macros::contract(ts = "ThemeServiceContract")]
pub trait ThemeService {
    /// The modifiers, the light/dark choice and the revision now, and each change of them.
    #[state(ts = "ThemeSnapshot")]
    fn theme(&self) -> &StateCell<ThemeSnapshot>;

    /// Shows `context` for the modifier `modifier` (`accent`, `cyan`); whether that differs from
    /// what the snapshot showed at the call. For the light/dark modifier it takes only its light
    /// and dark contexts, and sets the choice to `Light` or `Dark`. An unknown modifier or
    /// context is refused and changes nothing.
    #[ts("(modifier: string, context: string): boolean")]
    async fn set_context(&self, modifier: String, context: String) -> Result<bool, ThemeError>;

    /// Makes the light/dark modifier follow the color scheme shown (`System`) or show one of its
    /// contexts (`Light`, `Dark`); whether the choice changed.
    #[ts("(choice: ColorModeChoice): boolean")]
    async fn set_color_mode(&self, choice: ColorModeChoice) -> Result<bool, ThemeError>;
}

impl dyn ThemeService {
    /// The modifiers, the light/dark choice and their revision.
    pub fn snapshot(&self) -> ThemeSnapshot {
        (*self.theme().get()).clone()
    }

    /// The modifier `name`, when the active document has it.
    pub fn modifier(&self, name: &str) -> Option<ThemeModifier> {
        self.theme().get().modifier(name).cloned()
    }

    /// The context the modifier `name` shows, when the active document has it.
    pub fn selected(&self, name: &str) -> Option<String> {
        self.theme().get().selected(name).map(str::to_owned)
    }

    /// The light/dark choice.
    pub fn color_mode(&self) -> ColorModeChoice {
        self.theme().get().color_mode
    }
}
