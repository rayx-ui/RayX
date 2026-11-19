//! The RayX gallery's crate: what its TSX screens import from `rayx-gallery/native`.
//!
//! - RayX's UI set (`#[rayx::native_module]`): every RayX TS class, the sets of the services the
//!   crate's `rayx` features enable (the four the host requires and the theme service, which
//!   `App.tsx` starts RayX with: `addPlatform`, `addSuspension`, `addAssets`, `addFonts`,
//!   `addTheme`) and RayX's component library: the components (`Button`), RayX's roles (the
//!   names the app's classes read: `bg-surface-panel1`, `h-default-lvl6-height`) and RayX's
//!   default theme document, which maps them, its modes the modifiers the theme service picks.
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
    fn the_set_exports_rayx_s_set_the_services_and_the_glyphs_documented() {
        let set = super::registry();
        assert_eq!(set.undocumented(), Vec::<String>::new());
        let ts = nsh_core::element::declarations(&set, &[]);
        for export in [
            "export declare const Button:",
            "export declare const Glyph:",
            "export declare class RayXAppBuilder",
            // The four services the host requires and the theme service, which `App.tsx` starts
            // RayX with.
            "addPlatform(",
            "addSuspension(",
            "addAssets(",
            "addFonts(",
            "addTheme(",
            "export declare class ThemeService",
        ] {
            assert!(ts.contains(export), "no `{export}`");
        }
    }
}
