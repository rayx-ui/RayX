// The RayX gallery: RayX's components as the Figma file shows them, in code. Its screens are TSX
// (hot reload); the components are Rust (`rayx_components`), imported from the app's crate.
import { mount, signal } from "nsh";
import { RayXAppBuilder } from "rayx-gallery/native";
import { Gallery, type RayXStart } from "./src/gallery";

// RayX's start, with the four services the host requires (platform state, suspension, assets and
// fonts: the crate's `rayx` features). The pages draw while it starts; a start that fails says
// why in the toolbar.
const rayx = signal<RayXStart>("starting");
new RayXAppBuilder()
  .addPlatform()
  .addSuspension()
  .addAssets()
  .addFonts()
  .start()
  .then(() => rayx.set("started"))
  .catch((error) => rayx.set(`RayX did not start: ${error}`));

mount(() => <Gallery rayx={rayx()} />);
