//! RayX's component library for [NSH](https://github.com/dXnSix/nsh) apps: the RayX design
//! system in code, for Rust views and TSX screens alike.
//!
//! - [`theme`]: the design system's tokens as an NSH theme document (`theme.json`, generated from
//!   the Figma export), read through typed keys ([`theme::keys`]); its modes are the document's
//!   modifiers ([`theme::modifiers`]), set by the app. [`color!`] writes the adaptive colors a
//!   custom theme's tokens take.
//! - The components, each the Figma component of the same name on its nsh-base base component
//!   (behavior, focus, keyboard and accessibility from nsh-base; the look from the theme):
//!   [`Button`](button::Button).
//!
//! An app registers the library in its UI set, which makes the components and the theme document
//! (its tokens in theme files, classes and `token()`, its modifiers in `setModifier`) available to
//! its TSX:
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
mod macros;
pub mod theme;

use nsh_core::element::Registry;

#[doc(hidden)]
pub use macros::__private;

/// What a RayX view uses: the components and their props' types, the theme's keys, modifiers and
/// lookups, and `color!`. Names that NSH's prelude also has (`Size`, `Components`) are not in it.
pub mod prelude {
    pub use crate::button::{
        Button, ButtonElement, ButtonEvent, ButtonProps, ButtonVariant, button,
    };
    pub use crate::theme::modifiers::{Accent, Density, Os};
    pub use crate::theme::{
        ComponentSize, LevelKeys, RoleKeys, SizeKeys, TextStyleKeys, bind_text_style, keys, value,
    };
    pub use crate::{color, text_style_keys};
}

/// Registers RayX's components and its theme document into `registry`, an app's UI set: its TSX
/// imports the components by the set's specifier, its theme file, classes and `token()` name
/// RayX's tokens, `setModifier` picks its modes, and nsh-base's parts draw with them.
pub fn register(registry: &mut Registry) -> &mut Registry {
    registry
        .theme_document(theme::DOCUMENT)
        .register::<button::Button>()
}
