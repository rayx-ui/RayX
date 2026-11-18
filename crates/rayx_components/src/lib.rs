//! RayX's component library for [NSH](https://github.com/dXnSix/nsh) apps: the RayX design
//! system in code, for Rust views and TSX screens alike.
//!
//! - [`theme`]: the roles RayX's components read ([`theme::roles`], generated from RayX's role
//!   list, `roles.json`), read with [`theme::value`], and RayX's default theme
//!   ([`theme::DOCUMENT`]), an NSH theme document generated from the design system that maps
//!   every role onto its tokens. Any other theme that maps the roles draws RayX's components
//!   too; its modes are its modifiers, which the app picks.
//! - The components, each the Figma component of the same name on its nsh-base base component
//!   (behavior, focus, keyboard and accessibility from nsh-base; the look from the roles):
//!   [`Button`](button::Button).
//!
//! An app registers the library in its UI set, which makes the components, RayX's roles (TS's
//! `role()` is typed with them) and RayX's default theme (its tokens in theme files, classes and
//! `token()`, its modifiers in `setModifier`) available to its TSX:
//!
//! ```no_run
//! use nsh_core::prelude::*;
//!
//! #[native_module("my-app/native")]
//! fn native(registry: &mut Registry) {
//!     rayx_components::register(registry);
//! }
//! # fn main() {}
//! ```
//!
//! ```tsx
//! import { Button } from "my-app/native";
//!
//! <Button text="Save changes" variant="primary" onPress={save} />
//! ```
//!
//! Rust views use the constructors: `button().text("Save changes").variant(ButtonVariant::Primary)`.

pub mod button;
pub mod theme;

use nsh_core::element::Registry;

/// What a RayX view uses: the components and their props' types, [`ComponentSize`](theme::ComponentSize),
/// the role groups, the roles and [`value`](theme::value). Names that NSH's prelude also has (`Size`,
/// `Components`) are not in it.
pub mod prelude {
    pub use crate::button::{
        Button, ButtonElement, ButtonEvent, ButtonProps, ButtonVariant, button,
    };
    pub use crate::theme::{
        ComponentSize, IntentRoles, LevelRoles, SizeRoles, SizeTextRoles, roles, value,
    };
}

/// Registers RayX's theme document, RayX's roles and its components into `registry`, an app's
/// UI set: its TSX imports the components by the set's specifier, `role()` and role-named
/// classes read RayX's roles, the default theme maps them (and nsh-base's) unless the app has a
/// theme of its own, and NSH checks a theme's mappings against them.
pub fn register(registry: &mut Registry) -> &mut Registry {
    registry
        .theme_document(theme::DOCUMENT)
        .roles(theme::roles::all())
        .register::<button::Button>()
}
