// The RayX gallery: RayX's components as the Figma file shows them, in code. Its screens are TSX
// (hot reload); the components are Rust (`rayx_components`), imported from the app's crate.
import { mount } from "nsh";
import { Gallery } from "./src/gallery";

mount(Gallery);
