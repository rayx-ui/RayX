// Menu's previews: the states a design tool or a gallery shows it in, plain data a tool reads
// without running it (docs/component-meta.md). Every value is a literal.
import { Menu, type ComponentMeta } from "rayx/components";

export default {
  id: "menu",
  title: "Menu",
  description:
    "Opens a themed popup of actions anchored to its trigger, with keyboard navigation, typeahead, and automatic viewport clamping.",
  group: "Components",
  component: Menu,
  sections: [
    {
      title: "Placement & Alignment",
      description:
        "Positions the popup on any side of its trigger and aligns it to the start or end edge.",
      states: {
        BottomStart: {
          props: {
            placement: "bottom",
            align: "start",
            offset: 4,
          },
        },
        BottomEnd: {
          props: {
            placement: "bottom",
            align: "end",
            offset: 6,
          },
        },
        TopStart: {
          props: {
            placement: "top",
            align: "start",
            offset: 4,
          },
        },
      },
    },
  ],
} satisfies ComponentMeta<typeof Menu>;
