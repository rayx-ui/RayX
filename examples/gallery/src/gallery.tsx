// The gallery's shell: the components in a sidebar, the theme in a toolbar, and the selected
// component's page. The toolbar picks the theme document (RayX's, or the sample theme of
// `themes/`, through NSH's `setTheme`), the color mode, and a context for each other modifier of
// the active document (RayX's `accent`, `density` and `os`; the sample's `brand`), through RayX's
// theme service. Its pickers come from the service's snapshot, so they always list the active
// document's modifiers and contexts.
import { Scroll, setTheme, signal, Text, type Theme, View, windowSize } from "nsh";
import type { ColorModeChoice, ThemeService, ThemeSnapshot } from "rayx-gallery/native";
import sample from "../themes/sample.json";
import { ButtonPage } from "./pages/button";
import { Choice, type } from "./ui";

/** Where RayX's start is (`App.tsx`): starting, started, or why it failed. */
export type RayXStart = "starting" | "started" | `RayX did not start: ${string}`;

/** RayX's theme service once RayX started, and its snapshot now (`App.tsx` keeps it current). */
export type GalleryTheme = { service: ThemeService; snapshot: ThemeSnapshot };

/** The theme documents the toolbar picks: RayX's default theme, or the sample theme. */
const documents = ["rayx", "sample"] as const;
type DocumentName = (typeof documents)[number];

/** The modifier the color mode drives (the theme service's default, `colorMode`). */
const COLOR_MODE = "colorMode";
const colorModes = ["System", "Light", "Dark"] as const satisfies readonly ColorModeChoice[];

/** The theme document the gallery shows: the toolbar owns it. */
const shownDocument = signal<DocumentName>("rayx");
/** What `setTheme` said of the last document picked, when it refused or warned. */
const themeSaid = signal("");

/** Makes `name` the active theme document: the sample's text, or `null` for RayX's own. */
function showDocument(name: DocumentName) {
  const said = setTheme(name === "sample" ? (sample as unknown as Theme) : null);
  shownDocument.set(name);
  themeSaid.set(said.map((d) => `${d.severity} ${d.code} ${d.pointer}`).join(", "));
}

/** A modifier as the pickers show it: its name, its contexts and the one shown. */
type Shown = { name: string; contexts: string[]; selected: string };

/** The snapshot's modifiers, each member filled in (the snapshot's are optional in TS). */
function shown(snapshot: ThemeSnapshot): Shown[] {
  return (snapshot.modifiers ?? []).map((m) => ({
    name: m.name ?? "",
    contexts: m.contexts ?? [],
    selected: m.selected ?? m.default ?? "",
  }));
}

/** A name as a label: `accent` is `Accent`, `os` is `OS`. */
function label(name: string): string {
  return name.length <= 2 ? name.toUpperCase() : name[0].toUpperCase() + name.slice(1);
}

/** The components the gallery has a page for, in the Figma file's order. */
const pages = [{ key: "button", title: "Button", section: "Buttons" }] as const;

function Sidebar() {
  return (
    <View class="w-[220px] gap-gap3 px-inset4 py-inset5 bg-surface-panel1 border-r border-surface-border-muted">
      <View class="gap-gap1 px-default-lvl6-inset">
        <Text class={`text-surface-text ${type.h5}`}>RayX</Text>
        <Text class={`text-surface-text-muted ${type.regular}`}>The design system in code</Text>
      </View>
      <View class="gap-gap1">
        <Text class={`text-surface-text-subtle ${type.small} px-default-lvl6-inset`}>{pages[0].section}</Text>
        <Each of={[...pages]}>
          {(page) => (
            <View class="h-default-lvl6-height px-default-lvl6-inset justify-center rounded-default bg-intent-neutral-bg-selected">
              <Text class={`text-surface-text ${type.regular}`}>{page().title}</Text>
            </View>
          )}
        </Each>
      </View>
    </View>
  );
}

/** The theme's pickers: the document, the color mode and one per other modifier of the document. */
function ThemePickers(props: { theme: GalleryTheme }) {
  const others = () => shown(props.theme.snapshot).filter((m) => m.name !== COLOR_MODE);
  return (
    <>
      <Choice label="Theme" options={documents} value={shownDocument()} name={label} onChange={showDocument} />
      <Choice<ColorModeChoice>
        label="Color mode"
        id="colormode"
        options={colorModes}
        value={props.theme.snapshot.color_mode ?? "System"}
        onChange={(choice) => void props.theme.service.setColorMode(choice)}
      />
      <View testID="modifier-pickers" class="flex-row flex-wrap items-center gap-gap5">
        <Each of={others()} key={(m) => m.name}>
          {(modifier) => (
            <Choice
              label={label(modifier().name)}
              id={modifier().name}
              options={modifier().contexts}
              value={modifier().selected}
              name={label}
              onChange={(context) => void props.theme.service.setContext(modifier().name, context)}
            />
          )}
        </Each>
      </View>
      <Text class={`text-surface-text-subtle ${type.code}`}>
        {[shownDocument(), ...shown(props.theme.snapshot).map((m) => m.selected)].join(" · ")}
      </Text>
    </>
  );
}

function Toolbar(props: { rayx: RayXStart; theme: GalleryTheme | null }) {
  const failed = () => props.rayx !== "starting" && props.rayx !== "started";
  return (
    <View class="flex-row flex-wrap items-center gap-gap5 px-inset7 py-inset3 bg-surface-panel1 border-b border-surface-border-muted">
      <If when={props.theme}>{(theme) => <ThemePickers theme={theme()} />}</If>
      <If when={themeSaid() !== ""}>
        <Text class={`text-surface-text-danger ${type.regular}`}>{themeSaid()}</Text>
      </If>
      <If when={failed()}>
        <Text class={`text-surface-text-danger ${type.regular}`}>{props.rayx}</Text>
      </If>
    </View>
  );
}

/**
 * The gallery: the sidebar on wide windows, the toolbar and the page. Its root's test id follows
 * RayX's start: `rayx-starting`, then `rayx-started` or `rayx-failed`.
 */
export function Gallery(props: { rayx: RayXStart; theme: GalleryTheme | null }) {
  const wide = () => windowSize().width >= 760;
  const settled = () => (props.rayx === "starting" ? "rayx-starting" : props.rayx === "started" ? "rayx-started" : "rayx-failed");
  return (
    <View testID={settled()} class="flex-1 flex-row bg-surface-app">
      <If when={wide()}>
        <Sidebar />
      </If>
      <View class="flex-1">
        <Toolbar rayx={props.rayx} theme={props.theme} />
        <Scroll class="flex-1">
          <ButtonPage />
        </Scroll>
      </View>
    </View>
  );
}
