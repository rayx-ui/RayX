// AutoLayout's previews (Figma's auto layout): the states a design tool or a gallery shows it in,
// plain data a tool reads without running it (docs/previews.md). Every value is a literal; the
// children are what a design tool drops into it.
import { AutoLayout, type Previews } from "rayx/components";

export default {
  id: "auto-layout",
  title: "AutoLayout",
  group: "Layout",
  component: AutoLayout,
  states: {
    Vertical: { props: { gap: "gap3", padding: "inset3" } },
    Horizontal: { props: { direction: "horizontal", gap: "gap3", padding: "inset3" } },
    Centered: {
      props: {
        align: "center",
        padding: "inset4",
        width: "fixed",
        fixedWidth: 240,
        height: "fixed",
        fixedHeight: 120,
      },
    },
    "Space between": {
      props: {
        direction: "horizontal",
        spaceBetween: true,
        align: "left",
        padding: "inset3",
        width: "fixed",
        fixedWidth: 320,
      },
    },
    Wrap: {
      props: {
        direction: "horizontal",
        wrap: true,
        gap: "gap2",
        wrapGap: "gap4",
        width: "fixed",
        fixedWidth: 240,
      },
    },
    Fill: { props: { width: "fill", padding: "inset3" } },
  },
} satisfies Previews<typeof AutoLayout>;
