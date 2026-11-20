// The RayX gallery: RayX's components as the Figma file shows them, in code. Its screens are TSX
// (hot reload); the components are Rust (`rayx_components`), imported from RayX's library set,
// `rayx/components`, which the app's crate serves.
import { mount, signal } from "nsh";
import { RayXAppBuilder, ThemeService } from "rayx-gallery/native";
import { Gallery, type GalleryTheme, type RayXStart } from "./src/gallery";

// RayX's start, with the four services the host requires (platform state, suspension, assets and
// fonts) and the theme service (`addTheme`, which follows the system's light or dark): the
// crate's `rayx` features. The theme service picks before `mount`, so the first frame already
// shows the system's light or dark. The pages draw while RayX starts; a start that fails says why
// in the toolbar.
const rayx = signal<RayXStart>("starting");
const theme = signal<GalleryTheme | null>(null);
new RayXAppBuilder()
  .addPlatform()
  .addSuspension()
  .addAssets()
  .addFonts()
  .addTheme()
  .start()
  .then((services) => {
    // The toolbar's pickers follow the theme service's snapshot.
    const service = services.getRequiredService(ThemeService);
    theme.set({ service, snapshot: service.current() });
    service.onChange((snapshot) => theme.set({ service, snapshot }));
    rayx.set("started");
  })
  .catch((error) => rayx.set(`RayX did not start: ${error}`));

mount(() => <Gallery rayx={rayx()} theme={theme()} />);
