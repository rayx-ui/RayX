// Button's previews (Figma `buttons/Button`): the states a design tool or a gallery shows it in,
// plain data a tool reads without running it (docs/component-meta.md). Every value is a literal.
import { Button, type ComponentMeta } from "rayx/components";

export default {
  id: "button",
  title: "Button",
  group: "Components",
  component: Button,
  states: {
    Neutral: { props: { text: "Button" } },
    Primary: { props: { text: "Button", variant: "primary" } },
    Danger: { props: { text: "Button", variant: "danger" } },
    Success: { props: { text: "Button", variant: "success" } },
    Ghost: { props: { text: "Button", variant: "ghost" } },
    Link: { props: { text: "Button", variant: "link" } },
    Outlined: { props: { text: "Button", outlined: true } },
    "Primary outlined": { props: { text: "Button", variant: "primary", outlined: true } },
    Small: { props: { text: "Button", size: "small" } },
    Large: { props: { text: "Button", size: "large" } },
    Disabled: { props: { text: "Button", disabled: true } },
    "Primary disabled": { props: { text: "Button", variant: "primary", disabled: true } },
  },
} satisfies ComponentMeta<typeof Button>;
