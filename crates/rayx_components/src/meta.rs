//! The types of a component's meta file: the states a design tool or a gallery shows it in,
//! declared in a `*.meta.tsx` file beside the component as plain data typed with
//! `ComponentMeta<C>`, which the library's set, `rayx/components`, exports:
//!
//! ```tsx
//! import { Button, type ComponentMeta } from "rayx/components";
//!
//! export default {
//!   id: "button",
//!   title: "Button",
//!   group: "Components",
//!   component: Button,
//!   states: { Primary: { props: { text: "Save changes", variant: "primary" } } },
//! } satisfies ComponentMeta<typeof Button>;
//! ```
//!
//! Nothing runs to make a file a meta file: a tool reads the literal `id`, `title`, `group`, `tags`,
//! state names and props without running it, and imports it to show one state. An app's components
//! declare theirs the same way, importing the types from `rayx/components` too. Each of RayX's
//! components ships its meta file beside its source (`button.meta.tsx`).
//!
//! A meta file may carry `tags`, words in lower case to find it by (`tags: ["form", "auth"]`).
//!
//! A meta file may describe its component in `description`, Markdown a design tool shows above
//! the states, and group its states in `sections` in place of `states`: each section a `title`,
//! an optional `description` (Markdown too) and its own named `states`, shown in their order. A
//! file has `states` or `sections`, never both, and a state's name is unique within its entry
//! (the file's top level, or one private entry) across its sections, so moving a state from one
//! section to another changes nothing that names it:
//!
//! ```tsx
//! export default {
//!   id: "button",
//!   title: "Button",
//!   description: "Starts an action. Use **primary** for the main action of a screen.",
//!   component: Button,
//!   sections: [
//!     {
//!       title: "Solid variants",
//!       description: "One per kind of action.",
//!       states: {
//!         Neutral: { props: { text: "Button" } },
//!         Primary: { props: { text: "Button", variant: "primary" } },
//!       },
//!     },
//!     { title: "Sizes", states: { Small: { props: { text: "Button", size: "small" } } } },
//!   ],
//! } satisfies ComponentMeta<typeof Button>;
//! ```
//!
//! A `title` or a `description` is a string literal, or a template literal with no substitution.
//!
//! Two fields are for the components that need them. A component with no natural size (a screen,
//! which fills what it is given) declares the `size`, in points, a tool shows it at. A component
//! whose file holds private components (written for its own use, and exported only for the
//! meta file to import) lists their previews under `private`, each typed with
//! `PrivateMeta<C>` of its own component, which checks its props:
//!
//! ```tsx
//! import type { ComponentMeta, PrivateMeta } from "rayx/components";
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
//!     } satisfies PrivateMeta<typeof Hero>,
//!   ],
//! } satisfies ComponentMeta<typeof TripPlanner>;
//! ```
//!
//! A state may add `services`: a function a design host runs to register, in the instance's own
//! scope, the services that state shows its data with (a fake of a view model, say). It gets
//! the scope's registrations ([`META_SERVICES`]), and it is the one part of the file a tool
//! runs rather than reads:
//!
//! ```tsx
//! states: {
//!   Empty: {
//!     props: { title: "Inbox" },
//!     services: (scope) => {
//!       scope.addSingletonAs(InboxViewModel, () => new DesignInboxViewModel([]));
//!     },
//!   },
//! },
//! ```

use nsh_core::element::TsAlias;

/// A component's meta: the states a design tool or a gallery shows it in, declared in a
/// `*.meta.tsx` file beside it as plain data typed with this alias (`export default { … }
/// satisfies ComponentMeta<typeof Card>`). Nothing runs to make it one: a tool reads the literal `id`,
/// `title`, `group`, `tags`, `size`, state names and props, and those of its `private` previews
/// ([`PRIVATE_META`]), without running the file, and imports it to show one.
pub const COMPONENT_META: TsAlias = TsAlias {
    name: "ComponentMeta<C>",
    ts: "{ readonly id: string; readonly title?: string; readonly description?: string; \
         readonly group?: string; readonly tags?: readonly string[]; \
         readonly size?: { readonly width: number; readonly height: number }; \
         readonly component: C; \
         readonly private?: readonly PrivateMeta<(props: any) => unknown>[] } & \
         ({ readonly states: { readonly [name: string]: \
         MetaState<C extends (props: infer P) => unknown ? P : never> }; readonly sections?: never } | \
         { readonly sections: readonly \
         MetaSection<C extends (props: infer P) => unknown ? P : never>[]; readonly states?: never })",
    doc: "A component's meta, declared in its `*.meta.tsx` file: a stable `id` (unique in \
          its package, the app's project or a library's crate, and kept when the file moves), a \
          `title` and `group` to list it under, a `description` (Markdown), `tags` to find it by \
          (words in lower case, any number), the `component`, and either its named `states` or \
          its `sections`, each a `title`, a `description` and named `states` of its own (never \
          both; a state's name is unique within the entry across its sections). A component with no natural size (a \
          screen, which fills what it is given) adds the \
          `size`, in points, a tool shows it at; one with a natural size leaves it out. \
          `private` lists the previews of its private components, each written `{ … } satisfies \
          PrivateMeta<typeof Part>`.",
};

