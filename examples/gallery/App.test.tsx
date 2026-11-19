// `nsh test` runs this file in a headless runner it builds with the gallery's crate (`native/`),
// so the Button and the theme are RayX's own.
import { expect, open, test } from "nsh/test";

test("starts RayX with the four services the host requires and the theme service", async () => {
  const app = await open("./App.tsx");
  // The gallery's root is `rayx-started` once the start resolves, `rayx-failed` if it threw.
  await app.waitFor("id:rayx-started");
});

test("the Button page shows the playground, the variants and the examples", async () => {
  const app = await open("./App.tsx");
  const texts = app.texts();
  for (const text of ["Playground", "Variants", "Sizes", "Examples", "Save changes", "Delete project", "Send a reset link"]) {
    expect(texts).toContain(text);
  }
  expect(texts).toContain('<Button text="Button" iconLeft={<Glyph name="plus" />} iconRight={<Glyph name="chevronRight" />} showRightIcon={false} />');
});

test("the playground's controls set the Figma properties", async () => {
  const app = await open("./App.tsx");
  await app.press("id:variant-primary");
  await app.press("id:size-large");
  expect(app.texts()).toContain(
    '<Button text="Button" variant="primary" size="large" iconLeft={<Glyph name="plus" />} iconRight={<Glyph name="chevronRight" />} showRightIcon={false} />',
  );
  await app.press("id:playground");
  expect(app.texts()).toContain("Pressed 1 time");
});

test("the toolbar switches the theme's modes through the theme service", async () => {
  const app = await open("./App.tsx");
  // The pickers come from the theme service's snapshot, once RayX started.
  await app.waitFor("id:accent-sky");
  await app.press("id:accent-sky");
  await app.press("id:density-touch");
  await app.press("id:colormode-Dark");
  await app.waitFor("text:rayx · dark · sky · touch · windows");
});

test("the toolbar switches the theme document and keeps the color mode", async () => {
  const app = await open("./App.tsx");
  await app.waitFor("id:theme-sample");
  await app.press("id:colormode-Dark");
  await app.press("id:theme-sample");
  // The sample theme's own modifier, `brand`, in place of RayX's.
  await app.waitFor("id:brand-ember");
  await app.press("id:brand-ember");
  await app.waitFor("text:sample · dark · ember");
  await app.press("id:theme-rayx");
  await app.waitFor("text:rayx · dark · cyan · desktop · windows");
});
