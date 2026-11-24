// Slot's previews: the states a design tool or a gallery shows it in, plain data a tool reads
// without running it (docs/component-meta.md). Every value is a literal. A slot is shown empty: in the
// running app it then shows nothing, and in design mode the box a design tool drops into.
import { Slot, type ComponentMeta } from "rayx/components";

export default {
  id: "slot",
  title: "Slot",
  group: "Layout",
  component: Slot,
  states: {
    Empty: { props: { name: "content" } },
    Footer: { props: { name: "footer" } },
  },
} satisfies ComponentMeta<typeof Slot>;
