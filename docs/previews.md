# Previews

A component's previews are the states a design tool (or a gallery) shows it in. They are declared
in a `*.preview.tsx` file beside the component, as plain data typed with `Previews<C>`, which every
app's set exports (`rayx_app_nsh::ts::PREVIEWS`):

```tsx
// src/components/Card.preview.tsx
import type { Previews } from "my-app/native";
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
} satisfies Previews<typeof Card>;
```

- `id`: the previews' identity, unique in the project. A tool keeps it when the file moves or is
  renamed, so what refers to it does not break.
- `title`, `group`: what a tool lists it as, and under which heading.
- `component`: the component, imported by name from its module (a TSX file of the app, or the
  app's native module for a Rust component).
- `states`: each named state's `props` (`PreviewState<P>`, `Partial` of the component's props);
  props left out keep the component's defaults.

Nothing runs to make a file a preview: a tool reads the literal `id`, `title`, `group`, state names
and props without running it, and imports the file only to show one of its states. Keep them
literals; a value written as code still reaches the component but the tool cannot show it.

The file is the app's own code, committed beside the component and reviewed like it. It is not part
of the app's bundle unless something imports it.
