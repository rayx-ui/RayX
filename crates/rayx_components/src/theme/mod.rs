//! RayX's theme: the roles RayX's components read, and RayX's default theme, which maps them.
//!
//! - **Roles.** A role is a name RayX reads (`intent-primary-bg`, `spacing-default-lvl5-height`,
//!   `text-default-title1-bold`): the design system's semantic layer. [`roles`] has one typed
//!   constant per role, generated from `crates/rayx_components/roles.json`, RayX's role list, and
//!   [`roles::all`], their declarations, which [`crate::register`] registers. [`value`] reads one
//!   where it is used and follows every change of it (`.bg(move || value(roles::SURFACE_PANEL1))`);
//!   [`IntentRoles`], [`ComponentSize::roles`], [`SizeRoles::level`] and [`SizeTextRoles`] pick
//!   roles by intent, size and level. TSX reads them with `role()` and role-named classes
//!   (`bg-surface-panel1`; a namespaced utility drops its namespace's prefix, so
//!   `h-default-lvl5-height` reads `spacing-default-lvl5-height`, `rounded-default` reads
//!   `radius-default` and `p-gap4` reads `spacing-gap4`; `text-default-title1-bold` is the
//!   typography role of that name). A role known only by its name at run time is NSH's
//!   `Tokens::role_by_name`.
//! - **Themes.** A theme is any NSH theme document that maps RayX's roles onto its own tokens
//!   (`"roles": { "intent-primary-bg": "brand-500" }`). RayX's default theme, [`DOCUMENT`]
//!   (`theme.json`, generated from the design system's DTCG by the `rayx-components` skill's
//!   `theme_export.py`, never edited by hand), is one: [`crate::register`] registers it. Its
//!   tokens belong to it, and RayX's code never names one.
//! - **Modes** are the document's modifiers: `colorMode` (light, dark), `accent` (cyan, sky,
//!   blue), `density` (desktop, touch) and `os` (windows, macos, gnome, kde), each at its default
//!   (light, cyan, desktop, windows) until the app picks one: NSH's `set_modifier("accent",
//!   "sky")`, TSX `setModifier`, a `ThemeScope`'s `modifiers`, or the theme service. RayX sets
//!   none itself.
//! - **Custom themes** are NSH's: an app's own document (`Root::theme`, `nsh.json`'s `"theme"`,
//!   `set_theme`), a patch over the active one (`nsh.json`'s `themeOverrides`, `update_tokens`
//!   with a `Patch` of token values or of `roles`), and `ThemeScope`.

use nsh_core::element::{NumberRole, Role, RoleValue, Typography};
use nsh_core::prelude::*;

/// RayX's roles: one constant per role of `roles.json`, named as the role in upper snake case
/// (`intent-primary-bg-hover` is [`INTENT_PRIMARY_BG_HOVER`](roles::INTENT_PRIMARY_BG_HOVER)),
/// typed by what it reads ([`Role`] for a color, [`NumberRole`], `FontRole`,
/// `Role<Typography>`, `Role<Arc<[BoxShadow]>>`, `Role<Blur>`), and [`all`](roles::all).
///
/// A role that leaves `roles.json` leaves this module, so code that names it no longer
/// compiles:
///
/// ```compile_fail
/// let _ = rayx_components::theme::roles::INTENT_PRIMARY_BG_ON_MEDIA;
/// ```
///
/// ```
/// use rayx_components::theme::roles;
///
/// assert_eq!(roles::INTENT_PRIMARY_BG.name(), "intent-primary-bg");
/// assert!(roles::all().iter().any(|r| r.name == "text-default-title1-bold" && r.required));
/// ```
pub mod roles {
    #![allow(clippy::all, missing_docs, unused_imports)]
    use std::sync::Arc;

    use nsh_core::element::{
        Blur, Border, CubicBezier, FontRole, NumberRole, Role, RoleSpec, TransitionToken,
        Typography,
    };
    use nsh_core::{BoxShadow, Gradient};

    include!(concat!(env!("OUT_DIR"), "/roles.rs"));
}

/// RayX's default theme document, as [`crate::register`] registers it.
pub const DOCUMENT: &str = include_str!("../../theme.json");

