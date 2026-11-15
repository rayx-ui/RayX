//! `Themes`: the modes of RayX's theme, for TSX.

use nsh_core::prelude::*;

use crate::theme::{Accent, Density, Modes, Os, RayXTheme};

/// RayX's theme in the modes the app picks (Figma's `Theme.Colors`, `Platform.Density` and
/// `Platform.OS`). Light and Dark follow the appearance: `setColorScheme` of `nsh`.
#[module]
pub struct Themes;

#[module]
impl Themes {
    /// Draws the app with RayX's theme in `accent`, `density` and `os`; one left out is its
    /// default (`cyan`, `desktop`, `windows`). It sets every RayX token, so a custom token the app
    /// changed before (`updateTheme`) takes the mode's value: change it again after.
    ///
    /// # Rust
    ///
    /// `set_theme(RayXTheme::of(Modes { .. }))` from a view, a listener or a task.
    pub fn set(
        accent: Option<Accent>,
        density: Option<Density>,
        os: Option<Os>,
        cx: &mut ModuleCx,
    ) {
        cx.set_theme(RayXTheme::of(Modes {
            accent: accent.unwrap_or_default(),
            density: density.unwrap_or_default(),
            os: os.unwrap_or_default(),
        }));
    }

    /// The accents, in the design system's order.
    pub fn accents() -> Vec<Accent> {
        Accent::ALL.to_vec()
    }

    /// The densities, in the design system's order.
    pub fn densities() -> Vec<Density> {
        Density::ALL.to_vec()
    }

    /// The operating systems whose window chrome the theme has, in the design system's order.
    pub fn operating_systems() -> Vec<Os> {
        Os::ALL.to_vec()
    }
}
