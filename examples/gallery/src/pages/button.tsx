// Button (Figma `buttons/Button`, page "Button"): a playground of every Figma property, the
// variants and sizes as the component set lays them out, and the page's usage examples.
import { signal, Text, View } from "nsh";
import { Button, Glyph, type ButtonVariant, type ComponentSize } from "rayx-gallery/native";
import { Choice, CountBadge, Example, Note, Page, Section, Stage, type } from "../ui";

const variants = ["neutral", "primary", "danger", "success", "ghost", "link"] as const satisfies readonly ButtonVariant[];
const sizes = ["small", "default", "large"] as const satisfies readonly ComponentSize[];
const labels = ["Button", "Save changes", "Delete project", "Export"] as const;
const onOff = ["off", "on"] as const;

/** Every Figma property of Button, set by the controls beside it, and the TSX it makes. */
function Playground() {
  const text = signal<(typeof labels)[number]>("Button");
  const variant = signal<ButtonVariant>("neutral");
  const size = signal<ComponentSize>("default");
  const outlined = signal(false);
  const disabled = signal(false);
  const showLeftIcon = signal(true);
  const showRightIcon = signal(false);
  const showSlotLeft = signal(false);
  const showSlotRight = signal(false);
  const presses = signal(0);

  // The TSX of the button as set: the Figma properties that differ from their defaults.
  const code = () => {
    const props = [`text="${text()}"`];
    if (variant() !== "neutral") props.push(`variant="${variant()}"`);
    if (outlined()) props.push("outlined");
    if (size() !== "default") props.push(`size="${size()}"`);
    if (disabled()) props.push("disabled");
    props.push('iconLeft={<Glyph name="plus" />}');
    if (!showLeftIcon()) props.push("showLeftIcon={false}");
    props.push('iconRight={<Glyph name="chevronRight" />}');
    if (!showRightIcon()) props.push("showRightIcon={false}");
    if (showSlotLeft()) props.push("showSlotLeft slotLeft={<CountBadge count={2} />}");
    if (showSlotRight()) props.push("showSlotRight slotRight={<CountBadge count={3} />}");
    return `<Button ${props.join(" ")} />`;
  };
  const toggle = (label: string, value: () => boolean, set: (on: boolean) => void) => (
    <Choice aligned label={label} options={onOff} value={value() ? "on" : "off"} onChange={(v) => set(v === "on")} />
  );

  return (
    <Section title="Playground" description="Every property of the Figma component, with its Figma name. Hover, press and Tab to see the runtime states.">
      <View class="flex-row flex-wrap gap-gap5 items-start">
        <View class="gap-gap3 min-w-[320px] flex-1">
          <Stage column>
            <Button
              testID="playground"
              text={text()}
              variant={variant()}
              size={size()}
              outlined={outlined()}
              disabled={disabled()}
              showLeftIcon={showLeftIcon()}
              iconLeft={<Glyph name="plus" />}
              showRightIcon={showRightIcon()}
              iconRight={<Glyph name="chevronRight" />}
              showSlotLeft={showSlotLeft()}
              slotLeft={<CountBadge count={2} />}
              showSlotRight={showSlotRight()}
              slotRight={<CountBadge count={3} />}
              onPress={() => presses.set(presses() + 1)}
            />
            <Note>{`Pressed ${presses()} ${presses() === 1 ? "time" : "times"}`}</Note>
          </Stage>
          <Text class={`text-surfaces-text-muted ${type.code}`}>{code()}</Text>
        </View>
        <View class="gap-gap2 min-w-[320px] flex-1">
          <Choice aligned label="text" options={labels} value={text()} onChange={(v) => text.set(v)} />
          <Choice aligned label="variant" options={variants} value={variant()} onChange={(v) => variant.set(v)} />
          <Choice aligned label="size" options={sizes} value={size()} onChange={(v) => size.set(v)} />
          {toggle("outlined", outlined, (on) => outlined.set(on))}
          {toggle("disabled", disabled, (on) => disabled.set(on))}
          {toggle("showLeftIcon", showLeftIcon, (on) => showLeftIcon.set(on))}
          {toggle("showRightIcon", showRightIcon, (on) => showRightIcon.set(on))}
          {toggle("showSlotLeft", showSlotLeft, (on) => showSlotLeft.set(on))}
          {toggle("showSlotRight", showSlotRight, (on) => showSlotRight.set(on))}
        </View>
      </View>
    </Section>
  );
}

