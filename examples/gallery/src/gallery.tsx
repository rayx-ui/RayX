// The gallery's shell: the components in a sidebar, the theme's modes in a toolbar (Figma's
// Light/Dark, Theme.Colors and Platform.Density: the appearance and RayX's theme document's
// `accent` and `density` modifiers), and the selected component's page.
import {
  colorScheme,
  modifier,
  modifiers,
  Scroll,
  setColorScheme,
  setModifier,
  signal,
  Text,
  type ThemeModifiers,
  View,
  windowSize,
} from "nsh";
import { ButtonPage } from "./pages/button";
import { Choice, type } from "./ui";

type Appearance = "system" | "light" | "dark";

/** Where RayX's start is (`App.tsx`): starting, started, or why it failed. */
export type RayXStart = "starting" | "started" | `RayX did not start: ${string}`;

type Accent = ThemeModifiers["accent"];
type Density = ThemeModifiers["density"];

/** The appearance the gallery shows: the toolbar owns it. */
const appearance = signal<Appearance>("system");

/** The contexts of the theme's modifier `name`, in the document's order. */
function contexts<M extends keyof ThemeModifiers>(name: M): ThemeModifiers[M][] {
  return (modifiers().find((m) => m.name === name)?.contexts ?? []) as ThemeModifiers[M][];
}

/** The context of modifier `name` picked now. */
function picked<M extends keyof ThemeModifiers>(name: M): ThemeModifiers[M] {
  return modifier(name) as ThemeModifiers[M];
}

/** The components the gallery has a page for, in the Figma file's order. */
const pages = [{ key: "button", title: "Button", section: "Buttons" }] as const;

function Sidebar() {
  return (
    <View class="w-[220px] gap-gap3 px-insets-inset4 py-insets-inset5 bg-surfaces-panel1 border-r border-surfaces-border-muted">
      <View class="gap-gap1 px-size-default-lvl6-inset">
        <Text class={`text-surfaces-text ${type.h5}`}>RayX</Text>
        <Text class={`text-surfaces-text-muted ${type.regular}`}>The design system in code</Text>
      </View>
      <View class="gap-gap1">
        <Text class={`text-surfaces-text-subtle ${type.small} px-size-default-lvl6-inset`}>{pages[0].section}</Text>
        <Each of={[...pages]}>
          {(page) => (
            <View class="h-size-default-lvl6-height px-size-default-lvl6-inset justify-center rounded-size-default-borders-radius bg-roles-neutral-bg-selected">
              <Text class={`text-surfaces-text ${type.regular}`}>{page().title}</Text>
            </View>
          )}
        </Each>
      </View>
    </View>
  );
}

function Toolbar(props: { rayx: RayXStart }) {
  const failed = () => props.rayx !== "starting" && props.rayx !== "started";
  return (
    <View class="flex-row flex-wrap items-center gap-gap5 px-insets-inset7 py-insets-inset3 bg-surfaces-panel1 border-b border-surfaces-border-muted">
      <Choice
        label="Appearance"
        options={["system", "light", "dark"] as const}
        value={appearance()}
        name={(a) => a[0].toUpperCase() + a.slice(1)}
        onChange={(a) => {
          appearance.set(a);
          setColorScheme(a);
        }}
      />
      <Choice
        label="Accent"
        options={contexts("accent")}
        value={picked("accent")}
        name={(a: Accent) => a[0].toUpperCase() + a.slice(1)}
        onChange={(a: Accent) => setModifier("accent", a)}
      />
      <Choice
        label="Density"
        options={contexts("density")}
        value={picked("density")}
        name={(d: Density) => d[0].toUpperCase() + d.slice(1)}
        onChange={(d: Density) => setModifier("density", d)}
      />
      <Text class={`text-surfaces-text-subtle ${type.code}`}>{`${colorScheme()} · ${picked("accent")} · ${picked("density")}`}</Text>
      <If when={failed()}>
        <Text class={`text-surfaces-text-danger ${type.regular}`}>{props.rayx}</Text>
      </If>
    </View>
  );
}

/**
 * The gallery: the sidebar on wide windows, the toolbar and the page. Its root's test id follows
 * RayX's start: `rayx-starting`, then `rayx-started` or `rayx-failed`.
 */
export function Gallery(props: { rayx: RayXStart }) {
  const wide = () => windowSize().width >= 760;
  const settled = () => (props.rayx === "starting" ? "rayx-starting" : props.rayx === "started" ? "rayx-started" : "rayx-failed");
  return (
    <View testID={settled()} class="flex-1 flex-row bg-surfaces-app">
      <If when={wide()}>
        <Sidebar />
      </If>
      <View class="flex-1">
        <Toolbar rayx={props.rayx} />
        <Scroll class="flex-1">
          <ButtonPage />
        </Scroll>
      </View>
    </View>
  );
}