/// The meta of a private component: one written in the previewed component's file for its
/// own use, and exported only for the meta file to import. They are the entries of that
/// component's `private` list ([`COMPONENT_META`]), each written `{ … } satisfies
/// PrivateMeta<typeof Part>`: the list itself takes the previews of any component, so it is
/// the entry's own `satisfies` that checks its states' props against its component. An entry has
/// no `group` (it is listed under its component), no `size` and no `private` of its own; it
/// takes `tags`, a `description` and `states` or `sections`, as its component's meta does.
pub const PRIVATE_META: TsAlias = TsAlias {
    name: "PrivateMeta<C>",
    ts: "{ readonly id: string; readonly title?: string; readonly description?: string; \
         readonly tags?: readonly string[]; readonly component: C } & \
         ({ readonly states: { readonly [name: string]: \
         MetaState<C extends (props: infer P) => unknown ? P : never> }; readonly sections?: never } | \
         { readonly sections: readonly \
         MetaSection<C extends (props: infer P) => unknown ? P : never>[]; readonly states?: never })",
    doc: "The meta of a private component: one written in the previewed component's file for \
          its own use, and exported only for the meta file to import. An entry of that \
          component's `private` list, written `{ … } satisfies PrivateMeta<typeof Part>`, \
          which checks its states' props against its own component: an `id` unique within the \
          list, a `title`, a `description` (Markdown), `tags` to find it by, the `component`, \
          and either its named `states` or its `sections`, never both.",
};

/// A section of a component's meta ([`COMPONENT_META`], [`PRIVATE_META`]): a `title`, an optional
/// `description` (Markdown) and its own named states, which a design tool shows under the title,
/// in the order the sections are written. A section has no id: a state is named once within its
/// entry, across every section, so what names a state names the entry and the state alone.
pub const META_SECTION: TsAlias = TsAlias {
    name: "MetaSection<P>",
    ts: "{ readonly title: string; readonly description?: string; \
         readonly states: { readonly [name: string]: MetaState<P> } }",
    doc: "A section of a component's meta: a `title`, a `description` (Markdown), and its own \
          named `states`, shown under the title in the order the sections are written. A state's \
          name is unique within its entry across every section.",
};

/// One named state of a component's meta ([`COMPONENT_META`], [`PRIVATE_META`]): the props it
/// is shown with, and the services it adds to the instance's own scope ([`META_SERVICES`]).
pub const META_STATE: TsAlias = TsAlias {
    name: "MetaState<P>",
    ts: "{ readonly props?: Partial<P>; readonly services?: (scope: MetaServices) => void }",
    doc: "A named state of a component's meta: the props it is shown with (those left out \
          keep the component's defaults), and `services`, a function a design host runs with the \
          instance's own scope's registrations to add the services the state shows its data \
          with (they replace the app's and the design registrations' for that instance alone).",
};

/// What a preview state's `services` gets: the registrations of the instance's own scope, a
/// scope's `ScopeRegistrations` (`createScope(configure)`'s) as a design host passes it, typed
/// here by their shape (tokens are objects; a factory gets the provider it resolves in), so the
/// library names no type of another crate.
pub const META_SERVICES: TsAlias = TsAlias {
    name: "MetaServices",
    ts: "{ \
         addSingleton(token: object, factory: (services: { getRequiredService(token: object): any; getService(token: object): any }) => unknown): MetaServices; \
         addSingletonAs(token: object, factory: (services: { getRequiredService(token: object): any; getService(token: object): any }) => unknown): MetaServices; \
         addScoped(token: object, factory: (services: { getRequiredService(token: object): any; getService(token: object): any }) => unknown): MetaServices; \
         addScopedAs(token: object, factory: (services: { getRequiredService(token: object): any; getService(token: object): any }) => unknown): MetaServices; \
         addTransient(token: object, factory: (services: { getRequiredService(token: object): any; getService(token: object): any }) => unknown): MetaServices; \
         addTransientAs(token: object, factory: (services: { getRequiredService(token: object): any; getService(token: object): any }) => unknown): MetaServices \
         }",
    doc: "The registrations of a preview instance's own scope, what a state's `services` gets: \
          `addSingleton`, `addScoped`, `addTransient` and their `As` forms, each taking a token \
          (a class, a `ServiceToken`, a contract's class) and a factory called with the provider \
          it resolves in. A singleton is one per instance; what is registered here wins over \
          the app's registrations for that instance alone. Startup services belong to the app's \
          root and are not registered here.",
};
