//! `addFonts` in TS through NSH's engine, headless: its options callback sets aliases as Rust's
//! `FontOptions` does and they reach RayX's font service (replacing the host fakes' one), what
//! it sets applies after the options bound from `appsettings.json` (a value it sets overrides
//! the file's, one it leaves alone keeps it), a callback that throws makes `addFonts` throw with
//! its message, and the options classes are declared with Rust's methods.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use nsh_core::JsonValue;
use nsh_core::element::Registry;
use nsh_core::prelude::*;
use nsh_engine::Assets;
use rayx_app_nsh::testing;
use rayx_app_nsh::ts::ServiceProvider;
use rayx_services_fonts::FontOptions;

/// What the test reads of the app's options from Rust.
#[module]
struct Probe;

#[module]
impl Probe {
    /// The font options the service was made with.
    pub(crate) fn options(services: Instance<ServiceProvider>) -> Result<JsonValue, String> {
        let options = services
            .with(|services| services.services().options::<FontOptions>())
            .map_err(|error| error.to_string())?;
        serde_json::to_value(options.cloned()).map_err(|error| error.to_string())
    }
}

fn set() -> Registry {
    testing::set(|set| {
        rayx_services_fonts::register(set);
        set.module::<Probe>();
    })
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { FontService, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder().addHostFakes()
      .addFonts((options) => {
        options.alias("script", (alias) => { alias.family("Pacifico").glyphFallback("serif"); });
        options.alias("serif").defaultAlias("script");
      })
      .start();
    const fonts = services.getRequiredService(FontService);
    lines.push(fonts.family("script"));
    lines.push(fonts.aliases().map((alias) => `${alias.alias}:${alias.resolved_family}`).join(","));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn aliases_set_in_ts_reach_the_font_service() {
    let shown = testing::run(set(), TSX);
    let parts: Vec<&str> = shown.split('|').collect();
    assert!(parts[0].starts_with("done "), "{shown}");
    assert!(parts[0].contains("Pacifico"), "{shown}");
    assert_eq!(parts[0], "done Pacifico, serif", "{shown}");
    assert_eq!(parts[1], "script:Pacifico,serif:serif", "{shown}");
    assert_eq!(parts.len(), 2, "{shown}");
}

/// The app's files: its settings.
struct AppFiles;

impl Assets for AppFiles {
    fn read(&self, uri: &str) -> Result<Vec<u8>, String> {
        match uri {
            "asset:/@app/appsettings.json" => Ok(br#"{ "RayX": { "Services": {
                "Fonts": { "AutoRegisterAssets": false, "DefaultAlias": "serif" }
            } } }"#
                .to_vec()),
            _ => Err(format!("{uri} is not bundled")),
        }
    }
}

const LAYERED: &str = r#"import { mount, signal, Text } from "nsh";
import { FontService, Probe, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder().addHostFakes().addAppSettings("app")
      .addFonts((options) => {
        options.alias("script", (alias) => { alias.family("Pacifico").glyphFallback("serif"); });
        options.alias("serif").defaultAlias("script");
      })
      .start();
    // Set by the callback over the file's; left alone, the file's.
    const fonts = Probe.options(services);
    lines.push(`${fonts.auto_register_assets} ${fonts.default_alias} ${fonts.aliases.map((a) => `${a.alias}=${a.family}/${a.glyph_fallbacks}`).join(",")}`);
    lines.push(services.getRequiredService(FontService).family("script"));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn options_set_in_ts_apply_after_those_from_app_settings() {
    assert_eq!(
        testing::run_with_assets(set(), LAYERED, Arc::new(AppFiles)),
        "done false script script=Pacifico/serif,serif=serif/|Pacifico, serif"
    );
}

const THROWING: &str = r#"import { mount, signal, Text } from "nsh";
import { RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  const thrown = (f) => { try { f(); return "no throw"; } catch (e) { return e.message; } };
  const builder = new RayXAppBuilder().addHostFakes();
  lines.push(thrown(() => builder.addFonts((options) => {
    options.alias("script", () => { throw new Error("no family"); });
  })));
  lines.push(thrown(() => builder.addFonts(() => { throw new Error("no fonts"); })));
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_throwing_callback_makes_add_fonts_throw_with_its_message() {
    assert_eq!(testing::run(set(), THROWING), "done no family|no fonts");
}

#[test]
fn the_font_options_classes_are_declared_with_rust_s_methods() {
    let ts = testing::declarations(&set());
    for declared in [
        "export declare class FontOptions {",
        "  alias(alias: string, configure?: ((alias: FontAliasOptions) => void) | null): FontOptions;",
        "export declare class FontAliasOptions {",
        "addFonts(configure?: ((options: FontOptions) => void) | null): RayXAppBuilder;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
    // `addFonts` carries its doc comment.
    let add_fonts = ts
        .find("addFonts(configure?: ((options: FontOptions) => void) | null): RayXAppBuilder;")
        .unwrap();
    let before = ts[..add_fonts].trim_end();
    assert!(
        before.ends_with("*/"),
        "no doc comment on addFonts in\n{ts}"
    );
    let doc = &before[before.rfind("/**").unwrap()..];
    assert!(doc.contains("font service"), "{doc}");
}
