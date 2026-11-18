//! The theme service's options for TSX (`addTheme`).

use nsh_core::prelude::*;
use rayx_app::services::theme::ColorModeChoice;
use rayx_app_nsh::ts::push;

use crate::options::ThemeOptions as Options;

/// The theme service's options (`addTheme`), as Rust's configure closure sets `ThemeOptions`:
/// the light/dark choice the app starts with, the light/dark modifier's names, and the contexts
/// to pick at start. Each returns the object, so calls chain.
#[class]
pub struct ThemeOptions {
    steps: Vec<ThemeStep>,
}

/// What one `ThemeOptions` method set.
#[derive(Clone, Debug)]
pub(crate) enum ThemeStep {
    ColorMode(ColorModeChoice),
    ColorModeModifier {
        modifier: String,
        light: String,
        dark: String,
    },
    Context {
        modifier: String,
        context: String,
    },
}

impl ThemeStep {
    pub(crate) fn apply(&self, options: &mut Options) {
        match self {
            Self::ColorMode(choice) => {
                options.color_mode(*choice);
            }
            Self::ColorModeModifier {
                modifier,
                light,
                dark,
            } => {
                options.color_mode_modifier(modifier.clone(), light.clone(), dark.clone());
            }
            Self::Context { modifier, context } => {
                options.context(modifier.clone(), context.clone());
            }
        }
    }
}

/// The choice `text` names.
pub(crate) fn choice(text: &str) -> Result<ColorModeChoice, String> {
    match text {
        "System" => Ok(ColorModeChoice::System),
        "Light" => Ok(ColorModeChoice::Light),
        "Dark" => Ok(ColorModeChoice::Dark),
        other => Err(format!(
            "`{other}` is no color mode: \"System\", \"Light\" or \"Dark\""
        )),
    }
}

impl ThemeOptions {
    pub(crate) fn new() -> Self {
        Self { steps: Vec::new() }
    }

    pub(crate) fn take(&mut self) -> Vec<ThemeStep> {
        std::mem::take(&mut self.steps)
    }
}

#[class]
impl ThemeOptions {
    /// Starts the light/dark modifier at `choice`: following the color scheme shown
    /// (`"System"`, the default) or always `"Light"` or `"Dark"`. Throws for another value.
    #[ts("(choice: ColorModeChoice): ThemeOptions")]
    pub fn color_mode(this: Instance<Self>, choice: String) -> Result<Instance<Self>, String> {
        let choice = self::choice(&choice)?;
        Ok(push(this, |o| o.steps.push(ThemeStep::ColorMode(choice))))
    }

    /// Names the theme's light/dark modifier (`colorMode`) and its light and dark contexts
    /// (`light`, `dark`).
    pub fn color_mode_modifier(
        this: Instance<Self>,
        modifier: String,
        light: String,
        dark: String,
    ) -> Instance<Self> {
        push(this, |o| {
            o.steps.push(ThemeStep::ColorModeModifier {
                modifier,
                light,
                dark,
            })
        })
    }

    /// Picks `context` for the theme's modifier `modifier` when the app starts
    /// (`context("accent", "cyan")`); not the light/dark modifier's (`colorMode` sets it).
    pub fn context(this: Instance<Self>, modifier: String, context: String) -> Instance<Self> {
        push(this, |o| {
            o.steps.push(ThemeStep::Context { modifier, context })
        })
    }
}
