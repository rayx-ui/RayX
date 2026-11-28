// MenuItem's previews: the states a design tool or a gallery shows it in, plain data a tool reads
// without running it (docs/component-meta.md). Every value is a literal.
import { MenuItem, type ComponentMeta } from "rayx/components";

export default {
  id: "menu-item",
  title: "MenuItem",
  description:
    "One action row inside a Menu or ContextMenu, supporting optional shortcuts, checkable state, danger emphasis, and modifier-aware selection.",
  group: "Components",
  component: MenuItem,
  sections: [
    {
      title: "Variants & Shortcuts",
      description:
        "Standard action rows with optional right-aligned shortcut hints and destructive danger items styled with danger theme roles.",
      states: {
        Default: {
          props: {
            label: "Rename",
          },
        },
        "With shortcut": {
          props: {
            label: "Duplicate",
            shortcut: "Ctrl+D",
          },
        },
        Danger: {
          props: {
            label: "Delete",
            shortcut: "Del",
            variant: "danger",
          },
        },
      },
    },
    {
      title: "Checkable & Disabled",
      description:
        "Checkable menu items showing their current toggle state alongside disabled standard and destructive items.",
      states: {
        Checked: {
          props: {
            label: "Show Grid",
            checked: true,
          },
        },
        Unchecked: {
          props: {
            label: "Show Rulers",
            checked: false,
          },
        },
        Disabled: {
          props: {
            label: "Paste",
            disabled: true,
          },
        },
        "Danger disabled": {
          props: {
            label: "Delete",
            variant: "danger",
            disabled: true,
          },
        },
      },
    },
  ],
} satisfies ComponentMeta<typeof MenuItem>;
