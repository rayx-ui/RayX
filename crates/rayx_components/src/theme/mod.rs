//! RayX's theme: the design system's tokens as one NSH theme type, [`RayXTheme`].
//!
//! Its fields are the tokens' WEB code syntax in Figma, keys in snake case: the variable
//! `theme/colors/roles/primary/bgHover` is `rayx().theme.colors.roles.primary.bg_hover`, and
//! `platform/density/size/default/lvl5/height` is
//! `rayx().platform.density.size.default.lvl5.height`. The structs and their values in every mode
//! are generated from the DTCG export ([`tokens`]); this module adds the theme type around them,
//! the modes, the roles NSH's own parts draw with, and the macros that make custom themes
//! ([`rayx_theme!`](crate::rayx_theme), [`rayx_patch!`](crate::rayx_patch),
//! [`color!`](crate::color)).
//!
//! - **Reading.** [`rayx()`] is the theme the app is drawn with: reading it in a binding
//!   subscribes, so the binding runs again when the theme changes (`.bg(move ||
//!   rayx().theme.colors.surfaces.app)`). Colors are adaptive: Light and Dark switch natively.
//! - **Modes.** The accent ([`Accent`]), the density ([`Density`]) and the OS ([`Os`]) are
//!   values of the theme: [`RayXTheme::new`] / [`RayXTheme::of`] make the theme of some modes,
//!   and `set_theme(RayXTheme::of(..))` switches to it. Light and Dark are the platform's
//!   appearance (`set_color_scheme`, the app's `nsh.json` `"appearance"`).
//! - **Custom themes.** A custom theme is a [`RayXTheme`] value: [`rayx_theme!`](crate::rayx_theme)
//!   makes one from a base and the tokens it changes, [`rayx_patch!`](crate::rayx_patch) the patch
//!   `update_theme::<RayXTheme>(..)` applies, and an app's `nsh.json` theme file sets tokens by
//!   their names (`"roles-primary-bg": "#7c3aed"`).
//! - **Names.** In theme files, TSX classes and `token()`, a token is named by its path after its
//!   collection (`theme.colors`, `theme.effects`, `platform.density`, `platform.os`) in kebab
//!   case, behind its kind's namespace: `roles-primary-bg` (`bg-roles-primary-bg`),
//!   `surfaces-text` (`text-surfaces-text`), `spacing-size-default-lvl5-height`
//!   (`h-size-default-lvl5-height`), `radius-size-default-borders-radius`
//!   (`rounded-size-default-borders-radius`). A group named as its tokens' namespace adds no
//!   prefix: `spacing-gap1` (`gap-gap1`), `shadow-default` (`shadow-default`). A token named as
//!   one of Tailwind's sizes keeps Tailwind's class, so it is named through `var()`: `shadow-md`
//!   is `shadow-(--shadow-md)`, `blur-md` is `blur-(--blur-md)`.
//! - **Roles.** RayX's tokens play nsh-base's roles ([`roles`]), so what NSH draws itself (alerts,
//!   menus, scroll bars, text views, the default font) takes RayX's colors, radius and fonts.

mod roles;
pub mod tokens;

use std::sync::{Arc, LazyLock};

use nsh_core::element::{AppTheme, TokenSpec, Tokens};
use nsh_core::prelude::*;

pub use self::roles::ROLE_TOKENS;
pub use self::tokens::*;

/// RayX's design tokens: `theme` (colors and effects, chosen by the accent) and `platform` (sizes,
/// chosen by the density, and window chrome, chosen by the OS), as Figma's WEB code syntax names
/// them. Read it with [`rayx()`]; register it with `registry.theme::<RayXTheme>()` (done by
/// [`crate::register`]).
///
/// It is an NSH theme type: `set_theme(theme)` draws the app with a whole theme,
/// `update_theme::<RayXTheme>(patch)` changes some tokens, `reset_theme::<RayXTheme>()` puts the
/// app's theme back. [`Default`] is the export's default modes (Cyan, Desktop, Windows).
#[derive(Debug, Clone, PartialEq)]
pub struct RayXTheme {
    /// `theme.*`: `theme.colors` (surfaces, roles, interaction, palettes) and `theme.effects`
    /// (shadows, blurs).
    pub theme: ThemeTokens,
    /// `platform.*`: `platform.density` (levels, sizes, spacing, radii, typography) and
    /// `platform.os` (window chrome).
    pub platform: PlatformTokens,
}

/// The modes a [`RayXTheme`] is made in: Figma's `Theme.Colors`, `Platform.Density` and
/// `Platform.OS` (Light and Dark are in every color).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modes {
    /// The accent (Cyan, Sky, Blue).
    pub accent: Accent,
    /// The density (Desktop, Touch).
    pub density: Density,
    /// The OS whose window chrome is drawn (Windows, macOS, GNOME, KDE).
    pub os: Os,
}

impl RayXTheme {
    /// RayX's theme in `accent` and `density`, with the default OS.
    pub fn new(accent: Accent, density: Density) -> Self {
        Self::of(Modes {
            accent,
            density,
            ..Modes::default()
        })
    }

    /// RayX's theme in `modes`.
    pub fn of(modes: Modes) -> Self {
        Self {
            theme: ThemeTokens::of(modes.accent),
            platform: PlatformTokens::of(modes.density, modes.os),
        }
    }
}

impl Default for RayXTheme {
    fn default() -> Self {
        Self::of(Modes::default())
    }
}