/// The value of `role` where it is read (reactive in a binding: it follows a picked context, a
/// patch and another theme), or its type's default (a transparent color, 0) when the active
/// theme maps it onto nothing.
pub fn value<V: RoleValue + Default>(role: Role<V>) -> V {
    nsh_core::element::role(role).unwrap_or_default()
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

    /// Its name in Figma, TSX and its roles (`spacing-<size>-…`).
    pub const fn name(self) -> &'static str {
        match self {
            ComponentSize::Small => "small",
            ComponentSize::Default => "default",
            ComponentSize::Large => "large",
        }
    }

    /// The roles of this size (`spacing-<size>-…`, `radius-<size>`, `text-<size>-…`).
    pub const fn roles(self) -> &'static SizeRoles {
        match self {
            ComponentSize::Small => &SMALL,
            ComponentSize::Default => &DEFAULT,
            ComponentSize::Large => &LARGE,
        }
    }
}

/// The roles of one level of a size (`spacing-<size>-lvl<n>-…`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelRoles {
    /// The outer height of the components on the level.
    pub height: NumberRole,
    /// The height of the nested element (a switch thumb, a segment, an icon in a row).
    pub inner_height: NumberRole,
    /// The space between the outer edge and the nested element, on every side.
    pub inner_inset: NumberRole,
    /// The space between the left or right edge and the content.
    pub inset: NumberRole,
    /// The size of an adornment: an icon, or a slot holding a badge or a small control.
    pub adornment_size: NumberRole,
}

/// The text styles of a size (`text-<size>-<style>`, Figma's `text/size/<size>/*`), each a
/// whole typography: font size, line height, letter spacing, weight and family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SizeTextRoles {
    pub regular: Role<Typography>,
    pub code: Role<Typography>,
    pub subtitle1_bold: Role<Typography>,
    pub subtitle1_regular: Role<Typography>,
    pub title1_bold: Role<Typography>,
    pub title1_regular: Role<Typography>,
    pub title2_regular: Role<Typography>,
    pub title3_regular: Role<Typography>,
}

/// The roles of one component size: levels 1 to 7, corner radii, spacing and text styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SizeRoles {
    /// Levels 1 to 7 ([`Self::level`]).
    pub levels: [LevelRoles; 7],
    /// The outer corner radius of the size's components (`radius-<size>`).
    pub radius: NumberRole,
    /// The corner radius of elements nested in a component, and of the smallest components
    /// (`radius-<size>-inner`).
    pub radius_inner: NumberRole,
    /// The space between the parts of one element: icon, label, caret, badge
    /// (`spacing-<size>-content`).
    pub content: NumberRole,
    /// The space between the elements of a bar or group (`spacing-<size>-items`).
    pub items: NumberRole,
    /// The size's text styles.
    pub text: SizeTextRoles,
}

impl SizeRoles {
    /// Level `n` (1 to 7; others clamp to that range).
    pub const fn level(&self, n: u8) -> LevelRoles {
        let index = match n {
            0 | 1 => 0,
            2..=7 => n as usize - 1,
            _ => 6,
        };
        self.levels[index]
    }
}

// `SMALL`, `DEFAULT` and `LARGE`, generated by `build.rs` over the constants of [`roles`].
include!(concat!(env!("OUT_DIR"), "/size_groups.rs"));

/// The color roles of one intent (`intent-<intent>-…`): an element's fills and the text and
/// accent on them.
///
/// The groups ([`IntentRoles`]'s constants, [`ComponentSize::roles`]) are generated over the
/// constants of [`roles`], so each field is its role's constant, and a role that leaves
/// `roles.json` fails to compile in them as it does anywhere code names its constant:
///
/// ```
/// use rayx_components::theme::{ComponentSize, IntentRoles, roles};
///
/// assert_eq!(IntentRoles::PRIMARY.bg_hover, roles::INTENT_PRIMARY_BG_HOVER);
/// assert_eq!(ComponentSize::Large.roles().level(5).height, roles::SPACING_LARGE_LVL5_HEIGHT);
/// ```
///
/// ```compile_fail
/// use rayx_components::theme::{IntentRoles, roles};
///
/// // `intent-primary-bg-pressed` is no role of `roles.json`.
/// let _ = IntentRoles { bg_active: roles::INTENT_PRIMARY_BG_PRESSED, ..IntentRoles::PRIMARY };
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntentRoles {
    pub bg: Role,
    pub bg_hover: Role,
    pub bg_active: Role,
    pub bg_selected: Role,
    pub bg_selected_hover: Role,
    pub bg_translucent: Role,
    pub text: Role,
    pub text_muted: Role,
    pub text_subtle: Role,
    pub accent: Role,
}

// `IntentRoles::NEUTRAL` … `IntentRoles::ALERT_SUCCESS`, generated by `build.rs` over the
// constants of [`roles`]: a role renamed or removed in `roles.json` fails to compile here.
include!(concat!(env!("OUT_DIR"), "/intent_groups.rs"));
