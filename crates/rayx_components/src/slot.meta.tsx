// Slot's previews: the states a design tool or a gallery shows it in, plain data a tool reads
// without running it (docs/component-meta.md). Every value is a literal. A slot is shown empty: in the
// running app it then shows nothing, and in design mode the box a design tool drops into.
import { Slot, type ComponentMeta } from "rayx/components";

export default {
  id: "slot",
  title: "Slot",
  description:
    "Marks where a component takes content from the screen that uses it. Empty, it draws nothing in the running app; a design tool shows it as the box content is dropped into.",
  group: "Layout",
  component: Slot,
  sections: [
    {
      title: "Named slots",
      description:
        "A slot is known by its `name`, the prop of its component that takes the content: the main `content` and a `footer` are two places in one component.",
      states: {
        Empty: { props: { name: "content" } },
        Footer: { props: { name: "footer" } },
      },
    },
  ],
} satisfies ComponentMeta<typeof Slot>;
