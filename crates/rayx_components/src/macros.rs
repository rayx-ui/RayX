//! [`color!`](crate::color): an adaptive color from hex strings, the value a color token takes
//! (`Tokens::default().with(KEY, color!("#7c3aed", "#a78bfa"))`).

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

/// What `color!` expands to.
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
