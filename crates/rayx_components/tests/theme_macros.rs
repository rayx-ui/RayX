//! The macros of custom themes: `color!` (checked hex colors), `rayx_theme!` (a theme from a base
//! and the tokens it changes) and `rayx_patch!` (the tokens `update_theme` changes).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use nsh_core::color::rgba;
use nsh_core::element::{AdaptiveColor, AppTheme, Tokens};
use rayx_components::theme::{Accent, Density, RayXTheme, RoleColors, RoleSet};
use rayx_components::{color, rayx_patch, rayx_theme};

#[test]
fn color_reads_hex_with_and_without_alpha_for_both_appearances() {
    assert_eq!(
        color!("#7c3aed"),
        AdaptiveColor::new(rgba(0x7c3aedff), rgba(0x7c3aedff))
    );
    assert_eq!(
        color!("#7c3aed", "a78bfa"),
        AdaptiveColor::new(rgba(0x7c3aedff), rgba(0xa78bfaff))
    );
    assert_eq!(
        color!(light: "#0000001A", dark: "#ffffff1a"),
        AdaptiveColor::new(rgba(0x0000001a), rgba(0xffffff1a))
    );
}

#[test]
fn rayx_theme_changes_only_the_tokens_it_names() {
    let base = RayXTheme::new(Accent::Blue, Density::Touch);
    let brand = rayx_theme! {
        extends: base.clone(),
        theme.colors.roles.primary.bg: color!("#7c3aed", "#8b5cf6"),
        theme.colors.roles.success: RoleColors::set(RoleSet::Green),
        platform.density.size.default.borders.radius: 10.0,
    };
    assert_eq!(
        brand.theme.colors.roles.primary.bg,
        color!("#7c3aed", "#8b5cf6")
    );
    assert_eq!(
        brand.theme.colors.roles.success,
        RoleColors::set(RoleSet::Green)
    );
    assert_eq!(brand.platform.density.size.default.borders.radius, 10.0);
    // Everything else is the base's.
    let mut restored = brand.clone();
    restored.theme.colors.roles.primary.bg = base.theme.colors.roles.primary.bg;
    restored.theme.colors.roles.success = base.theme.colors.roles.success.clone();
    restored.platform.density.size.default.borders.radius =
        base.platform.density.size.default.borders.radius;
    assert_eq!(restored, base);
    // Without a base, RayX's default theme.
    let quiet = rayx_theme! { theme.colors.surfaces.app: color!("#fafafa", "#101010") };
    assert_eq!(
        quiet.platform.density.size,
        RayXTheme::default().platform.density.size
    );
}

#[test]
fn rayx_patch_writes_only_the_tokens_it_names() {
    let patch = rayx_patch! {
        theme.colors.roles.primary.bg: color!("#e11d48", "#fb7185"),
        theme.colors.roles.primary.text: color!("#ffffff"),
        platform.density.size.default.lvl5.height: 34.0,
    };
    let mut tokens = Tokens::default();
    RayXTheme::write_patch(&patch, "", &mut tokens);
    assert_eq!(
        tokens.color("roles-primary-bg"),
        Some(color!("#e11d48", "#fb7185"))
    );
    assert_eq!(tokens.color("roles-primary-text"), Some(color!("#ffffff")));
    assert_eq!(
        tokens.number("spacing-size-default-lvl5-height"),
        Some(34.0)
    );
    assert_eq!(tokens.color("roles-primary-bg-hover"), None);
    // An empty patch writes nothing.
    let mut none = Tokens::default();
    RayXTheme::write_patch(&rayx_patch! {}, "", &mut none);
    assert_eq!(none, Tokens::default());
}
