// The gallery's own layout: pages, sections, captioned examples and the choices that set props
// and modes. Every color, size and text style is a class of a RayX role (the design system's
// semantic layer, which any theme maps onto its own tokens); nothing here restyles a RayX component.
import { Text, View, type JSX } from "nsh";
import { Button } from "rayx/components";

/**
 * RayX's text styles as classes: each is RayX's typography role of a Figma text style, which sets
 * the font family, size, weight, line height and letter spacing together (`text-h3` reads the role
 * `text-h3`).
 */
export const type = {
  /** `text/typography/h3`: a page's title. */
  h3: "text-h3",
  /** `text/typography/h5`: a section's title. */
  h5: "text-h5",
  /** `text/typography/body2`: running text. */
  body2: "text-body2",
  /** `text/size/default/title1Bold`: an example's caption title. */
  title: "text-default-title1-bold",
  /** `text/size/default/regular`: small print beside components. */
  regular: "text-default-regular",
  /** `text/size/small/regular`: labels of the controls. */
  small: "text-small-regular",
  /** `text/size/default/code`, in the monospaced family. */
  code: "text-default-code",
};

/** A component's page: its title and subtitle over its sections. */
export function Page(props: { title: string; subtitle: string; source: string; children?: JSX.Element }) {
  return (
    <View class="gap-gap6 px-inset7 py-inset6">
      <View class="gap-gap2">
        <Text class={`text-surface-text ${type.h3}`}>{props.title}</Text>
        <Text class={`text-surface-text-muted ${type.body2}`}>{props.subtitle}</Text>
        <Text class={`text-surface-text-subtle ${type.code}`}>{props.source}</Text>
      </View>
      {props.children}
    </View>
  );
}

/** A section of a page: its title, what it shows, then its content. */
export function Section(props: { title: string; description?: string; children?: JSX.Element }) {
  return (
    <View class="gap-gap4">
      <View class="gap-gap1">
        <Text class={`text-surface-text ${type.h5}`}>{props.title}</Text>
        {props.description ? <Text class={`text-surface-text-muted ${type.body2}`}>{props.description}</Text> : null}
      </View>
      {props.children}
    </View>
  );
}

/** A surface the components sit on: `surface-panel1`, as Figma's example frames. */
export function Stage(props: { children?: JSX.Element; column?: boolean }) {
  return (
    <View
      class={`${props.column ? "flex-col items-start" : "flex-row flex-wrap items-center"} gap-gap3 p-inset5 rounded-4 bg-surface-panel1 border border-surface-border-muted`}
    >
      {props.children}
    </View>
  );
}

/** A usage example: a caption (title and description) over the example, as the Figma page has. */
export function Example(props: { title: string; description: string; children?: JSX.Element }) {
  return (
    <View class="gap-gap3">
      <View class="gap-gap1">
        <Text class={`text-surface-text ${type.title}`}>{props.title}</Text>
        <Text class={`text-surface-text-muted ${type.regular}`}>{props.description}</Text>
      </View>
      <Stage>{props.children}</Stage>
    </View>
  );
}

/** A small label beside a component (a matrix's row or column). */
export function Note(props: { children?: JSX.Element }) {
  return <Text class={`text-surface-text-subtle ${type.small}`}>{props.children}</Text>;
}

/**
 * One of several values, as small buttons: the current one is `primary`, the others `ghost`. Its
 * test id is `choice-<id>` and each button's `<id>-<value>` (`variant-primary`), where `id` is the
 * label in lower case unless given. `aligned` gives the label a column's width, wide enough for the
 * playground's longest (`showRightIcon`) in both densities, so a list of choices lines up. (Until
 * RayX has its SegmentedButtons and ToggleButton in code.)
 */
export function Choice<T extends string>(props: {
  label: string;
  options: readonly T[];
  value: T;
  onChange: (value: T) => void;
  name?: (value: T) => string;
  id?: string;
  aligned?: boolean;
}) {
  const id = () => props.id ?? props.label.toLowerCase();
  return (
    <View testID={`choice-${id()}`} class="flex-row flex-wrap items-center gap-small-items">
      <Text class={`text-surface-text-muted ${type.small} ${props.aligned ? "w-[112px]" : ""}`}>{props.label}</Text>
      <Each of={[...props.options]}>
        {(option) => (
          <Button
            testID={`${id()}-${option()}`}
            text={props.name ? props.name(option()) : option()}
            size="small"
            variant={props.value === option() ? "primary" : "ghost"}
            onPress={() => props.onChange(option())}
          />
        )}
      </Each>
    </View>
  );
}

/** A count badge, drawn from RayX's roles until RayX has its Badge in code (`intent-danger`, lvl2). */
export function CountBadge(props: { count: number }) {
  return (
    <View class="h-default-lvl2-height min-w-default-lvl2-height px-small-lvl2-inset rounded-full bg-intent-danger-bg items-center justify-center">
      <Text class={`text-intent-danger-text ${type.small}`}>{`${props.count}`}</Text>
    </View>
  );
}