/// Some tokens of a [`RayXTheme`], each `None` for "as it is": what
/// `update_theme::<RayXTheme>(patch)` applies. [`rayx_patch!`](crate::rayx_patch) writes one by
/// paths.
#[derive(Clone, Default)]
pub struct RayXThemePatch {
    /// Tokens of `theme.*`.
    pub theme: Option<ThemeTokensPatch>,
    /// Tokens of `platform.*`.
    pub platform: Option<PlatformTokensPatch>,
}

impl AppTheme for RayXTheme {
    type Patch = RayXThemePatch;

    fn token_specs() -> &'static [TokenSpec] {
        static SPECS: LazyLock<Vec<TokenSpec>> = LazyLock::new(|| {
            ThemeTokens::token_specs()
                .iter()
                .chain(PlatformTokens::token_specs())
                .cloned()
                .map(roles::played)
                .collect()
        });
        &SPECS
    }

    fn write_tokens(&self, prefix: &str, out: &mut Tokens) {
        self.theme.write_tokens(prefix, out);
        self.platform.write_tokens(prefix, out);
    }

    fn read_tokens(tokens: &Tokens, prefix: &str) -> Self {
        Self {
            theme: ThemeTokens::read_tokens(tokens, prefix),
            platform: PlatformTokens::read_tokens(tokens, prefix),
        }
    }

    fn write_patch(patch: &Self::Patch, prefix: &str, out: &mut Tokens) {
        if let Some(theme) = &patch.theme {
            ThemeTokens::write_patch(theme, prefix, out);
        }
        if let Some(platform) = &patch.platform {
            PlatformTokens::write_patch(platform, prefix, out);
        }
    }
}

/// The RayX theme the app is drawn with: `theme_of::<RayXTheme>()`. Reading it subscribes, so a
/// binding that reads it runs again when the theme changes; it is read once per change of the
/// tokens, so every binding can call it.
///
/// # Panics
///
/// Outside the reactive runtime (a component's body, bindings, listeners, tasks and `cx`
/// functions of modules have it).
pub fn rayx() -> Arc<RayXTheme> {
    theme_of::<RayXTheme>()
}

/// A component's size (Figma's `size` property): every component height comes from a level in
/// one of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PropEnum)]
pub enum ComponentSize {
    /// Dense toolbars, tables and inspectors.
    Small,
    /// Most forms and dialogs.
    #[default]
    Default,
    /// Touch layouts and prominent calls to action.
    Large,
}

impl ComponentSize {
    /// Every size, smallest first.
    pub const ALL: [ComponentSize; 3] = [
        ComponentSize::Small,
        ComponentSize::Default,
        ComponentSize::Large,
    ];

    /// Its name in Figma and TSX.
    pub const fn name(self) -> &'static str {
        match self {
            ComponentSize::Small => "small",
            ComponentSize::Default => "default",
            ComponentSize::Large => "large",
        }
    }
}

impl Sizes {
    /// The tokens of `size` (`platform.density.size.<size>`).
    pub fn of(&self, size: ComponentSize) -> &SizeTokens {
        match size {
            ComponentSize::Small => &self.small,
            ComponentSize::Default => &self.default,
            ComponentSize::Large => &self.large,
        }
    }
}

impl SizeTokens {
    /// Level `n` (1 to 7; others clamp to that range).
    pub fn level(&self, n: u8) -> &Level {
        match n {
            0 | 1 => &self.lvl1,
            2 => &self.lvl2,
            3 => &self.lvl3,
            4 => &self.lvl4,
            5 => &self.lvl5,
            6 => &self.lvl6,
            _ => &self.lvl7,
        }
    }
}

impl TextStyle {
    /// Applies these values to `element`, once: its font size, line height, weight and letter
    /// spacing, the parts of a Figma text style (`text/size/<size>/*`, `text/typography/*`). In a
    /// view, [`bind_text_style`] follows the theme instead.
    pub fn apply<E: Styled>(&self, element: E) -> E {
        element
            .text_size(px(self.font_size))
            .line_height(px(self.line_height))
            .font_weight(weight(self.font_weight))
            .letter_spacing(px(self.letter_spacing))
    }
}

impl ParagraphStyle {
    /// The style without its paragraph spacing.
    pub fn text(&self) -> TextStyle {
        TextStyle {
            font_size: self.font_size,
            line_height: self.line_height,
            font_weight: self.font_weight,
            letter_spacing: self.letter_spacing,
        }
    }
}

/// Gives `element` the text style `pick` reads from the theme, bound: its font size, line height
/// and letter spacing follow the theme (a mode, a custom theme) as it changes. The weight is read
/// once, when the view is made: NSH's weights are no bindings.
///
/// ```no_run
/// use nsh_core::prelude::*;
/// use rayx_components::theme::{bind_text_style, rayx};
///
/// let _ = bind_text_style(text("Settings"), |t| t.platform.density.typography.h3.clone())
///     .text_color(move || rayx().theme.colors.surfaces.text);
/// ```
///
/// # Panics
///
/// Outside the reactive runtime: call it where views are made.
pub fn bind_text_style<E: Styled>(
    element: E,
    pick: impl Fn(&RayXTheme) -> TextStyle + 'static,
) -> E {
    let style = computed(move || pick(&rayx()));
    let font_weight = weight(style.with_untracked(|s| s.font_weight));
    element
        .text_size(move || px(style.with(|s| s.font_size)))
        .line_height(move || px(style.with(|s| s.line_height)))
        .letter_spacing(move || px(style.with(|s| s.letter_spacing)))
        .font_weight(font_weight)
}

/// A token's font weight (CSS's 100 to 900) as NSH's.
pub fn weight(value: f32) -> FontWeight {
    FontWeight(value.round().clamp(1.0, 1000.0) as u16)
}
