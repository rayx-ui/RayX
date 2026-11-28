// ContextMenu's previews: the states a design tool or a gallery shows it in, plain data a tool
// reads without running it (docs/component-meta.md). Every value is a literal.
import { ContextMenu, type ComponentMeta } from "rayx/components";

export default {
  id: "context-menu",
  title: "ContextMenu",
  description:
    "Opens a themed menu at the pointer on secondary click, touch hold, Shift+F10, or an assistive technology context menu action.",
  group: "Components",
  component: ContextMenu,
  sections: [
    {
      title: "Availability",
      description:
        "An active context menu target and a disabled target that ignores secondary clicks and long presses.",
      states: {
        Enabled: {
          props: {
            disabled: false,
            margin: 8,
          },
        },
        Disabled: {
          props: {
            disabled: true,
          },
        },
      },
    },
  ],
} satisfies ComponentMeta<typeof ContextMenu>;
