// AutoLayout's previews (Figma's auto layout): the states a design tool or a gallery shows it in,
// plain data a tool reads without running it (docs/component-meta.md). Every value is a literal; the
// children are what a design tool drops into it.
import { AutoLayout, type ComponentMeta } from "rayx/components";

export default {
  id: "auto-layout",
  title: "AutoLayout",
  description:
    "Lays its children out in a row or a column, like Figma's auto layout: it sets their gap and padding, how they align and wrap, and how wide and tall it is itself.",
  group: "Layout",
  component: AutoLayout,
  sections: [
    {
      title: "Direction",
      description:
        "The way the children run: down a **column** by default, along a **row** when horizontal, and onto further rows when a horizontal layout is set to wrap.",
      states: {
        Vertical: { props: { gap: "gap3", padding: "inset3" } },
        Horizontal: { props: { direction: "horizontal", gap: "gap3", padding: "inset3" } },
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
      },
    },
    {
      title: "Alignment",
      description:
        "Where the children sit in the space the layout is given: centered, or pushed to the two ends of the row with the space between them.",
      states: {
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
      },
    },
    {
      title: "Sizing",
      description: "How wide the layout is itself: as wide as its content by default, or filling the space its parent gives it.",
      states: {
        Fill: { props: { width: "fill", padding: "inset3" } },
      },
    },
  ],
} satisfies ComponentMeta<typeof AutoLayout>;
