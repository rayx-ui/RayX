//! The nsh-base roles RayX's tokens play, so what NSH draws itself (alerts and action sheets,
//! menus, scroll bars, toasts, text views, the default font) takes RayX's look.
//!
//! A role reads the token it is mapped onto; a token may play several roles, a role is played by
//! one token. Roles not listed here (charts, diffs, syntax, highlight) keep nsh-base's fallbacks.

use nsh_base::role::*;
use nsh_core::element::TokenSpec;

/// Each RayX token that plays nsh-base roles, with the roles' names: what [`played`] maps, for
/// the docs and the tests (every name here is one of [`super::RayXTheme`]'s tokens).
pub const ROLE_TOKENS: &[(&str, &[&str])] = &[
    ("surfaces-app", &["background"]),
    (
        "surfaces-text",
        &[
            "foreground",
            "card-foreground",
            "popover-foreground",
            "accent-foreground",
            "sidebar-foreground",
            "sidebar-accent-foreground",
        ],
    ),
    ("surfaces-panel1", &["card", "sidebar"]),
    ("surfaces-overlay", &["popover"]),
    ("surfaces-text-muted", &["muted-foreground"]),
    ("surfaces-border", &["border", "input", "sidebar-border"]),
    ("surfaces-text-success", &["success"]),
    ("surfaces-text-warning", &["warning"]),
    ("surfaces-text-info", &["info"]),
    ("roles-primary-bg", &["primary", "sidebar-primary"]),
    (
        "roles-primary-text",
        &["primary-foreground", "sidebar-primary-foreground"],
    ),
    ("roles-neutral-bg", &["secondary"]),
    ("roles-neutral-text", &["secondary-foreground"]),
    ("roles-neutral-bg-strong", &["muted"]),
    ("roles-neutral-bg-hover", &["accent", "sidebar-accent"]),
    ("roles-danger-bg", &["destructive"]),
    ("interaction-focus-ring", &["ring", "sidebar-ring"]),
    ("interaction-text-selection-bg", &["selection"]),
    ("radius-size-default-borders-radius", &["radius"]),
    ("font-typography-font-family-primary", &["font-sans"]),
    ("font-typography-font-family-mono", &["font-mono"]),
];

/// `spec` with the nsh-base roles its token plays.
pub(crate) fn played(spec: TokenSpec) -> TokenSpec {
    match spec.name.as_ref() {
        "surfaces-app" => spec.role(BACKGROUND),
        "surfaces-text" => spec
            .role(FOREGROUND)
            .role(CARD_FOREGROUND)
            .role(POPOVER_FOREGROUND)
            .role(ACCENT_FOREGROUND)
            .role(SIDEBAR_FOREGROUND)
            .role(SIDEBAR_ACCENT_FOREGROUND),
        "surfaces-panel1" => spec.role(CARD).role(SIDEBAR),
        "surfaces-overlay" => spec.role(POPOVER),
        "surfaces-text-muted" => spec.role(MUTED_FOREGROUND),
        "surfaces-border" => spec.role(BORDER).role(INPUT).role(SIDEBAR_BORDER),
        "surfaces-text-success" => spec.role(SUCCESS),
        "surfaces-text-warning" => spec.role(WARNING),
        "surfaces-text-info" => spec.role(INFO),
        "roles-primary-bg" => spec.role(PRIMARY).role(SIDEBAR_PRIMARY),
        "roles-primary-text" => spec
            .role(PRIMARY_FOREGROUND)
            .role(SIDEBAR_PRIMARY_FOREGROUND),
        "roles-neutral-bg" => spec.role(SECONDARY),
        "roles-neutral-text" => spec.role(SECONDARY_FOREGROUND),
        "roles-neutral-bg-strong" => spec.role(MUTED),
        "roles-neutral-bg-hover" => spec.role(ACCENT).role(SIDEBAR_ACCENT),
        "roles-danger-bg" => spec.role(DESTRUCTIVE),
        "interaction-focus-ring" => spec.role(RING).role(SIDEBAR_RING),
        "interaction-text-selection-bg" => spec.role(SELECTION),
        "radius-size-default-borders-radius" => spec.role(RADIUS),
        "font-typography-font-family-primary" => spec.role(FONT_SANS),
        "font-typography-font-family-mono" => spec.role(FONT_MONO),
        _ => spec,
    }
}
