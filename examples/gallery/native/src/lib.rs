//! The RayX gallery's crate: what its TSX screens import from `rayx-gallery/native`.
//!
//! - RayX's UI set (`#[rayx::native_module]`): every RayX TS class, the sets of the services the
//!   crate's `rayx` features enable (the four the host requires, which `App.tsx` starts RayX
//!   with: `addPlatform`, `addSuspension`, `addAssets`, `addFonts`) and RayX's component library,
//!   the components (`Button`) and the theme document (RayX's tokens as the app's classes and
//!   `token()`, its modes as the modifiers `setModifier` picks).
//! - [`Glyph`](glyph::Glyph): the icons the examples put in the components' icon slots, drawn in
//!   the color of the text around them. RayX's own `Icon` (Figma `primitives/Icon`) takes its
//!   place once it is built.

pub mod glyph;

use rayx::prelude::*;

/// The gallery's UI set: RayX's, and the glyphs.
#[rayx::native_module("rayx-gallery/native")]
fn native(registry: &mut Registry) {
    registry.register::<glyph::Glyph>();
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_set_exports_rayx_s_set_the_required_services_and_the_glyphs_documented() {
        let set = super::registry();
        assert_eq!(set.undocumented(), Vec::<String>::new());
        let ts = nsh_core::element::declarations(&set, &[]);
        for export in [
            "export declare const Button:",
            "export declare const Glyph:",
            "export declare class RayXAppBuilder",
            // The four services the host requires, which `App.tsx` starts RayX with.
            "addPlatform(",
            "addSuspension(",
            "addAssets(",
            "addFonts(",
        ] {
            assert!(ts.contains(export), "no `{export}`");
        }
    }
}
