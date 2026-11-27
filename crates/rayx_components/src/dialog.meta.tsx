// Dialog's previews: the states a design tool or a gallery shows it in, plain data a tool reads
// without running it (docs/component-meta.md). Every value is a literal.
import { Dialog, type ComponentMeta } from "rayx/components";

export default {
  id: "dialog",
  title: "Dialog",
  description:
    "Presents a themed modal card over a dimmed scrim backdrop with optional title, description, body content, and footer actions.",
  group: "Components",
  component: Dialog,
  sections: [
    {
      title: "Dialog & AlertDialog",
      description:
        "Standard informational or form dialogs alongside high-emphasis `alertdialog` confirmations for destructive actions.",
      states: {
        Standard: {
          props: {
            open: true,
            title: "Rename item",
            description: "Choose a clear name to identify this item in the list.",
          },
        },
        "Alert confirmation": {
          props: {
            open: true,
            role: "alertdialog",
            title: "Delete item?",
            description: "This moves the selected item to the trash. You can restore it later.",
          },
        },
      },
    },
    {
      title: "Dismissal & Visibility",
      description:
        "A closed dialog keeping zero layout footprint and a strict alert dialog that disables Escape and backdrop dismissal.",
      states: {
        Closed: {
          props: {
            open: false,
            title: "Hidden dialog",
          },
        },
        "Strict confirmation": {
          props: {
            open: true,
            role: "alertdialog",
            title: "Confirm reset",
            description: "Choose an explicit action below to continue.",
            closeOnEscape: false,
            closeOnBackdropPress: false,
          },
        },
      },
    },
  ],
} satisfies ComponentMeta<typeof Dialog>;
