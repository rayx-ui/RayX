// The gallery's own layout: pages, sections, captioned examples and the choices that set props
// and modes. Every color, size and text style is a RayX token class; nothing here restyles a
// RayX component.
import { Text, View, type JSX } from "nsh";
import { Button } from "rayx-gallery/native";

/**
 * RayX's text styles as classes: each part of a Figma text style (font size, line height, weight,
 * letter spacing). Line height and letter spacing are points, read through `var()`: NSH's own
 * `leading-*` and `tracking-*` names are multiples and ems.
 */
export const type = {
  /** `text/typography/h3`: a page's title. */
  h3: "text-typography-h3-font-size leading-(--spacing-typography-h3-line-height) font-typography-h3-font-weight tracking-(--spacing-typography-h3-letter-spacing)",
  /** `text/typography/h5`: a section's title. */
  h5: "text-typography-h5-font-size leading-(--spacing-typography-h5-line-height) font-typography-h5-font-weight tracking-(--spacing-typography-h5-letter-spacing)",
  /** `text/typography/body2`: running text. */
  body2:
    "text-typography-body2-font-size leading-(--spacing-typography-body2-line-height) font-typography-body2-font-weight tracking-(--spacing-typography-body2-letter-spacing)",
  /** `text/size/default/title1Bold`: an example's caption title. */
  title: "text-size-default-text-title1-bold-font-size leading-(--spacing-size-default-text-title1-bold-line-height) font-size-default-text-title1-bold-font-weight tracking-(--spacing-size-default-text-title1-bold-letter-spacing)",
  /** `text/size/default/regular`: small print beside components. */
  regular:
    "text-size-default-text-regular-font-size leading-(--spacing-size-default-text-regular-line-height) font-size-default-text-regular-font-weight tracking-(--spacing-size-default-text-regular-letter-spacing)",
  /** `text/size/small/regular`: labels of the controls. */
  small:
    "text-size-small-text-regular-font-size leading-(--spacing-size-small-text-regular-line-height) font-size-small-text-regular-font-weight tracking-(--spacing-size-small-text-regular-letter-spacing)",
  /** `text/size/default/code`, in the monospaced family. */
  code: "font-typography-font-family-mono text-size-default-text-code-font-size leading-(--spacing-size-default-text-code-line-height) font-size-default-text-code-font-weight tracking-(--spacing-size-default-text-code-letter-spacing)",
};

/** A component's page: its title and subtitle over its sections. */
export function Page(props: { title: string; subtitle: string; source: string; children?: JSX.Element }) {
  return (
    <View class="gap-gap6 px-insets-inset7 py-insets-inset6">
      <View class="gap-gap2">
        <Text class={`text-surfaces-text ${type.h3}`}>{props.title}</Text>
        <Text class={`text-surfaces-text-muted ${type.body2}`}>{props.subtitle}</Text>
        <Text class={`text-surfaces-text-subtle ${type.code}`}>{props.source}</Text>
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
        <Text class={`text-surfaces-text ${type.h5}`}>{props.title}</Text>
        {props.description ? <Text class={`text-surfaces-text-muted ${type.body2}`}>{props.description}</Text> : null}
      </View>
      {props.children}
    </View>
  );
}

/** A surface the components sit on: `surfaces/panel1`, as Figma's example frames. */
export function Stage(props: { children?: JSX.Element; column?: boolean }) {
  return (
    <View
      class={`${props.column ? "flex-col items-start" : "flex-row flex-wrap items-center"} gap-gap3 p-insets-inset5 rounded-borders-radius4 bg-surfaces-panel1 border border-surfaces-border-muted`}
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
        <Text class={`text-surfaces-text ${type.title}`}>{props.title}</Text>
        <Text class={`text-surfaces-text-muted ${type.regular}`}>{props.description}</Text>
      </View>
      <Stage>{props.children}</Stage>
    </View>
  );
}

/** A small label beside a component (a matrix's row or column). */
export function Note(props: { children?: JSX.Element }) {
  return <Text class={`text-surfaces-text-subtle ${type.small}`}>{props.children}</Text>;
}

/**
 * One of several values, as small buttons: the current one is `primary`, the others `ghost`;
 * each button's test id is `<label>-<value>` (`variant-primary`). `aligned` gives the label a
 * column's width, wide enough for the playground's longest (`showRightIcon`) in both densities,
 * so a list of choices lines up. (Until RayX has its SegmentedButtons and ToggleButton in code.)
 */
export function Choice<T extends string>(props: {
  label: string;
  options: readonly T[];
  value: T;
  onChange: (value: T) => void;
  name?: (value: T) => string;
  aligned?: boolean;
}) {
  return (
    <View class="flex-row flex-wrap items-center gap-size-small-items">
      <Text class={`text-surfaces-text-muted ${type.small} ${props.aligned ? "w-[112px]" : ""}`}>{props.label}</Text>
      <Each of={[...props.options]}>
        {(option) => (
          <Button
            testID={`${props.label.toLowerCase()}-${option()}`}
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

/** A count badge, drawn from RayX's tokens until RayX has its Badge in code (`roles/danger`, lvl2). */
export function CountBadge(props: { count: number }) {
  return (
    <View class="h-size-default-lvl2-height min-w-size-default-lvl2-height px-size-small-lvl2-inset rounded-borders-radius-full bg-roles-danger-bg items-center justify-center">
      <Text class={`text-roles-danger-text ${type.small}`}>{`${props.count}`}</Text>
    </View>
  );
}
