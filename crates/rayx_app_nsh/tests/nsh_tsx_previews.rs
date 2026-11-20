//! The previews' types every app's set declares: `Previews<C>`, a component's previews as a
//! `*.preview.tsx` file declares them, and `PreviewState<P>`, one named state's props.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod headless;

#[test]
fn the_set_declares_the_previews_types() {
    let set = headless::set(|_| {});
    let ts = headless::declarations(&set);
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
