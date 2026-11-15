// `nsh test` runs this file in a headless runner it builds with the gallery's crate (`native/`),
// so the Button and the theme are RayX's own.
import { expect, open, test } from "nsh/test";

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

test("the toolbar switches the theme's modes", async () => {
  const app = await open("./App.tsx");
  await app.press("id:accent-sky");
  await app.press("id:density-touch");
  await app.press("id:appearance-dark");
  expect(app.texts()).toContain("dark · sky · touch");
});
