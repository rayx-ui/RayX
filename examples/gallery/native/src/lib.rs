//! The RayX gallery's crate: what its TSX screens import from `rayx-gallery/native`.
//!
//! - RayX's component library (`rayx_components::register`): the components (`Button`), the
//!   theme type (RayX's tokens as the app's classes and `token()`) and `Themes` (the modes).
//! - [`Glyph`](glyph::Glyph): the icons the examples put in the components' icon slots, drawn in
//!   the color of the text around them. RayX's own `Icon` (Figma `primitives/Icon`) takes its
//!   place once it is built.

pub mod glyph;

use nsh_core::prelude::*;

/// The gallery's UI set: RayX's library and the glyphs.
#[native_module("rayx-gallery/native")]
fn native(registry: &mut Registry) {
    rayx_components::register(registry);
    registry.register::<glyph::Glyph>();
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_set_exports_rayx_and_the_glyphs_documented() {
        let set = super::registry();
        assert_eq!(set.undocumented(), Vec::<String>::new());
        let ts = nsh_core::element::declarations(&set, &[]);
        for export in [
            "export declare const Button:",
            "export declare const Themes:",
            "export declare const Glyph:",
        ] {
            assert!(ts.contains(export), "no `{export}`");
        }
    }
}
