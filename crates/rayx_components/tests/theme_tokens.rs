//! RayX's theme type: the generated tokens in every mode, their NSH names and kinds, the nsh-base
//! roles they play, and the theme as a registered UI set's token source.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashSet;

use nsh_core::element::{AppTheme, Registry, TokenKind, Tokens};
use rayx_components::theme::{
    Accent, ComponentSize, Density, Modes, Os, ROLE_TOKENS, RayXTheme, RoleColors, RoleSet,
};

fn names() -> HashSet<String> {
    RayXTheme::token_specs()
        .iter()
        .map(|s| s.name.to_string())
        .collect()
}

#[test]
fn the_default_theme_is_the_exports_default_modes() {
    assert_eq!(
        RayXTheme::default(),
        RayXTheme::new(Accent::Cyan, Density::Desktop)
    );
    assert_eq!(
        RayXTheme::default(),
        RayXTheme::of(Modes {
            accent: Accent::Cyan,
            density: Density::Desktop,
            os: Os::Windows,
        })
    );
}

#[test]
fn level_5_follows_the_size_and_the_density() {
    // sizing.md: lvl5 height 24/32/40 on Desktop, 40/48/56 on Touch.
    let expected = [
        (Density::Desktop, [24.0, 32.0, 40.0]),
        (Density::Touch, [40.0, 48.0, 56.0]),
    ];
    for (density, heights) in expected {
        let theme = RayXTheme::new(Accent::Cyan, density);
        for (size, height) in ComponentSize::ALL.into_iter().zip(heights) {
            let level = &theme.platform.density.size.of(size).lvl5;
            assert_eq!(level.height, height, "{density:?} {size:?}");
            assert_eq!(
                level.height,
                level.inner_height + 2.0 * level.inner_inset,
                "{density:?} {size:?}: height = innerHeight + 2 x innerInset"
            );
        }
    }
    let desktop = RayXTheme::default();
    let default = desktop.platform.density.size.of(ComponentSize::Default);
    assert_eq!(default.lvl5.inset, 12.0);
    assert_eq!(default.lvl5.adornment_size, 18.0);
    assert_eq!(default.spacing.content, 6.0);
    assert_eq!(default.borders.radius, 8.0);
    assert_eq!(default.level(5), &default.lvl5);
}

#[test]
fn the_accent_picks_the_primary_role_and_keeps_the_neutral_one() {
    let [cyan, sky, blue] = Accent::ALL.map(|a| RayXTheme::new(a, Density::Desktop));
    let primary = |t: &RayXTheme| t.theme.colors.roles.primary.bg;
    assert_ne!(primary(&cyan), primary(&sky));
    assert_ne!(primary(&sky), primary(&blue));
    assert_eq!(
        cyan.theme.colors.roles.neutral,
        blue.theme.colors.roles.neutral
    );
    // Light and Dark are both in every color (the app's background differs between them).
    let app = cyan.theme.colors.surfaces.app;
    assert_ne!(app.light, app.dark);
    // The role sets are the roles' sources: the accent's set is primary.
    assert_eq!(
        cyan.theme.colors.roles.primary,
        RoleColors::set(RoleSet::Cyan)
    );
    assert_eq!(
        blue.theme.colors.roles.primary,
        RoleColors::set(RoleSet::Blue)
    );
}

#[test]
fn tokens_are_named_by_their_path_behind_their_kind() {
    let names = names();
    for name in [
        "surfaces-app",
        "surfaces-text",
        "roles-primary-bg",
        "roles-primary-bg-hover",
        "roles-neutral-bg-strong",
        "roles-alerts-info-bg",
        "interaction-focus-ring",
        "palettes-red-50",
        "palettes-mist-950",
        "shadow-md",
        "blur-default",
        "spacing-gap1",
        "spacing-insets-inset1",
        "spacing-size-default-lvl5-height",
        "spacing-size-small-content",
        "radius-size-default-borders-radius",
        "radius-borders-radius-full",
        "text-size-default-text-title1-bold-font-size",
        "spacing-size-default-text-title1-bold-line-height",
        "font-weight-size-default-text-title1-bold-font-weight",
        "font-typography-font-family-primary",
        "borders-focus-ring-width",
        "spacing-window-title-bar-height",
    ] {
        assert!(names.contains(name), "no token `{name}`");
    }
    let specs = RayXTheme::token_specs();
    assert_eq!(specs.len(), names.len(), "a name is declared twice");
    let kind = |name: &str| specs.iter().find(|s| s.name == name).unwrap().kind;
    assert_eq!(kind("roles-primary-bg"), TokenKind::Color);
    assert_eq!(kind("shadow-md"), TokenKind::Shadow);
    assert_eq!(
        kind("radius-size-default-borders-radius"),
        TokenKind::Radius
    );
    assert_eq!(
        kind("text-size-default-text-title1-bold-font-size"),
        TokenKind::Text
    );
    assert_eq!(kind("font-typography-font-family-primary"), TokenKind::Font);
    assert!(
        specs.iter().all(|s| !s.doc.is_empty()),
        "every token is documented"
    );
}

#[test]
fn rayx_tokens_play_nsh_bases_roles() {
    let specs = RayXTheme::token_specs();
    for (name, roles) in ROLE_TOKENS {
        let spec = specs
            .iter()
            .find(|s| s.name == *name)
            .unwrap_or_else(|| panic!("`{name}` plays roles but is no token"));
        let played: Vec<String> = spec.roles.iter().map(|r| r.name().to_string()).collect();
        for role in *roles {
            assert!(
                played.iter().any(|p| p == role),
                "`{name}` plays no `{role}`: {played:?}"
            );
        }
    }
    let tokens = RayXTheme::default().to_tokens();
    // nsh-base's theme reads each field through its role.
    let theme = <nsh_base::Theme as AppTheme>::read_tokens(&tokens, "");
    let rayx = RayXTheme::default();
    assert_eq!(theme.primary, rayx.theme.colors.roles.primary.bg);
    assert_eq!(theme.background, rayx.theme.colors.surfaces.app);
    assert_eq!(theme.border, rayx.theme.colors.surfaces.border);
    assert_eq!(theme.ring, rayx.theme.colors.interaction.focus_ring);
}

#[test]
fn every_mode_reads_back_from_its_tokens() {
    for accent in Accent::ALL {
        for density in Density::ALL {
            for os in Os::ALL {
                let theme = RayXTheme::of(Modes {
                    accent,
                    density,
                    os,
                });
                let mut tokens = Tokens::default();
                theme.write_tokens("", &mut tokens);
                assert_eq!(
                    RayXTheme::read_tokens(&tokens, ""),
                    theme,
                    "{accent:?} {density:?} {os:?}"
                );
            }
        }
    }
}

#[test]
fn a_ui_set_registers_the_theme_as_the_apps_token_source() {
    let mut set = Registry::ui_set("test/native");
    rayx_components::register(&mut set);
    // Merging validates the type's names, kinds and roles, as an engine does when it starts.
    let merged = Registry::merged(&[nsh_base::registry(), set.clone()]);
    drop(merged);
    let (tokens, left_out) = set.tokens(None).expect("the set brings tokens").unwrap();
    assert!(left_out.is_empty(), "{left_out:?}");
    let theme = RayXTheme::read_tokens(&tokens, "");
    assert_eq!(theme, RayXTheme::default());
}
