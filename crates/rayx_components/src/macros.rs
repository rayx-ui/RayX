//! The macros of custom themes: [`color!`](crate::color), [`rayx_theme!`](crate::rayx_theme) and
//! [`rayx_patch!`](crate::rayx_patch).

/// An adaptive color from hex strings: `"#rrggbb"` or `"#rrggbbaa"`, `#` optional, checked when
/// the crate compiles.
///
/// - `color!("#7c3aed")`: the same in Light and Dark.
/// - `color!("#7c3aed", "#a78bfa")`: Light, then Dark.
/// - `color!(light: "#7c3aed", dark: "#a78bfa")`: the same, named.
///
/// ```
/// use rayx_components::color;
///
/// let brand = color!("#7c3aed", "#a78bfa");
/// let scrim = color!("#0000004d");
/// assert_ne!(brand.light, brand.dark);
/// assert_eq!(scrim.light, scrim.dark);
/// ```
///
/// A string that is no hex color fails to compile:
///
/// ```compile_fail
/// let _ = rayx_components::color!("#zz0000");
/// ```
#[macro_export]
macro_rules! color {
    (light: $light:expr, dark: $dark:expr $(,)?) => {
        $crate::color!($light, $dark)
    };
    ($light:expr, $dark:expr $(,)?) => {
        $crate::__private::adaptive(
            const { $crate::__private::hex($light) },
            const { $crate::__private::hex($dark) },
        )
    };
    ($both:expr $(,)?) => {
        $crate::color!($both, $both)
    };
}

/// A custom [`RayXTheme`](crate::theme::RayXTheme): a base theme with the tokens it changes, each
/// a path of the theme's fields (Figma's WEB code syntax in snake case) and a value.
///
/// The base is `extends: <expr>` (any `RayXTheme`), else RayX's default theme. A value is of the
/// field's type: a color an `AdaptiveColor` ([`color!`](crate::color)), a size an `f32` (`10.0`),
/// a font family a `SharedString`, a group a whole struct (`RoleColors::set(RoleSet::Green)`). A
/// path that is no field, or a value of another type, fails to compile.
///
/// ```
/// use rayx_components::theme::{Accent, Density, RayXTheme, RoleColors, RoleSet};
/// use rayx_components::{color, rayx_theme};
///
/// /// The app's brand: violet actions on RayX's Blue theme, rounder controls.
/// fn brand() -> RayXTheme {
///     rayx_theme! {
///         extends: RayXTheme::new(Accent::Blue, Density::Desktop),
///         theme.colors.roles.primary.bg: color!("#7c3aed", "#8b5cf6"),
///         theme.colors.roles.primary.bg_hover: color!("#6d28d9", "#a78bfa"),
///         theme.colors.roles.success: RoleColors::set(RoleSet::Green),
///         platform.density.size.default.borders.radius: 10.0,
///     }
/// }
///
/// let theme = brand();
/// assert_eq!(theme.platform.density.size.default.borders.radius, 10.0);
/// // In a view, a listener or a task: `set_theme(brand());`
/// ```
#[macro_export]
macro_rules! rayx_theme {
    (extends: $base:expr $(, $($path:ident).+ : $value:expr)* $(,)?) => {{
        let mut theme: $crate::theme::RayXTheme = $base;
        $( theme.$($path).+ = $value; )*
        theme
    }};
    ($($($path:ident).+ : $value:expr),* $(,)?) => {
        $crate::rayx_theme!(
            extends: <$crate::theme::RayXTheme as ::core::default::Default>::default()
            $(, $($path).+ : $value)*
        )
    };
}

/// A [`RayXThemePatch`](crate::theme::RayXThemePatch): the tokens
/// `update_theme::<RayXTheme>(patch)` changes, each a path of the theme's fields and a value, the
/// others left as they are. The paths and values are [`rayx_theme!`](crate::rayx_theme)'s.
///
/// ```
/// use nsh_core::element::AppTheme;
/// use rayx_components::theme::RayXTheme;
/// use rayx_components::{color, rayx_patch};
///
/// let patch = rayx_patch! {
///     theme.colors.roles.primary.bg: color!("#e11d48", "#fb7185"),
///     platform.density.size.default.lvl5.height: 34.0,
/// };
/// let mut tokens = nsh_core::element::Tokens::default();
/// RayXTheme::write_patch(&patch, "", &mut tokens);
/// assert!(tokens.color("roles-primary-bg").is_some());
/// // In a view, a listener or a task: `update_theme::<RayXTheme>(patch);`
/// ```
#[macro_export]
macro_rules! rayx_patch {
    ($($first:ident $(. $rest:ident)* : $value:expr),* $(,)?) => {{
        #[allow(unused_mut)]
        let mut patch = $crate::theme::RayXThemePatch::default();
        $( $crate::__patch_set!(patch; $first $(. $rest)* ; $value); )*
        patch
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! __patch_set {
    ($target:expr; $last:ident ; $value:expr) => {
        $target.$last = ::core::option::Option::Some($value);
    };
    ($target:expr; $head:ident $(. $rest:ident)+ ; $value:expr) => {
        $crate::__patch_set!(
            $target.$head.get_or_insert_with(::core::default::Default::default);
            $($rest).+ ;
            $value
        )
    };
}

/// What the macros expand to.
#[doc(hidden)]
pub mod __private {
    use nsh_core::color::rgba;
    use nsh_core::element::AdaptiveColor;

    /// `0xRRGGBBAA` in Light and Dark.
    pub fn adaptive(light: u32, dark: u32) -> AdaptiveColor {
        AdaptiveColor::new(rgba(light), rgba(dark))
    }

    /// `"#rrggbb"` or `"#rrggbbaa"` (`#` optional) as `0xRRGGBBAA`; anything else panics, which
    /// in the `const` block `color!` puts it in fails the build.
    pub const fn hex(text: &str) -> u32 {
        let bytes = text.as_bytes();
        let start = if !bytes.is_empty() && bytes[0] == b'#' {
            1
        } else {
            0
        };
        let digits = bytes.len() - start;
        if digits != 6 && digits != 8 {
            panic!("color!: a color is \"#rrggbb\" or \"#rrggbbaa\"");
        }
        let mut value: u32 = 0;
        let mut i = start;
        while i < bytes.len() {
            let digit = match bytes[i] {
                b'0'..=b'9' => bytes[i] - b'0',
                b'a'..=b'f' => bytes[i] - b'a' + 10,
                b'A'..=b'F' => bytes[i] - b'A' + 10,
                _ => panic!("color!: a color's digits are hex (0-9, a-f)"),
            };
            value = (value << 4) | digit as u32;
            i += 1;
        }
        if digits == 6 {
            (value << 8) | 0xff
        } else {
            value
        }
    }
}
