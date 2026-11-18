//! `addAssets` on TS's `RayXAppBuilder` through NSH's engine, headless: its options callback
//! adds the packages the service reads, its options object is valid during the callback only,
//! and the declarations carry the options class and the builder method.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use nsh_engine::Assets;
use rayx_app_nsh::testing;

/// The app's files: an asset catalog listing the logo.
struct AppFiles;

impl Assets for AppFiles {
    fn read(&self, uri: &str) -> Result<Vec<u8>, String> {
        match uri {
            "asset:/@app/assets.toml" => Ok(b"files = [\"images/logo.png\"]\n".to_vec()),
            "asset:/@app/images/logo.png" => Ok(b"png".to_vec()),
            _ => Err(format!("{uri} is not bundled")),
        }
    }
}

fn set() -> nsh_core::element::Registry {
    testing::set(|set| {
        rayx_services_assets::register(set);
    })
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { AssetService, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  const thrown = (f) => { try { f(); return "no throw"; } catch (e) { return e.message; } };
  try {
    let kept;
    const services = await new RayXAppBuilder().addHostFakes()
      .addAssets((options) => { kept = options; options.package("app"); })
      .start();
    lines.push(services.getRequiredService(AssetService).url("images/logo.png"));
    // An options object is valid during its callback only.
    lines.push(thrown(() => kept.package("late")).includes("disposed") ? "disposed" : "kept");
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn add_assets_options_reach_the_service_during_their_callback_only() {
    assert_eq!(
        testing::run_with_assets(set(), TSX, Arc::new(AppFiles)),
        "done asset:/@app/images/logo.png|disposed"
    );
}

#[test]
fn the_options_class_and_add_assets_are_declared() {
    let ts = testing::declarations(&set());
    for declared in [
        "export declare class AssetOptions {",
        "  package(package: string): AssetOptions;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
    let method = "addAssets(configure?: ((options: AssetOptions) => void) | null): RayXAppBuilder;";
    let at = ts
        .find(method)
        .unwrap_or_else(|| panic!("no `{method}` in\n{ts}"));
    // With its doc comment just above it.
    let before = ts[..at].trim_end();
    assert!(
        before.ends_with("*/"),
        "no doc comment before `{method}` in\n{ts}"
    );
    let doc = &before[before.rfind("/**").expect("a doc comment")..];
    assert!(doc.contains("asset service"), "{doc}");
}
