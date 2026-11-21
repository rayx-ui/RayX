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
//! Nothing runs to make a file a preview: a tool reads the literal `id`, `title`, `group`, `tags`,
//! state names and props without running it, and imports it to show one state. An app's components
//! declare theirs the same way, importing the types from `rayx/components` too. Each of RayX's
//! components ships its preview file beside its source (`button.preview.tsx`).
//!
//! A preview may carry `tags`, words in lower case to find it by (`tags: ["form", "auth"]`).
//!
//! Two fields are for the components that need them. A component with no natural size (a screen,
//! which fills what it is given) declares the `size`, in points, a tool shows it at. A component
//! whose file holds private components (written for its own use, and exported only for the
//! preview file to import) lists their previews under `private`, each typed with
//! `PrivatePreviews<C>` of its own component, which checks its props:
//!
//! ```tsx
//! import type { Previews, PrivatePreviews } from "rayx/components";
//! import { Hero, TripPlanner } from "./TripPlanner";
//!
//! export default {
//!   id: "trip-planner",
//!   size: { width: 390, height: 844 },
//!   component: TripPlanner,
//!   states: { Default: { props: { traveler: "Ana" } } },
//!   private: [
//!     {
//!       id: "hero",
//!       component: Hero,
//!       states: { Wide: { props: { destination: "Lisbon" } } },
//!     } satisfies PrivatePreviews<typeof Hero>,
//!   ],
//! } satisfies Previews<typeof TripPlanner>;
//! ```

use nsh_core::element::TsAlias;

/// A component's previews: the states a design tool or a gallery shows it in, declared in a
/// `*.preview.tsx` file beside it as plain data typed with this alias (`export default { … }
/// satisfies Previews<typeof Card>`). Nothing runs to make it one: a tool reads the literal `id`,
/// `title`, `group`, `tags`, `size`, state names and props, and those of its `private` previews
/// ([`PRIVATE_PREVIEWS`]), without running the file, and imports it to show one.
pub const PREVIEWS: TsAlias = TsAlias {
    name: "Previews<C>",
    ts: "{ readonly id: string; readonly title?: string; readonly group?: string; \
         readonly tags?: readonly string[]; \
         readonly size?: { readonly width: number; readonly height: number }; \
         readonly component: C; readonly states: { readonly [name: string]: \
         PreviewState<C extends (props: infer P) => unknown ? P : never> }; \
         readonly private?: readonly PrivatePreviews<(props: any) => unknown>[] }",
    doc: "A component's previews, declared in its `*.preview.tsx` file: a stable `id` (unique in \
          its package, the app's project or a library's crate, and kept when the file moves), a \
          `title` and `group` to list it under, `tags` to find it by (words in lower case, any \
          number), the `component`, and its named `states`. A component with no natural size (a \
          screen, which fills what it is given) adds the \
          `size`, in points, a tool shows it at; one with a natural size leaves it out. \
          `private` lists the previews of its private components, each written `{ … } satisfies \
          PrivatePreviews<typeof Part>`.",
};

/// The previews of a private component: one written in the previewed component's file for its
/// own use, and exported only for the preview file to import. They are the entries of that
/// component's `private` list ([`PREVIEWS`]), each written `{ … } satisfies
/// PrivatePreviews<typeof Part>`: the list itself takes the previews of any component, so it is
/// the entry's own `satisfies` that checks its states' props against its component. An entry has
/// no `group` (it is listed under its component), no `size` and no `private` of its own; it
/// takes `tags`, as its component's previews do.
pub const PRIVATE_PREVIEWS: TsAlias = TsAlias {
    name: "PrivatePreviews<C>",
    ts: "{ readonly id: string; readonly title?: string; readonly tags?: readonly string[]; \
         readonly component: C; readonly states: { readonly [name: string]: \
         PreviewState<C extends (props: infer P) => unknown ? P : never> } }",
    doc: "The previews of a private component: one written in the previewed component's file for \
          its own use, and exported only for the preview file to import. An entry of that \
          component's `private` list, written `{ … } satisfies PrivatePreviews<typeof Part>`, \
          which checks its states' props against its own component: an `id` unique within the \
          list, a `title`, `tags` to find it by, the `component`, and its named `states`.",
};

/// One named state of a component's previews ([`PREVIEWS`], [`PRIVATE_PREVIEWS`]): the props it
/// is shown with.
pub const PREVIEW_STATE: TsAlias = TsAlias {
    name: "PreviewState<P>",
    ts: "{ readonly props?: Partial<P> }",
    doc: "A named state of a component's previews: the props it is shown with (those left out \
          keep the component's defaults).",
};
