// Button's previews (Figma `buttons/Button`): the states a design tool or a gallery shows it in,
// plain data a tool reads without running it (docs/component-meta.md). Every value is a literal.
import { Button, type ComponentMeta } from "rayx/components";

export default {
  id: "button",
  title: "Button",
  description:
    "Starts an action. Use **primary** for the main action of a screen, **danger** for one that destroys something and **success** for one that confirms; the other variants step back from them.",
  group: "Components",
  component: Button,
  sections: [
    {
      title: "Solid Variants",
      description:
        "Filled buttons, one for each kind of action: neutral by default, **primary** for the main action, **danger** for a destructive one and **success** for a confirming one.",
      states: {
        Neutral: { props: { text: "Button" } },
        Primary: { props: { text: "Button", variant: "primary" } },
        Danger: { props: { text: "Button", variant: "danger" } },
        Success: { props: { text: "Button", variant: "success" } },
      },
    },
    {
      title: "Outlined Variants",
      description:
        "The same kinds of action drawn as an outline over the background, for actions that sit beside a solid one and should not compete with it.",
      states: {
        Outlined: { props: { text: "Button", outlined: true } },
        "Primary outlined": { props: { text: "Button", variant: "primary", outlined: true } },
        "Danger outlined": { props: { text: "Button", variant: "danger", outlined: true } },
        "Success outlined": { props: { text: "Button", variant: "success", outlined: true } },
      },
    },
    {
      title: "Other Variants",
      description:
        "Buttons with no fill: **ghost** shows a background only when it is pointed at, and **link** reads as text inside a sentence or a toolbar.",
      states: {
        Ghost: { props: { text: "Button", variant: "ghost" } },
        Link: { props: { text: "Button", variant: "link" } },
      },
    },
    {
      title: "Disabled States",
      description:
        "A button that cannot be pressed right now: it keeps its kind of action but draws muted and takes no pointer or key.",
      states: {
        Disabled: { props: { text: "Button", disabled: true } },
        "Primary disabled": { props: { text: "Button", variant: "primary", disabled: true } },
        "Danger disabled": { props: { text: "Button", variant: "danger", disabled: true } },
        "Success disabled": { props: { text: "Button", variant: "success", disabled: true } },
      },
    },
    {
      title: "Sizes",
      description: "The two sizes besides the default one, for dense toolbars and for prominent calls to action.",
      states: {
        Small: { props: { text: "Button", size: "small" } },
        Large: { props: { text: "Button", size: "large" } },
      },
    },
  ],
} satisfies ComponentMeta<typeof Button>;
