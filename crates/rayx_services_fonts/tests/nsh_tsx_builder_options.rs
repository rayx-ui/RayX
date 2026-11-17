//! `addFonts` in TS through NSH's engine, headless: its options callback sets aliases as Rust's
//! `FontOptions` does and they reach RayX's font service (replacing the host fakes' one), a
//! callback that throws makes `addFonts` throw with its message, and the options classes are
//! declared with Rust's methods.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use nsh_core::element::Registry;
use rayx_app_nsh::testing;

fn set() -> Registry {
    testing::set(|set| {
        rayx_services_fonts::register(set);
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
