//! [`ThemeOptions`]: the light/dark modifier and the choice it starts with, and the contexts to
//! pick when the app starts.

use std::collections::BTreeMap;

use rayx_app::services::platform::ColorScheme;
use rayx_app::services::theme::ColorModeChoice;
use serde::{Deserialize, Serialize};

/// The configuration section [`ThemeOptions`] bind from.
pub const THEME_CONFIG_PATH: &str = "RayX:Services:Theme";

/// The theme's light/dark modifier: its name, its light and dark contexts, and the choice the app
/// starts with.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorModeOptions {
    /// The modifier's name (`colorMode`).
    pub modifier: String,
    /// Its light context (`light`).
    pub light: String,
    /// Its dark context (`dark`).
    pub dark: String,
    /// Whether it follows the color scheme shown (`System`, the default) or shows one context.
    pub choice: ColorModeChoice,
}

impl Default for ColorModeOptions {
    fn default() -> Self {
        Self {
            modifier: "colorMode".to_owned(),
            light: "light".to_owned(),
            dark: "dark".to_owned(),
            choice: ColorModeChoice::System,
        }
    }
}

impl ColorModeOptions {
    /// The context the modifier shows for `choice` while the color scheme shown is `scheme`:
    /// `System` maps the scheme to the light or dark context, `Light` and `Dark` give theirs.
    pub fn context_for(&self, choice: ColorModeChoice, scheme: ColorScheme) -> &str {
        match (choice, scheme) {
            (ColorModeChoice::Light, _) | (ColorModeChoice::System, ColorScheme::Light) => {
                &self.light
            }
            (ColorModeChoice::Dark, _) | (ColorModeChoice::System, ColorScheme::Dark) => &self.dark,
        }
    }

    /// The choice that shows `context`: `Light` or `Dark`, `None` for any other context.
    pub fn choice_of(&self, context: &str) -> Option<ColorModeChoice> {
        if context == self.light {
            Some(ColorModeChoice::Light)
        } else if context == self.dark {
            Some(ColorModeChoice::Dark)
        } else {
            None
        }
    }
}

/// RayX's theme service's options, bound from [`THEME_CONFIG_PATH`] and then changed by
/// `add_theme`'s `configure`.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ThemeOptions {
    /// The light/dark modifier and its starting choice.
    pub color_mode: ColorModeOptions,
    /// The contexts to pick when the app starts, by modifier (`{ "accent": "cyan" }`); not the
    /// light/dark modifier's, which [`ColorModeOptions::choice`] sets.
    pub contexts: BTreeMap<String, String>,
}

impl ThemeOptions {
    /// Starts the light/dark modifier at `choice`.
    pub fn color_mode(&mut self, choice: ColorModeChoice) -> &mut Self {
        self.color_mode.choice = choice;
        self
    }

    /// Names the light/dark modifier and its light and dark contexts.
    pub fn color_mode_modifier(
        &mut self,
        modifier: impl Into<String>,
        light: impl Into<String>,
        dark: impl Into<String>,
    ) -> &mut Self {
        self.color_mode.modifier = modifier.into();
        self.color_mode.light = light.into();
        self.color_mode.dark = dark.into();
        self
    }

    /// Picks `context` for `modifier` when the app starts.
    pub fn context(
        &mut self,
        modifier: impl Into<String>,
        context: impl Into<String>,
    ) -> &mut Self {
        self.contexts.insert(modifier.into(), context.into());
        self
    }

    /// Whether the options are valid; the error names the entry that is not: an empty name, or a
    /// `contexts` entry for the light/dark modifier.
    pub fn validate(&self) -> Result<(), String> {
        for (entry, value) in [
            ("colorMode.modifier", &self.color_mode.modifier),
            ("colorMode.light", &self.color_mode.light),
            ("colorMode.dark", &self.color_mode.dark),
        ] {
            if value.trim().is_empty() {
                return Err(format!("the theme option `{entry}` is empty"));
            }
        }
        if self.color_mode.light == self.color_mode.dark {
            return Err(format!(
                "the theme options `colorMode.light` and `colorMode.dark` are both `{}`",
                self.color_mode.light
            ));
        }
        for (modifier, context) in &self.contexts {
            if modifier.trim().is_empty() {
                return Err("the theme option `contexts` has an entry with an empty name".into());
            }
            if context.trim().is_empty() {
                return Err(format!("the theme option `contexts.{modifier}` is empty"));
            }
            if *modifier == self.color_mode.modifier {
                return Err(format!(
                    "the theme option `contexts.{modifier}` picks the light/dark modifier: set \
                     `colorMode.choice` instead"
                ));
            }
        }
        Ok(())
    }

    /// The `contexts` entries the options would apply: the valid ones.
    pub(crate) fn valid_contexts(&self) -> impl Iterator<Item = (&str, &str)> {
        self.contexts
            .iter()
            .filter(|(modifier, context)| {
                !modifier.trim().is_empty()
                    && !context.trim().is_empty()
                    && **modifier != self.color_mode.modifier
            })
            .map(|(modifier, context)| (modifier.as_str(), context.as_str()))
    }
}
