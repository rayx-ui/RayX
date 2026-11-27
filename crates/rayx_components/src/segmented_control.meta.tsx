// SegmentedControl's previews: the states a design tool or a gallery shows it in, plain data a
// tool reads without running it (docs/component-meta.md). Every value is a literal.
import { SegmentedControl, type ComponentMeta } from "rayx/components";

export default {
  id: "segmented-control",
  title: "SegmentedControl",
  description:
    "Switches between mutually exclusive views or filters with a spring-animated selection indicator and Left/Right keyboard navigation.",
  group: "Components",
  component: SegmentedControl,
  sections: [
    {
      title: "Selection & Badges",
      description:
        "Standard segmented bars with plain labels, optional count badges, and an individually disabled option.",
      states: {
        Default: {
          props: {
            value: "all",
            items: [
              { value: "all", label: "All" },
              { value: "icons", label: "Icons" },
              { value: "fonts", label: "Fonts" },
              { value: "themes", label: "Themes" },
            ],
          },
        },
        "With Badges": {
          props: {
            value: "icons",
            items: [
              { value: "all", label: "All", badge: "24" },
              { value: "icons", label: "Icons", badge: "12" },
              { value: "fonts", label: "Fonts", badge: "4" },
              { value: "themes", label: "Themes", badge: "2", disabled: true },
            ],
          },
        },
      },
    },
    {
      title: "Sizes & Disabled",
      description:
        "Small for dense toolbars, large for touch targets, and the fully disabled state.",
      states: {
        Small: {
          props: {
            size: "small",
            value: "grid",
            items: [
              { value: "grid", label: "Grid" },
              { value: "list", label: "List" },
            ],
          },
        },
        Large: {
          props: {
            size: "large",
            value: "light",
            items: [
              { value: "light", label: "Light" },
              { value: "dark", label: "Dark" },
              { value: "system", label: "System" },
            ],
          },
        },
        Disabled: {
          props: {
            disabled: true,
            value: "all",
            items: [
              { value: "all", label: "All" },
              { value: "icons", label: "Icons" },
              { value: "fonts", label: "Fonts" },
            ],
          },
        },
      },
    },
  ],
} satisfies ComponentMeta<typeof SegmentedControl>;
