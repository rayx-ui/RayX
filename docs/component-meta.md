# Component meta files

A component's previews are the states a design tool or a gallery shows it in. They are declared in a
`*.meta.tsx` file beside the component, as plain data typed with `ComponentMeta<C>`, which RayX's component library
exports from its set, `rayx/components` (`rayx::components::meta::COMPONENT_META`), served by every app's set:

```tsx
// src/components/Card.meta.tsx
import type { ComponentMeta } from "rayx/components";
import { Card } from "./Card";

export default {
  id: "card",
  title: "Card",
  group: "Components",
  component: Card,
  states: {
    Default: { props: { title: "Trip to Zagreb", body: "3 days" } },
    Outlined: { props: { title: "Trip to Zagreb", outlined: true } },
  },
} satisfies ComponentMeta<typeof Card>;
```

- `id`: the component's identity in its meta file, unique in its package (the app's project, or a library's crate). A tool keeps it when
  the file moves or is renamed, so what refers to it does not break.
- `title`, `group`: what a tool lists it as, and under which heading.
- `description`: what the component is for, in Markdown, which a tool shows above its states; left out, it has none.
- `tags`: words in lower case to find it by, any number of them, `tags: ["form", "auth"]`; left out, it has none.
- `component`: the component, imported by name from its module (a TSX file of the app, the app's native module for a
  Rust component of its own, or a library's set: `import { Button } from "rayx/components"`).
- `states`: each named state's `props` (`MetaState<P>`, `Partial` of the component's props); props left out keep
  the component's defaults. A state may add `services`, the services it shows its data with
  ([below](#design-services)).
- `sections`: in place of `states`, the states grouped under titles ([below](#sections)). A file has `states` or
  `sections`, never both.
- `size`: only for a component with no natural size (a screen, which fills whatever it is given): the size a tool shows
  it at, `size: { width: 390, height: 844 }`, in points. A component with a natural size leaves it out and is shown at
  that size.
- `private`: the previews of the component's private components ([below](#private-components)).

Nothing runs to make a file a meta file: a tool reads the literal `id`, `title`, `description`, `group`, `tags`, `size`,
sections, state names and props without running it, and imports the file only to show one of its states. Keep them literals; a value written as
code still reaches the component but the tool cannot show it. A state's `services` is the one exception: a function,
which only a design host runs.

The file is the app's own code, committed beside the component and reviewed like it. It is not part of the app's
bundle unless something imports it.

## Sections

A component with many states groups them in `sections`, which a tool shows top to bottom, each under its title with its
description, in the order they are written:

```tsx
export default {
  id: "button",
  title: "Button",
  group: "Components",
  description: "Starts an action. Use **primary** for the one main action of a screen.",
  component: Button,
  sections: [
    {
      title: "Solid variants",
      description: "One per kind of action.",
      states: {
        Neutral: { props: { text: "Button" } },
        Primary: { props: { text: "Button", variant: "primary" } },
      },
    },
    {
      title: "Sizes",
      description: "Smaller and larger than the default.",
      states: {
        Small: { props: { text: "Button", size: "small" } },
        Large: { props: { text: "Button", size: "large" } },
      },
    },
  ],
} satisfies ComponentMeta<typeof Button>;
```

- A section (`MetaSection<P>`) is a `title`, an optional `description` (Markdown) and its own `states`, typed and
  checked as the top level's are. It has no id: what names a state names the file's `id` and the state's name.
- A state's name is unique within its entry (the file's top level, or one private entry) across all its sections, so
  moving a state to another section changes nothing that names it. Two entries may share a name.
- `title` and `description` are literals: a string, or a template literal with no substitution.
- `states` and `sections` together are refused by the type; a tool refuses the file as well.

## Design services

A view that follows the [app architecture](app-architecture.md) takes its data from a view model and the view model from
services, all resolved in DI. A design tool shows such a view with design implementations instead of the app's, in two
layers.

**`configurePreviews(builder)`**, exported from the app's `App.ts` beside its composition root `configure(builder)`,
registers the design implementations of the app's view models and services, for every preview:

```ts
// App.ts
export function configure(builder: RayXAppBuilder): RayXAppBuilder {
  return builder
    .addPlatform().addSuspension().addAssets().addFonts()
    .addSingletonAs(TripService, () => new DefaultTripService())
    .addScopedAs(TripViewModel, (services) => new DefaultTripViewModel(services.getRequiredService(TripService)));
}

export function configurePreviews(builder: RayXAppBuilder): RayXAppBuilder {
  return builder
    .addSingletonAs(TripService, () => new DesignTripService())        // fake data, or a development service's
    .addSingletonAs(TripStore, () => new InMemoryTripStore());         // never the app's persistent store
}
```

A design tool calls it after `configure`, on the same builder, so its registrations replace the app's (the last
registration of a token wins). Design registrations replace persistent stores with in-memory ones: a preview never
reads or writes the app's data. A `Design<Name>Service` may give fake data or data from a development service. A
view model or a service never asks whether it runs in a design tool: what differs is registered here.

**A state's `services`** then add what that state shows, in the preview instance's own scope (`createScope(configure)`
in [dependency injection](dependency-injection.md#scopes-with-registrations-of-their-own)): for its tokens they win over
the app's and `configurePreviews`' registrations for that instance alone, and its singletons are one per instance.
`scope` has `addSingleton`, `addScoped`, `addTransient` and their `As` forms (`MetaServices`); startup services
belong to the root.

```tsx
// src/screens/Trip.meta.tsx
import type { ComponentMeta } from "rayx/components";
import { Trip } from "./Trip";
import { DesignTripViewModel, TripViewModel } from "./TripViewModel";

export default {
  id: "trip",
  size: { width: 390, height: 844 },
  component: Trip,
  states: {
    Empty: { services: (scope) => scope.addScopedAs(TripViewModel, () => new DesignTripViewModel([])) },
    Long: {
      props: { title: "Two weeks" },
      services: (scope) => scope.addScopedAs(TripViewModel, () => new DesignTripViewModel(LONG_TRIP)),
    },
  },
} satisfies ComponentMeta<typeof Trip>;
```

The order of composition, for each instance: `configure`, `configurePreviews`, the state's `services`, then the
state's `props`.

An app composed in Rust (its root is `RayXAppBuilder::new()….start()`) puts `configure` and `configurePreviews` in an
`App.ts` beside its `App.tsx` all the same, composing through the TS builder and its extensions: RayX's builder
methods are its camelCase twins, and the app's own Rust extensions are exported to TS with `#[app_extension]`
(`addWeather()`), so `configure` registers what the Rust root does and `configurePreviews` replaces it.

## Private components

A component's file often holds components written for its own use: a trip planner's hero and its stat. They are no part
of what the file offers the app; exported only so that the meta file can import them, they are shown on their own
when the component's previews list theirs under `private`:

```tsx
// src/screens/TripPlanner.meta.tsx
import type { ComponentMeta, PrivateMeta } from "rayx/components";
import { Hero, Stat, TripPlanner } from "./TripPlanner";

export default {
  id: "trip-planner",
  title: "Trip planner",
  group: "Screens",
  size: { width: 390, height: 844 },
  component: TripPlanner,
  states: { Default: { props: { traveler: "Ana" } } },
  private: [
    {
      id: "hero",
      component: Hero,
      states: { Wide: { props: { destination: "Lisbon" } } },
    } satisfies PrivateMeta<typeof Hero>,
    {
      id: "stat",
      title: "Stat",
      component: Stat,
      states: { Days: { props: { value: "3", label: "days" } } },
    } satisfies PrivateMeta<typeof Stat>,
  ],
} satisfies ComponentMeta<typeof TripPlanner>;
```

- Each entry is a preview of its own, typed with `PrivateMeta<C>`: an `id`, a `title` if it needs one, a
  `description`, the `component` and its `states` or its `sections`, never both, all literals like the rest of the
  file.
- Its `id` is unique within the list, not the package: a tool names a private component by its component's `id` and its
  own.
- Write each entry `{ … } satisfies PrivateMeta<typeof Hero>`, naming its own component. The list takes the previews
  of any component (one type cannot check each entry against a different component), so the entry's own `satisfies` is
  what checks its props; without it a tool still reads the entry, and a wrong prop in it goes unnoticed.
- It has no `group` (a tool lists it under its component), no `size` and no `private` of its own: private components
  do not nest. It takes `tags`, as its component's previews do.

## A library's meta files

A library of components ships its meta files beside each component in its own crate, importing the component and
`ComponentMeta` from the library's set (its specifier, not an app's), with ids unique within the library's package. RayX's
own components ship theirs with RayX: `crates/rayx_components/src/button.meta.tsx` beside `button.rs`.

```tsx
// crates/rayx_components/src/button.meta.tsx (an excerpt)
import { Button, type ComponentMeta } from "rayx/components";

export default {
  id: "button",
  title: "Button",
  description: "Starts an action. Use **primary** for the main action of a screen, …",
  group: "Components",
  component: Button,
  sections: [
    {
      title: "Solid Variants",
      description: "Filled buttons, one for each kind of action: …",
      states: {
        Neutral: { props: { text: "Button" } },
        Primary: { props: { text: "Button", variant: "primary" } },
      },
    },
    {
      title: "Sizes",
      description: "The two sizes besides the default one, for dense toolbars and for prominent calls to action.",
      states: {
        Small: { props: { text: "Button", size: "small" } },
        Large: { props: { text: "Button", size: "large" } },
      },
    },
  ],
} satisfies ComponentMeta<typeof Button>;
```

The files ship in the crate's source (its package keeps them), and every app serves the library's set, so a tool can
show the library's components in their states beside the app's own, reading the files from the crate as Cargo has it;
an app writes no meta files for RayX's components. A new component of RayX lands with its
meta file: `crates/rayx_components/tests/meta_files.rs` checks that every component the set registers has a
`<snake_name>.meta.tsx` beside its source, written as literals (a `size`, a `description`, `sections` and `private`
previews too, where it has them; an entry with both `states` and `sections`, or naming a state twice, is refused), with a unique `id`, importing its component from `rayx/components`, and that every state renders in an app
serving the set. What TypeScript accepts and refuses of a `size`, of `tags` and of `private` previews is checked by the
crate's `meta_size`, `meta_tags`, `meta_private` and `meta_sections` groups, with the compiler `rayx app check` uses
(`-- --include-ignored`).
