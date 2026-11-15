// The gallery's shell: the components in a sidebar, the theme's modes in a toolbar (Figma's
// Light/Dark, Theme.Colors and Platform.Density), and the selected component's page.
import { colorScheme, Scroll, setColorScheme, signal, Text, View, windowSize } from "nsh";
import { Themes, type Accent, type Density } from "rayx-gallery/native";
import { ButtonPage } from "./pages/button";
import { Choice, type } from "./ui";

type Appearance = "system" | "light" | "dark";

/** The theme's modes the gallery shows: the toolbar owns them. */
const appearance = signal<Appearance>("system");
const accent = signal<Accent>("cyan");
const density = signal<Density>("desktop");

function setModes(next: { accent?: Accent; density?: Density }) {
  if (next.accent) accent.set(next.accent);
  if (next.density) density.set(next.density);
  Themes.set(accent(), density());
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

function Toolbar() {
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
        options={Themes.accents()}
        value={accent()}
        name={(a) => a[0].toUpperCase() + a.slice(1)}
        onChange={(a) => setModes({ accent: a })}
      />
      <Choice
        label="Density"
        options={Themes.densities()}
        value={density()}
        name={(d) => d[0].toUpperCase() + d.slice(1)}
        onChange={(d) => setModes({ density: d })}
      />
      <Text class={`text-surfaces-text-subtle ${type.code}`}>{`${colorScheme()} · ${accent()} · ${density()}`}</Text>
    </View>
  );
}

/** The gallery: the sidebar on wide windows, the toolbar and the page. */
export function Gallery() {
  const wide = () => windowSize().width >= 760;
  return (
    <View class="flex-1 flex-row bg-surfaces-app">
      <If when={wide()}>
        <Sidebar />
      </If>
      <View class="flex-1">
        <Toolbar />
        <Scroll class="flex-1">
          <ButtonPage />
        </Scroll>
      </View>
    </View>
  );
}
