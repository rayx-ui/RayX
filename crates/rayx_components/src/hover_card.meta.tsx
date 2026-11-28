// HoverCard's previews: the states a design tool or a gallery shows it in, plain data a tool reads
// without running it (docs/component-meta.md). Every value is a literal.
import { HoverCard, type ComponentMeta } from "rayx/components";

export default {
  id: "hover-card",
  title: "HoverCard",
  description:
    "Displays a themed floating preview card anchored to its trigger while hovered, with configurable open/close delays and lazy content mounting.",
  group: "Components",
  component: HoverCard,
  sections: [
    {
      title: "Visibility & Timing",
      description:
        "Default delayed hover behavior, zero-delay immediate previews, and a controlled open card.",
      states: {
        Default: {
          props: {
            openDelay: 600,
            closeDelay: 300,
            placement: "bottom",
            align: "center",
            offset: 6,
          },
        },
        Immediate: {
          props: {
            openDelay: 0,
            closeDelay: 120,
            placement: "bottom",
            align: "start",
            offset: 4,
          },
        },
        "Controlled Open": {
          props: {
            open: true,
            placement: "bottom",
            align: "center",
            offset: 6,
          },
        },
      },
    },
    {
      title: "Availability",
      description:
        "A disabled hover card that keeps its trigger interactive without opening a popup.",
      states: {
        Disabled: {
          props: {
            disabled: true,
          },
        },
      },
    },
  ],
} satisfies ComponentMeta<typeof HoverCard>;