/** The variants as the component set lays them out: rest and disabled, without and with `outlined`. */
function Variants() {
  return (
    <Section title="Variants" description="Each variant at rest and disabled, without and with outlined. link draws no border.">
      <Stage column>
        <Each of={[...variants]}>
          {(variant) => (
            <View class="flex-row items-center gap-gap3">
              <View class="w-[72px]">
                <Note>{variant()}</Note>
              </View>
              <Button text="Button" variant={variant()} iconLeft={<Glyph name="plus" />} />
              <Button text="Button" variant={variant()} outlined iconLeft={<Glyph name="plus" />} />
              <Button text="Button" variant={variant()} disabled iconLeft={<Glyph name="plus" />} />
              <Button text="Button" variant={variant()} outlined disabled iconLeft={<Glyph name="plus" />} />
            </View>
          )}
        </Each>
      </Stage>
    </Section>
  );
}

/** The three sizes (level 5), side by side. */
function Sizes() {
  return (
    <Section
      title="Sizes"
      description="small for dense toolbars and tables, default for most forms and dialogs, large for touch layouts and prominent calls to action. Switch Density to see Touch's sizes."
    >
      <Stage>
        <Each of={[...sizes]}>
          {(size) => <Button text={`Size ${size()}`} size={size()} variant="primary" iconLeft={<Glyph name="plus" />} />}
        </Each>
        <Each of={[...sizes]}>{(size) => <Button text={`Size ${size()}`} size={size()} outlined />}</Each>
      </Stage>
    </Section>
  );
}

/** The usage examples of the Figma page: the component as designers place it. */
function Examples() {
  return (
    <Section title="Examples" description="Setups to copy, at the default size.">
      <View class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-gap5">
        <Example
          title="Primary and secondary actions"
          description="One primary button per view, with neutral buttons for the other choices: a dialog's footer."
        >
          <Button text="Cancel" />
          <Button text="Save changes" variant="primary" />
        </Example>
        <Example
          title="A destructive action"
          description="danger for an action that destroys something; the label says what, so color is not the only signal."
        >
          <Button text="Keep project" />
          <Button text="Delete project" variant="danger" />
        </Example>
        <Example title="Icons" description="A leading icon makes an action faster to recognize; a trailing chevron says it goes on.">
          <Button text="New file" iconLeft={<Glyph name="filePlus" />} />
          <Button text="Continue" variant="primary" showLeftIcon={false} showRightIcon iconRight={<Glyph name="chevronRight" />} />
        </Example>
        <Example title="A badge in a slot" description="slotRight holds content at icon size, such as a count badge.">
          <Button text="Inbox" iconLeft={<Glyph name="inbox" />} showSlotRight slotRight={<CountBadge count={3} />} />
        </Example>
        <Example title="Quiet actions" description="ghost buttons in toolbars and dense surfaces; outlined keeps them readable on busy ones.">
          <Button text="Undo" variant="ghost" iconLeft={<Glyph name="undo" />} />
          <Button text="Redo" variant="ghost" iconLeft={<Glyph name="redo" />} />
          <Button text="Settings" variant="ghost" outlined iconLeft={<Glyph name="settings" />} />
        </Example>
        <Example title="An action inside a form" description="link reads like a link but starts an action; use a Link to go to another page.">
          <Text class={`text-surfaces-text-muted ${type.regular}`}>Forgot your password?</Text>
          <Button text="Send a reset link" variant="link" />
        </Example>
        <Example title="Confirming success" description="success confirms a positive outcome.">
          <Button text="Mark as done" variant="success" iconLeft={<Glyph name="check" />} />
        </Example>
        <Example title="An unavailable action" description="disabled while the action cannot run; say why next to it.">
          <Button text="Submit" variant="primary" disabled iconLeft={<Glyph name="send" />} />
          <Text class={`text-surfaces-text-muted ${type.regular}`}>Fill in every required field first.</Text>
        </Example>
      </View>
    </Section>
  );
}

/** The Button page. */
export function ButtonPage() {
  return (
    <Page
      title="Button"
      subtitle="A labelled control that starts an action, such as saving, submitting or opening a dialog."
      source="Figma buttons/Button · rayx_components::button::Button"
    >
      <Playground />
      <Variants />
      <Sizes />
      <Examples />
    </Page>
  );
}
