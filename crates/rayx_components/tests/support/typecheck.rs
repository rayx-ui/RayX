//! A real type check for the tests of the previews' types: a small app serving RayX's library,
//! with the `.nsh/types` and `tsconfig.json` NSH writes for one (`nsh_js::type_definitions`,
//! `nsh_bundler`'s files, NSH's strict settings), checked by a TypeScript compiler. No test
//! downloads one: it is `NSH_TSC`'s, else the one `rayx app check` keeps in NSH's cache, so the
//! tests that take it are `#[ignore]`d and run on demand
//! (`cargo test -p rayx_components --test preview_private -- --include-ignored`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The app's own components, `./TripPlanner` to the files beside it: a screen, which fills what
/// it is given, and the private components written in its file for its own use, exported for a
/// preview file to import.
pub const TRIP_PLANNER: &str = r#"import { Text, View } from "nsh";

export function Hero(props: { destination: string; wide?: boolean }) {
  return <Text>{props.wide ? `${props.destination}, wide` : props.destination}</Text>;
}

export function Stat(props: { value: string; label: string }) {
  return <Text>{`${props.value} ${props.label}`}</Text>;
}

export function Divider() {
  return <View />;
}

export function TripPlanner(props: { traveler: string; days?: number }) {
  return (
    <View>
      <Hero destination="Lisbon" />
      <Divider />
      <Stat value={`${props.days ?? 3}`} label={`days, ${props.traveler}`} />
    </View>
  );
}
"#;

/// What every case file starts with: the previews' types and the app's components.
pub const IMPORTS: &str = r#"import type { Previews, PrivatePreviews } from "rayx/components";
import { Divider, Hero, Stat, TripPlanner } from "./TripPlanner";
"#;

/// The compiler's file in TypeScript's package, as NSH unpacks it.
const TSC: &str = if cfg!(windows) {
    "package/lib/tsc.exe"
} else {
    "package/lib/tsc"
};

/// NSH's cache: `NSH_CACHE`, else the platform's cache directory.
fn nsh_cache() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("NSH_CACHE") {
        return Some(PathBuf::from(dir));
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        home.map(|home| home.join("Library/Caches"))
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| home.map(|home| home.join(".cache")))
    };
    base.map(|base| base.join("nsh"))
}

/// The compiler: `NSH_TSC`, else the newest TypeScript in NSH's cache.
fn tsc() -> PathBuf {
    if let Some(tsc) = std::env::var_os("NSH_TSC") {
        return PathBuf::from(tsc);
    }
    let cache = nsh_cache().expect("no cache directory: set NSH_TSC to a tsc");
    let mut found: Vec<PathBuf> = std::fs::read_dir(&cache)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with("typescript-"))
        })
        .map(|entry| entry.path().join(TSC))
        .filter(|tsc| tsc.is_file())
        .collect();
    found.sort();
    found.pop().unwrap_or_else(|| {
        panic!(
            "no TypeScript in {}: run `rayx app check` in an app once, or set NSH_TSC to a tsc",
            cache.display()
        )
    })
}

/// Writes `.nsh/types` and the two `tsconfig.json` of an app serving RayX's library into `dir`,
/// as NSH's command line does.
fn write_types(dir: &Path) {
    let sets = super::described();
    let theme = nsh_bundler::read_app_theme(None, None, &sets);
    let mut files = nsh_js::type_definitions(&sets);
    files.extend(nsh_bundler::type_definitions());
    files.extend([
        (
            "nsh/set-assets.d.ts".to_owned(),
            nsh_bundler::set_assets_declaration(&sets),
        ),
        (
            "nsh/tokens.d.ts".to_owned(),
            nsh_bundler::tokens_declaration(&theme.tokens),
        ),
        (
            "nsh/modifiers.d.ts".to_owned(),
            nsh_bundler::modifiers_declaration(&theme.tokens),
        ),
        (
            "nsh/roles.d.ts".to_owned(),
            nsh_bundler::roles_declaration(&sets).unwrap(),
        ),
    ]);
    for (path, contents) in files {
        let path = dir.join(".nsh/types").join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
    std::fs::write(dir.join(".nsh/tsconfig.json"), nsh_bundler::TSCONFIG_BASE).unwrap();
    std::fs::write(dir.join("tsconfig.json"), nsh_bundler::TSCONFIG).unwrap();
}

/// Type checks `files` (name, text), with [`TRIP_PLANNER`] beside them as `TripPlanner.tsx`, in
/// an app of their own named `name`, and gives the errors by file: each with its code and
/// message, and what the compiler adds under it. A file without errors has no entry.
pub fn errors(name: &str, files: &[(String, String)]) -> BTreeMap<String, Vec<String>> {
    let dir = std::env::temp_dir().join(format!("rayx-typecheck-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    write_types(&dir);
    std::fs::write(dir.join("TripPlanner.tsx"), TRIP_PLANNER).unwrap();
    for (file, text) in files {
        std::fs::write(dir.join(file), text).unwrap();
    }
    let tsc = tsc();
    let output = Command::new(&tsc)
        .args(["-p", ".", "--noEmit", "--pretty", "false"])
        .current_dir(&dir)
        .output()
        .unwrap_or_else(|error| panic!("run {}: {error}", tsc.display()));
    let _ = std::fs::remove_dir_all(&dir);
    let said = String::from_utf8_lossy(&output.stdout);
    let mut errors: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut last: Option<String> = None;
    for line in said.lines() {
        // `Card.preview.tsx(4,19): error TS2353: …`, then indented lines that explain it.
        let located = line
            .split_once("): error ")
            .and_then(|(place, message)| Some((place.rsplit_once('(')?.0, message)));
        match (located, &last) {
            (Some((file, message)), _) => {
                errors
                    .entry(file.to_owned())
                    .or_default()
                    .push(message.to_owned());
                last = Some(file.to_owned());
            }
            (None, Some(file)) if line.starts_with(' ') => {
                let message = errors.get_mut(file).and_then(|errors| errors.last_mut());
                if let Some(message) = message {
                    message.push('\n');
                    message.push_str(line);
                }
            }
            _ => panic!("{} said `{line}` in\n{said}", tsc.display()),
        }
    }
    assert_eq!(
        output.status.success(),
        errors.is_empty(),
        "{}: {}\n{said}{}",
        tsc.display(),
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    errors
}

/// Type checks the case files `accepted` and `refused` (name, the text under [`IMPORTS`]; for a
/// refused one, also what its errors say) in one app: every accepted file has no error, every
/// refused file has one saying each of its fragments, and no other file has any.
pub fn check(name: &str, accepted: &[(&str, &str)], refused: &[(&str, &str, &[&str])]) {
    let file = |case: &str| format!("{case}.preview.tsx");
    let files: Vec<(String, String)> = accepted
        .iter()
        .map(|(case, text)| (case, text))
        .chain(refused.iter().map(|(case, text, _)| (case, text)))
        .map(|(case, text)| (file(case), format!("{IMPORTS}\n{text}\n")))
        .collect();
    let mut errors = errors(name, &files);
    for (case, _) in accepted {
        assert_eq!(
            errors.remove(&file(case)),
            None,
            "`{case}` type checks without errors"
        );
    }
    for (case, _, fragments) in refused {
        let said = errors
            .remove(&file(case))
            .unwrap_or_else(|| panic!("`{case}` is refused by the type check"));
        for fragment in *fragments {
            assert!(
                said.iter().any(|error| error.contains(fragment)),
                "`{case}` is refused with `{fragment}`, not {said:#?}"
            );
        }
    }
    assert!(errors.is_empty(), "errors elsewhere: {errors:#?}");
}
