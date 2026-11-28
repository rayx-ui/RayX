// MenuSeparator's previews: the states a design tool or a gallery shows it in, plain data a tool
// reads without running it (docs/component-meta.md). Every value is a literal.
import { MenuSeparator, type ComponentMeta } from "rayx/components";

export default {
  id: "menu-separator",
  title: "MenuSeparator",
  description:
    "Draws a muted horizontal rule between groups of related actions inside a Menu or ContextMenu.",
  group: "Components",
  component: MenuSeparator,
  sections: [
    {
      title: "Divider",
      description: "A 1px horizontal separator colored by `surface-border-muted` with compact vertical spacing.",
      states: {
        Horizontal: {
          props: {},
        },
      },
    },
  ],
} satisfies ComponentMeta<typeof MenuSeparator>;
