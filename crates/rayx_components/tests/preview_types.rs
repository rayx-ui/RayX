//! The previews' types the library's set declares: `Previews<C>`, a component's previews as a
//! `*.preview.tsx` file declares them, and `PreviewState<P>`, one named state's props. Every app
//! serving the set imports them from `rayx/components`, as the library's own preview files do.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use nsh_core::element::declarations;

#[test]
fn the_library_set_declares_the_previews_types() {
    let set = rayx_components::set();
    assert_eq!(set.specifier(), Some(rayx_components::SPECIFIER));
    let ts = declarations(&set, &[]);
    for declared in [
        "export type Previews<C> = { readonly id: string; readonly title?: string; \
         readonly group?: string; readonly component: C; readonly states: \
         { readonly [name: string]: PreviewState<C extends (props: infer P) => unknown ? P : never> } };",
        "export type PreviewState<P> = { readonly props?: Partial<P> };",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
    assert_eq!(set.undocumented(), Vec::<String>::new());
}
