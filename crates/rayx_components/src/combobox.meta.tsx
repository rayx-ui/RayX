// Combobox's previews: the states a design tool or a gallery shows it in, plain data a tool
// reads without running it (docs/component-meta.md). Every value is a literal.
import { Combobox, type ComponentMeta } from "rayx/components";

export default {
  id: "combobox",
  title: "Combobox",
  description:
    "Searchable single- or multi-select combobox built on nsh/base Combobox with RayX theme roles.",
  group: "Components",
  component: Combobox,
  sections: [
    {
      title: "Selection Modes",
      description: "Single selection, multi-select with clear button, and grouped options.",
      states: {
        Default: {
          props: {
            placeholder: "Choose size...",
            value: ["256"],
            items: [
              { value: "128", label: "128 px" },
              { value: "192", label: "192 px" },
              { value: "256", label: "256 px" },
              { value: "320", label: "320 px" },
            ],
          },
        },
        "Multiple & Cleanable": {
          props: {
            multiple: true,
            cleanable: true,
            placeholder: "Filter tags...",
            value: ["ui", "icons"],
            items: [
              { value: "ui", label: "UI" },
              { value: "icons", label: "Icons" },
              { value: "fonts", label: "Fonts" },
              { value: "deprecated", label: "Deprecated", disabled: true },
            ],
          },
        },
      },
    },
    {
      title: "Sizes & Disabled",
      description: "Small for dense toolbars, large for touch layouts, and the disabled state.",
      states: {
        Small: {
          props: {
            size: "small",
            value: ["128"],
            items: [
              { value: "128", label: "128 px" },
              { value: "256", label: "256 px" },
              { value: "512", label: "512 px" },
            ],
          },
        },
        Large: {
          props: {
            size: "large",
            placeholder: "Select language...",
            items: [
              { value: "en", label: "English" },
              { value: "hr", label: "Hrvatski" },
            ],
          },
        },
        Disabled: {
          props: {
            disabled: true,
            value: ["256"],
            items: [
              { value: "128", label: "128 px" },
              { value: "256", label: "256 px" },
            ],
          },
        },
      },
    },
  ],
} satisfies ComponentMeta<typeof Combobox>;
