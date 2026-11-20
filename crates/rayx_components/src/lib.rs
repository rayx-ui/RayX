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
//! - [`previews`]: `Previews<C>` and `PreviewState<P>`, the types of a component's previews, the
//!   states a design tool or a gallery shows it in, declared in a `*.preview.tsx` file beside the
//!   component. Each of RayX's components ships its own (`button.preview.tsx`).
//!
//! The library is a UI set of its own, [`set`], imported as [`SPECIFIER`] (`rayx/components`):
//! the components, the previews' types, RayX's roles (TS's `role()` is typed with them) and
//! RayX's default theme (its tokens in theme files, classes and `token()`, its modifiers in
//! `setModifier`). An app's set serves it (`#[rayx::native_module]` does), so every app, and the
//! library's own preview files, import it by that one name; nothing names it in `nsh.json`:
//!
//! ```no_run
//! use nsh_core::prelude::*;
//!
//! #[native_module("my-app/native")]
//! fn native(registry: &mut Registry) {
//!     registry.serve(rayx_components::set());
//! }
//! # fn main() {}
//! ```
//!
//! ```tsx
//! import { Button } from "rayx/components";
//!
//! <Button text="Save changes" variant="primary" onPress={save} />
//! ```
//!
//! Rust views use the constructors: `button().text("Save changes").variant(ButtonVariant::Primary)`.

pub mod button;
pub mod previews;
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

/// The specifier RayX's component library is imported by, in apps and in the library's own
/// files: `import { Button } from "rayx/components"`.
pub const SPECIFIER: &str = "rayx/components";

/// RayX's component library as a UI set of its own, [`SPECIFIER`]: RayX's theme document,
/// RayX's roles, the previews' types ([`previews`]) and the components. An app's set serves it
/// (`registry.serve(rayx_components::set())`; `#[rayx::native_module]` does), and registers none
/// of it: its TSX imports the components from `rayx/components`, `role()` and role-named classes
/// read RayX's roles, the default theme maps them (and nsh-base's) unless the app has a theme of
/// its own, and NSH checks a theme's mappings against them.
pub fn set() -> Registry {
    let mut set = Registry::ui_set(SPECIFIER);
    set.theme_document(theme::DOCUMENT)
        .roles(theme::roles::all())
        .ts_alias(previews::PREVIEWS)
        .ts_alias(previews::PREVIEW_STATE)
        .register::<button::Button>();
    set
}
