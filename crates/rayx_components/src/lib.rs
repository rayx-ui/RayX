//! RayX's component library for [NSH](https://github.com/dXnSix/nsh) apps: the RayX design
//! system in code, for Rust views and TSX screens alike.
//!
//! - [`theme`]: the design system's tokens as one NSH theme type, [`RayXTheme`](theme::RayXTheme),
//!   generated from the Figma export; [`rayx()`](theme::rayx) reads it, its modes (accent,
//!   density, OS) make its values, and [`rayx_theme!`], [`rayx_patch!`] and [`color!`] make
//!   custom themes.
//! - The components, each the Figma component of the same name on its nsh-base base component
//!   (behavior, focus, keyboard and accessibility from nsh-base; the look from the theme):
//!   [`Button`](button::Button).
//! - [`Themes`](themes::Themes): the theme's modes for TSX (`Themes.set("sky", "touch")`).
//!
//! An app registers the library in its UI set, which makes the components, the theme type (its
//! tokens in theme files, classes and `token()`) and `Themes` available to its TSX:
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
pub mod themes;

use nsh_core::element::Registry;

#[doc(hidden)]
pub use macros::__private;

/// What a RayX view uses: the components and their props' types, the theme's accessor, modes and
/// macros. Names that NSH's prelude also has (`Size`, `Components`) are not in it.
pub mod prelude {
    pub use crate::button::{
        Button, ButtonElement, ButtonEvent, ButtonProps, ButtonVariant, button,
    };
    pub use crate::theme::{
        Accent, ComponentSize, Density, Modes, Os, RayXTheme, RayXThemePatch, RoleColors, RoleSet,
        bind_text_style, rayx,
    };
    pub use crate::themes::Themes;
    pub use crate::{color, rayx_patch, rayx_theme};
}

/// Registers RayX's components, its theme type and the `Themes` module into `registry`, an app's
/// UI set: its TSX imports them by the set's specifier, its theme file and classes name RayX's
/// tokens, and nsh-base's parts draw with them.
pub fn register(registry: &mut Registry) -> &mut Registry {
    registry
        .theme::<theme::RayXTheme>()
        .register::<button::Button>()
        .module::<themes::Themes>()
}
