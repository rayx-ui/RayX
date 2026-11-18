//! The font service's TS class declares the aliases it gives as `ResolvedFontAlias[]` (the JSON of
//! Rust's `ResolvedFontAlias`), not `unknown`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use rayx_app_nsh::testing;

/// The declaration of the class `name` in `ts`, up to its closing brace.
fn class<'a>(ts: &'a str, name: &str) -> &'a str {
    ts.split(&format!("export declare class {name} {{"))
        .nth(1)
        .and_then(|rest| rest.split("\n}\n").next())
        .unwrap_or_else(|| panic!("no class {name} in\n{ts}"))
}

#[test]
fn the_aliases_are_declared_as_resolved_font_aliases() {
    let ts = testing::declarations(&testing::set(rayx_services_fonts::register));
    let declared = class(&ts, "FontService");
    let line = "  aliases(): ResolvedFontAlias[];";
    assert!(declared.contains(line), "no `{line}` in\n{declared}");
    let alias = r#"export type ResolvedFontAlias = { alias: string; requested_family: string; resolved_family: string; glyph_fallback_families: string[] };"#;
    assert!(ts.contains(alias), "no `{alias}` in\n{ts}");
    assert!(
        !ts.contains("(): unknown"),
        "a member still gives `unknown` in\n{ts}"
    );
}

#[test]
fn resolved_font_alias_has_the_keys_of_rust_s_json() {
    let alias = rayx_app::services::fonts::ResolvedFontAlias {
        alias: "body".into(),
        requested_family: "Inter".into(),
        resolved_family: "Inter".into(),
        glyph_fallback_families: vec!["Noto Color Emoji".into()],
    };
    let json = serde_json::to_value(&alias).unwrap();
    let mut json_keys: Vec<&str> = json
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    json_keys.sort_unstable();
    let ts = rayx_services_fonts::RESOLVED_FONT_ALIAS.ts;
    let mut ts_keys: Vec<&str> = ts
        .trim_matches(|c| c == '{' || c == '}' || c == ' ')
        .split(';')
        .filter_map(|field| field.split(':').next())
        .map(str::trim)
        .collect();
    ts_keys.sort_unstable();
    assert_eq!(ts_keys, json_keys);
}
