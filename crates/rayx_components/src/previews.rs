//! The types of a component's previews: the states a design tool (RayX Studio) or a gallery shows
//! it in, declared in a `*.preview.tsx` file beside the component as plain data typed with
//! `Previews<C>`, which the library's set, `rayx/components`, exports:
//!
//! ```tsx
//! import { Button, type Previews } from "rayx/components";
//!
//! export default {
//!   id: "button",
//!   title: "Button",
//!   group: "Components",
//!   component: Button,
//!   states: { Primary: { props: { text: "Save changes", variant: "primary" } } },
//! } satisfies Previews<typeof Button>;
//! ```
//!
//! Nothing runs to make a file a preview: a tool reads the literal `id`, `title`, `group`, state
//! names and props without running it, and imports it to show one state. An app's components
//! declare theirs the same way, importing the types from `rayx/components` too. Each of RayX's
//! components ships its preview file beside its source (`button.preview.tsx`).

use nsh_core::element::TsAlias;

/// A component's previews: the states a design tool or a gallery shows it in, declared in a
/// `*.preview.tsx` file beside it as plain data typed with this alias (`export default { … }
/// satisfies Previews<typeof Card>`). Nothing runs to make it one: a tool reads the literal `id`,
/// `title`, `group`, state names and props without running the file, and imports it to show one.
pub const PREVIEWS: TsAlias = TsAlias {
    name: "Previews<C>",
    ts: "{ readonly id: string; readonly title?: string; readonly group?: string; \
         readonly component: C; readonly states: { readonly [name: string]: \
         PreviewState<C extends (props: infer P) => unknown ? P : never> } }",
    doc: "A component's previews, declared in its `*.preview.tsx` file: a stable `id` (unique in \
          its package, the app's project or a library's crate, and kept when the file moves), a \
          `title` and `group` to list it under, the `component`, and its named `states`.",
};

/// One named state of a component's previews ([`PREVIEWS`]): the props it is shown with.
pub const PREVIEW_STATE: TsAlias = TsAlias {
    name: "PreviewState<P>",
    ts: "{ readonly props?: Partial<P> }",
    doc: "A named state of a component's previews: the props it is shown with (those left out \
          keep the component's defaults).",
};
