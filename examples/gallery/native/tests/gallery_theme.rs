//! The gallery on RayX's roles, and its theme switcher, headless.
//!
//! - **Roles only.** Every class of the gallery's screens (`App.tsx`, `src/**`) that reads the
//!   theme by name reads a role: one of RayX's (`roles.json`) or one of the NSH roles RayX's
//!   default theme maps, never a token of a theme.
//! - **The sample theme** (`themes/sample.json`) maps every role the gallery reads, each onto a
//!   semantic token of its own that references its palette.
//! - **The toolbar** switches the theme document (`setTheme`), the color mode and each other
//!   modifier's context (the theme service): after each switch the primary Button and the page
//!   draw the values the active document resolves for that selection, and the pickers list exactly
//!   the active document's modifiers and contexts. Under a dark system scheme the first frame
//!   already draws RayX's dark values.

// Loading the compiled TSX app takes `Bundle::bytecode`.
#![allow(unsafe_code, clippy::expect_used, clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use nsh_core::color::Rgba;
use nsh_core::element::description::SetDescription;
use nsh_core::element::{Registry, Token, Tokens};
use nsh_core::platform::ColorScheme;
use nsh_core::{
    ApproxTextEngine, EventKind, EventMask, EventPayload, MountBatch, MountItem, NodeId,
};
use nsh_engine::{App, Bundle, Engine, EngineConfig, MountSink};
use nsh_theme::{Severity, ThemeDocument, ValueType, code};

/// The gallery's sample theme.
const SAMPLE: &str = include_str!("../../themes/sample.json");

/// RayX's default theme.
const RAYX: &str = rayx::components::theme::DOCUMENT;

/// The modifier the color mode drives.
const COLOR_MODE: &str = "colorMode";

fn gallery_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn ui_sets() -> Vec<Registry> {
    vec![nsh_base::registry(), rayx_gallery::registry()]
}

fn document(text: &str) -> Arc<ThemeDocument> {
    let read = nsh_theme::read(text);
    assert_eq!(read.diagnostics, Vec::new(), "the document reads cleanly");
    Arc::new(read.document)
}

/// The value of the color role `role` that `text` resolves under `picks` (every other modifier at
/// its default), as the app's UI sets resolve roles, as `0xRRGGBBAA`.
fn color(text: &str, picks: &[(&str, &str)], role: &str) -> u32 {
    let mut selection = nsh_theme::Selection::new();
    for (modifier, context) in picks {
        selection = selection.with(*modifier, *context);
    }
    let specs = Registry::merged(&ui_sets()).role_specs().to_vec();
    let (tokens, _) = Tokens::resolve_with_roles(document(text), selection, Vec::new(), &specs);
    match tokens.role_by_name(role) {
        Some(Token::Color(color)) => u32::from(Rgba::from(color)),
        other => panic!("`{role}` is no color: {other:?}"),
    }
}

/// The value of the number role `role` that `text` resolves under `picks`.
fn number(text: &str, picks: &[(&str, &str)], role: &str) -> f32 {
    let mut selection = nsh_theme::Selection::new();
    for (modifier, context) in picks {
        selection = selection.with(*modifier, *context);
    }
    let specs = Registry::merged(&ui_sets()).role_specs().to_vec();
    let (tokens, _) = Tokens::resolve_with_roles(document(text), selection, Vec::new(), &specs);
    match tokens.role_by_name(role) {
        Some(Token::Number(value)) => value,
        other => panic!("`{role}` is no number: {other:?}"),
    }
}

// ---------------------------------------------------------------------------------------------
// The roles a class may read.

/// The roles the gallery's classes may read, with their types: RayX's (`roles.json`) and the NSH
/// roles RayX's default theme maps (ROLE-MAP's), typed by the token each maps onto there.
fn known_roles() -> BTreeMap<String, ValueType> {
    let mut roles: BTreeMap<String, ValueType> = rayx::components::theme::roles::all()
        .into_iter()
        .map(|role| (role.name.clone(), role.value_type))
        .collect();
    let rayx = document(RAYX);
    for (role, token) in rayx.roles.iter() {
        if !roles.contains_key(role) {
            let token = rayx.token(token).expect("mapped onto a token");
            roles.insert(role.to_owned(), token.value_type);
        }
    }
    roles
}

/// What a utility reads from the theme by its value's name.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Reads {
    /// A color role of the value's name (`bg-surface-app`).
    Color,
    /// A role of the value's name in a namespace (`h-default-lvl5-height`: `spacing-…`).
    Namespace(&'static str, ValueType),
    /// A color role, else a typography role in `text-` (`text-surface-text`, `text-h3`).
    Text,
    /// A font role in `font-`, else a number in `font-weight-`.
    Font,
}

const SPACING: Reads = Reads::Namespace("spacing-", ValueType::Number);
const RADIUS: Reads = Reads::Namespace("radius-", ValueType::Number);

/// The utilities whose value comes from the theme by name, and what each reads.
const UTILITIES: &[(&str, Reads)] = &[
    ("bg", Reads::Color),
    ("text", Reads::Text),
    ("font", Reads::Font),
    ("border", Reads::Color),
    ("border-t", Reads::Color),
    ("border-r", Reads::Color),
    ("border-b", Reads::Color),
    ("border-l", Reads::Color),
    ("border-x", Reads::Color),
    ("border-y", Reads::Color),
    ("border-s", Reads::Color),
    ("border-e", Reads::Color),
    ("outline", Reads::Color),
    ("ring", Reads::Color),
    ("ring-offset", Reads::Color),
    ("divide", Reads::Color),
    ("fill", Reads::Color),
    ("stroke", Reads::Color),
    ("decoration", Reads::Color),
    ("accent", Reads::Color),
    ("caret", Reads::Color),
    ("placeholder", Reads::Color),
    ("from", Reads::Color),
    ("via", Reads::Color),
    ("to", Reads::Color),
    ("p", SPACING),
    ("px", SPACING),
    ("py", SPACING),
    ("pt", SPACING),
    ("pr", SPACING),
    ("pb", SPACING),
    ("pl", SPACING),
    ("ps", SPACING),
    ("pe", SPACING),
    ("m", SPACING),
    ("mx", SPACING),
    ("my", SPACING),
    ("mt", SPACING),
    ("mr", SPACING),
    ("mb", SPACING),
    ("ml", SPACING),
    ("ms", SPACING),
    ("me", SPACING),
    ("gap", SPACING),
    ("gap-x", SPACING),
    ("gap-y", SPACING),
    ("space-x", SPACING),
    ("space-y", SPACING),
    ("w", SPACING),
    ("h", SPACING),
    ("min-w", SPACING),
    ("min-h", SPACING),
    ("max-w", SPACING),
    ("max-h", SPACING),
    ("size", SPACING),
    ("inset", SPACING),
    ("inset-x", SPACING),
    ("inset-y", SPACING),
    ("top", SPACING),
    ("right", SPACING),
    ("bottom", SPACING),
    ("left", SPACING),
    ("basis", SPACING),
    ("indent", SPACING),
    ("translate-x", SPACING),
    ("translate-y", SPACING),
    ("rounded", RADIUS),
    ("rounded-t", RADIUS),
    ("rounded-r", RADIUS),
    ("rounded-b", RADIUS),
    ("rounded-l", RADIUS),
    ("rounded-s", RADIUS),
    ("rounded-e", RADIUS),
    ("rounded-tl", RADIUS),
    ("rounded-tr", RADIUS),
    ("rounded-br", RADIUS),
    ("rounded-bl", RADIUS),
    ("leading", Reads::Namespace("leading-", ValueType::Number)),
    ("tracking", Reads::Namespace("tracking-", ValueType::Number)),
    ("shadow", Reads::Namespace("shadow-", ValueType::Shadow)),
    ("blur", Reads::Namespace("blur-", ValueType::Blur)),
    (
        "backdrop-blur",
        Reads::Namespace("blur-bg-", ValueType::Blur),
    ),
];

/// Values that name nothing in a theme: numbers (`flex-1`, `p-4`), fractions and keywords.
fn no_theme_value(utility: &str, value: &str) -> bool {
    let numeric = value.parse::<f64>().is_ok()
        || value
            .split_once('/')
            .is_some_and(|(a, b)| a.parse::<u32>().is_ok() && b.parse::<u32>().is_ok());
    let keyword = [
        "full",
        "auto",
        "px",
        "screen",
        "min",
        "max",
        "fit",
        "none",
        "dvh",
        "svh",
        "lvh",
        "dvw",
        "svw",
        "lvw",
        "transparent",
        "current",
        "inherit",
    ]
    .contains(&value);
    let per_utility = match utility {
        "text" => [
            "left", "center", "right", "justify", "start", "end", "wrap", "nowrap", "balance",
            "pretty", "ellipsis", "clip",
        ]
        .contains(&value),
        u if u.starts_with("border") || u == "outline" || u == "divide" => [
            "solid", "dashed", "dotted", "double", "hidden", "collapse", "separate",
        ]
        .contains(&value),
        _ => false,
    };
    numeric || keyword || per_utility
}

/// Whether `class` reads the theme by name, and if so which role it reads (`Ok(Some(role))`),
/// or the error naming why it reads none (`Err`). `Ok(None)`: it reads nothing from the theme
/// by name (`flex-row`, `border-r`, `w-[220px]`, `p-4`).
fn reads(class: &str, roles: &BTreeMap<String, ValueType>) -> Result<Option<String>, String> {
    // Variants (`md:`, `hover:`), importance and negation, and an opacity (`/70`).
    let base = class.rsplit(':').next().unwrap_or(class);
    let base = base.trim_start_matches('!').trim_start_matches('-');
    let base = match base.rsplit_once('/') {
        Some((head, alpha)) if alpha.parse::<u32>().is_ok() => head,
        _ => base,
    };
    // Every variable form reads the role of its name.
    if let Some(open) = base.find("-(") {
        let inner = base[open + 2..].trim_end_matches(')');
        let name = inner.rsplit(':').next().unwrap_or(inner);
        let Some(name) = name.strip_prefix("--") else {
            return Err(format!("`{class}`: a variable form without `--`"));
        };
        return if roles.contains_key(name) {
            Ok(Some(name.to_owned()))
        } else {
            Err(format!("`{class}` reads `{name}`, no RayX or NSH role"))
        };
    }
    // Arbitrary values read nothing by name.
    if base.contains('[') {
        return Ok(None);
    }
    // A utility without a value (`border`, `border-r`).
    if UTILITIES.iter().any(|(utility, _)| *utility == base) {
        return Ok(None);
    }
    // The longest utility the class starts with, and its value.
    let Some((utility, value, read)) = UTILITIES
        .iter()
        .filter_map(|(utility, read)| {
            base.strip_prefix(utility)
                .and_then(|rest| rest.strip_prefix('-'))
                .filter(|value| !value.is_empty())
                .map(|value| (*utility, value, *read))
        })
        .max_by_key(|(utility, _, _)| utility.len())
    else {
        return Ok(None);
    };
    let typed = |name: String, types: &[ValueType]| {
        roles
            .get(&name)
            .filter(|ty| types.contains(ty))
            .map(|_| name)
    };
    let found = match read {
        Reads::Color => typed(value.to_owned(), &[ValueType::Color]),
        Reads::Namespace(prefix, ty) => typed(format!("{prefix}{value}"), &[ty]),
        Reads::Text => typed(value.to_owned(), &[ValueType::Color])
            .or_else(|| typed(format!("text-{value}"), &[ValueType::Typography])),
        Reads::Font => typed(format!("font-{value}"), &[ValueType::Font])
            .or_else(|| typed(format!("font-weight-{value}"), &[ValueType::Number])),
    };
    // `border-outline` also reads the border width of its name.
    let found = found.or_else(|| {
        (utility == "border")
            .then(|| typed(format!("border-width-{value}"), &[ValueType::Number]))
            .flatten()
    });
    match found {
        Some(role) => Ok(Some(role)),
        None if no_theme_value(utility, value) => Ok(None),
        None => Err(format!(
            "`{class}` reads `{value}` from the theme by name, which is no RayX or NSH role"
        )),
    }
}

/// The gallery's screens: `App.tsx` and every file of `src/`.
fn screens() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "tsx" || e == "ts") {
                out.push(path);
            }
        }
    }
    let mut out = vec![gallery_dir().join("App.tsx")];
    walk(&gallery_dir().join("src"), &mut out);
    out.sort();
    out
}

/// The words of every string in `source` (`"…"`, `'…'` and the text of `` `…` ``, its `${…}`
/// left out), the classes among them: the lines that import are skipped.
fn string_words(source: &str) -> Vec<String> {
    let mut words = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("import ") || trimmed.starts_with("//") || trimmed.starts_with('*') {
            continue;
        }
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let quote = chars[i];
            if quote == '"' || quote == '\'' || quote == '`' {
                let mut text = String::new();
                i += 1;
                let mut depth = 0;
                while i < chars.len() && (chars[i] != quote || depth > 0) {
                    if quote == '`' && chars[i] == '$' && chars.get(i + 1) == Some(&'{') {
                        depth += 1;
                        text.push(' ');
                        i += 2;
                        continue;
                    }
                    if depth > 0 && chars[i] == '}' {
                        depth -= 1;
                    } else if depth == 0 {
                        text.push(chars[i]);
                    }
                    i += 1;
                }
                words.extend(text.split_whitespace().map(str::to_owned));
            }
            i += 1;
        }
    }
    words
}

/// The classes of the gallery's screens that read the theme by name, with the role each reads,
/// and the errors of those that read none.
fn class_roles() -> (BTreeMap<String, String>, Vec<String>) {
    let roles = known_roles();
    let mut read = BTreeMap::new();
    let mut errors = Vec::new();
    for path in screens() {
        let source = std::fs::read_to_string(&path).unwrap();
        for word in string_words(&source) {
            // Only class-shaped words: lower case, digits, `-`, `:`, `/`, `.`, `[…]` and `(…)`.
            let class_shaped = word.contains('-')
                && word.chars().all(|c| {
                    c.is_ascii_lowercase() || c.is_ascii_digit() || "-:/.[]()!#%_".contains(c)
                });
            if !class_shaped {
                continue;
            }
            match reads(&word, &roles) {
                Ok(Some(role)) => {
                    read.insert(word, role);
                }
                Ok(None) => {}
                Err(error) => errors.push(format!("{}: {error}", path.display())),
            }
        }
    }
    (read, errors)
}

#[test]
fn every_class_that_reads_the_theme_reads_a_role() {
    let (read, errors) = class_roles();
    assert_eq!(errors, Vec::<String>::new());
    // The scan found the gallery's classes: its surfaces, sizes, radii and text styles.
    for (class, role) in [
        ("bg-surface-app", "surface-app"),
        ("bg-surface-panel1", "surface-panel1"),
        ("text-surface-text-muted", "surface-text-muted"),
        (
            "bg-intent-neutral-bg-selected",
            "intent-neutral-bg-selected",
        ),
        ("px-inset7", "spacing-inset7"),
        ("gap-gap3", "spacing-gap3"),
        ("h-default-lvl6-height", "spacing-default-lvl6-height"),
        ("rounded-default", "radius-default"),
        ("rounded-4", "radius-4"),
        ("rounded-full", "radius-full"),
        ("text-h3", "text-h3"),
        ("text-default-title1-bold", "text-default-title1-bold"),
        ("text-default-code", "text-default-code"),
    ] {
        assert_eq!(
            read.get(class).map(String::as_str),
            Some(role),
            "{class}: {read:?}"
        );
    }
}

#[test]
fn the_class_check_tells_roles_from_tokens_and_from_classes_without_a_theme_value() {
    let roles = known_roles();
    // Roles: RayX's and NSH's, by namespace, by color, and in a variable form.
    for (class, role) in [
        ("bg-surface-panel1", "surface-panel1"),
        (
            "hover:bg-intent-primary-bg-hover",
            "intent-primary-bg-hover",
        ),
        ("bg-primary/70", "primary"),
        ("text-muted-foreground", "muted-foreground"),
        ("p-gap4", "spacing-gap4"),
        ("min-w-default-lvl2-height", "spacing-default-lvl2-height"),
        ("rounded-small", "radius-small"),
        ("text-small-regular", "text-small-regular"),
        ("font-mono", "font-mono"),
        ("border-outline", "border-width-outline"),
        ("shadow-md", "shadow-md"),
        ("rounded-(--radius)", "radius"),
        ("bg-(--surface-app)", "surface-app"),
    ] {
        assert_eq!(reads(class, &roles), Ok(Some(role.to_owned())), "{class}");
    }
    // Classes that read no theme value by name.
    for class in [
        "flex-row",
        "border",
        "border-r",
        "w-[220px]",
        "min-w-[320px]",
        "p-4",
        "flex-1",
        "items-center",
        "md:grid-cols-2",
        "text-center",
        "w-full",
        "gap-0",
    ] {
        assert_eq!(reads(class, &roles), Ok(None), "{class}");
    }
    // Tokens of RayX's theme that are no role, the old token names, and unknown names.
    for class in [
        "bg-theme-colors-surfaces-app",
        "bg-surfaces-panel1",
        "px-insets-inset4",
        "h-size-default-lvl6-height",
        "rounded-borders-radius4",
        "bg-roles-neutral-bg-selected",
        "text-typography-h3-font-size",
        "leading-(--spacing-typography-h3-line-height)",
        "bg-palette-red-500",
        "rounded-lg",
        "bg-spacing-gap1",
    ] {
        assert!(
            reads(class, &roles).is_err(),
            "{class}: {:?}",
            reads(class, &roles)
        );
    }
}

// ---------------------------------------------------------------------------------------------
// The sample theme.

#[test]
fn the_sample_theme_maps_every_role_the_gallery_reads_onto_its_semantic_tokens() {
    let sample = document(SAMPLE);
    assert_eq!(sample.name.as_deref(), Some("sample"));
    let modifiers: Vec<&str> = sample.modifiers.keys().collect();
    assert_eq!(modifiers, ["colorMode", "brand"]);

    // Every role RayX requires (Button's) maps with no error, and no required role is unmapped.
    let specs = Registry::merged(&ui_sets()).role_specs().to_vec();
    for picks in [
        vec![],
        vec![("colorMode", "dark")],
        vec![("brand", "ember")],
        vec![("colorMode", "dark"), ("brand", "ember")],
    ] {
        let mut selection = nsh_theme::Selection::new();
        for (m, c) in &picks {
            selection = selection.with(*m, *c);
        }
        let resolved = nsh_theme::resolve(&sample, &selection, &[]);
        let said: Vec<_> = nsh_theme::check_roles(&sample, &[], &resolved, &specs, &[])
            .into_iter()
            .filter(|d| d.severity == Severity::Error || d.code == code::ROLE_UNMAPPED)
            .collect();
        assert_eq!(said, Vec::new(), "{picks:?}");
    }
    // Every role a gallery class reads.
    let (read, _) = class_roles();
    let unmapped: BTreeSet<&String> = read
        .values()
        .filter(|role| !sample.roles.contains_key(role))
        .collect();
    assert_eq!(unmapped, BTreeSet::new());

    // D-GALLERY: each role maps onto a semantic token, never the palette, and each semantic
    // token a role maps onto references the palette, in every context.
    let json: serde_json::Value = serde_json::from_str(SAMPLE).unwrap();
    let palette_refs = |value: &serde_json::Value| -> bool {
        let is_ref = |s: &str| s.starts_with("{palette-") && s.ends_with('}');
        match value {
            serde_json::Value::String(s) => is_ref(s),
            serde_json::Value::Object(fields) => {
                fields.values().all(|v| v.as_str().is_some_and(is_ref))
            }
            _ => false,
        }
    };
    for (role, token) in sample.roles.iter() {
        assert!(
            !token.starts_with("palette-"),
            "`{role}` maps onto the palette's `{token}`"
        );
        assert!(
            palette_refs(&json["tokens"][token]["value"]),
            "`{role}`'s `{token}` does not reference the palette"
        );
        for (modifier, m) in json["modifiers"].as_object().unwrap() {
            for (context, values) in m["contexts"].as_object().unwrap() {
                if let Some(value) = values.get(token) {
                    assert!(
                        palette_refs(value),
                        "`{token}` in `{modifier}: {context}` does not reference the palette"
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The toolbar, headless.

struct ChannelSink(Sender<MountBatch>);

impl MountSink for ChannelSink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.0.send(batch);
    }
}

#[derive(Default)]
struct Screen {
    parents: HashMap<NodeId, NodeId>,
    texts: HashMap<NodeId, String>,
    heights: HashMap<NodeId, f32>,
    ids: HashMap<String, NodeId>,
    events: HashMap<NodeId, EventMask>,
    backgrounds: HashMap<NodeId, u32>,
    /// Every background each view was drawn with, in order.
    drawn: HashMap<NodeId, Vec<u32>>,
}

impl Screen {
    fn apply(&mut self, batch: &MountBatch) {
        for item in &batch.items {
            match item {
                MountItem::Insert { parent, child, .. } => {
                    self.parents.insert(*child, *parent);
                }
                MountItem::Remove { id } => {
                    self.parents.remove(id);
                }
                MountItem::Delete { id } => {
                    self.parents.remove(id);
                    self.texts.remove(id);
                    self.events.remove(id);
                    self.ids.retain(|_, node| node != id);
                }
                MountItem::Text { id, text, .. } => {
                    self.texts.insert(*id, text.text.to_string());
                }
                MountItem::Frame { id, frame } => {
                    self.heights.insert(*id, frame.size.height.0);
                }
                MountItem::ViewProps { id, props } => {
                    if let Some(test_id) = &props.test_id {
                        self.ids.insert(test_id.to_string(), *id);
                    }
                }
                MountItem::Events { id, events } => {
                    self.events.insert(*id, *events);
                }
                MountItem::ViewStyle { id, style } => {
                    let color = style.background_color.0;
                    self.backgrounds.insert(*id, color);
                    self.drawn.entry(*id).or_default().push(color);
                }
                _ => {}
            }
        }
    }

    /// Whether `node` is in `ancestor`'s subtree.
    fn within(&self, mut node: NodeId, ancestor: NodeId) -> bool {
        loop {
            if node == ancestor {
                return true;
            }
            match self.parents.get(&node) {
                Some(parent) => node = *parent,
                None => return false,
            }
        }
    }

    /// The gallery's root view.
    fn root(&self) -> Option<NodeId> {
        ["rayx-started", "rayx-starting", "rayx-failed"]
            .iter()
            .find_map(|id| self.ids.get(*id).copied())
    }

    fn shows(&self, text: &str) -> bool {
        let Some(root) = self.root() else {
            return false;
        };
        self.texts
            .iter()
            .any(|(id, t)| t == text && self.within(*id, root))
    }

    /// Whether `node` is in one of the gallery's choices (`choice-*`).
    fn in_a_choice(&self, node: NodeId) -> bool {
        self.ids
            .iter()
            .any(|(id, n)| id.starts_with("choice-") && self.within(node, *n))
    }

    /// The button labelled `label` outside the choices (the playground's `text` choice has the
    /// examples' labels): the nearest view around its text that hears presses.
    fn button(&self, label: &str) -> Option<NodeId> {
        let root = self.root()?;
        let mut at = self
            .texts
            .iter()
            .find(|(id, t)| {
                t.as_str() == label && self.within(**id, root) && !self.in_a_choice(**id)
            })
            .map(|(id, _)| *id)?;
        loop {
            if self
                .events
                .get(&at)
                .is_some_and(|e| e.contains(EventKind::Press))
            {
                return Some(at);
            }
            at = *self.parents.get(&at)?;
        }
    }

    /// The page's background, and the primary Button's ("Save changes").
    fn backgrounds(&self) -> Option<(u32, u32)> {
        let root = self.root()?;
        let save = self.button("Save changes")?;
        Some((*self.backgrounds.get(&root)?, *self.backgrounds.get(&save)?))
    }

    /// The modifier pickers shown: each one's name and contexts.
    fn pickers(&self) -> BTreeMap<String, BTreeSet<String>> {
        let Some(root) = self.root() else {
            return BTreeMap::new();
        };
        let Some(&pickers) = self.ids.get("modifier-pickers") else {
            return BTreeMap::new();
        };
        if !self.within(pickers, root) {
            return BTreeMap::new();
        }
        let mut out = BTreeMap::new();
        for (id, &node) in &self.ids {
            let Some(name) = id.strip_prefix("choice-") else {
                continue;
            };
            if node == pickers || !self.within(node, pickers) {
                continue;
            }
            let contexts = self
                .ids
                .iter()
                .filter(|(other, n)| !other.starts_with("choice-") && self.within(**n, node))
                .filter_map(|(other, _)| other.strip_prefix(&format!("{name}-")))
                .map(str::to_owned)
                .collect();
            out.insert(name.to_owned(), contexts);
        }
        out
    }
}

struct Gallery {
    engine: Engine,
    rx: Receiver<MountBatch>,
    screen: Screen,
}

impl Gallery {
    /// The gallery on a headless engine whose platform shows `scheme`, once RayX started.
    fn start(scheme: ColorScheme) -> Gallery {
        let entry = gallery_dir().join("App.tsx");
        let sets = ui_sets();
        // The sets and the ones they serve (RayX's library, `rayx/components`), as a host serves
        // them.
        let described: Vec<SetDescription> = Registry::served_sets(&sets)
            .into_iter()
            .map(SetDescription::from)
            .collect();
        let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
            development: false,
            app: nsh_bundler::AppInfo::default(),
            asset_root_from: None,
            app_dir: None,
            entry,
            minify: false,
            split: false,
            ui_sets: described.clone(),
        })
        .unwrap();
        let bytecode = nsh_js::compile_bytecode_with(&bundle.code, false, &described).unwrap();
        let (tx, rx) = crossbeam_channel::unbounded();
        // SAFETY: compiled just now by this runtime.
        let app = App::Js(unsafe { Bundle::bytecode(bytecode) });
        let mut config = EngineConfig {
            ui_sets: sets,
            ..EngineConfig::new(app)
        };
        config.environment.color_scheme = scheme;
        let engine = Engine::start(config, || ApproxTextEngine, ChannelSink(tx)).unwrap();
        let mut gallery = Gallery {
            engine,
            rx,
            screen: Screen::default(),
        };
        assert!(
            gallery.wait(|s| s.ids.contains_key("rayx-started")
                && s.ids.contains_key("modifier-pickers")
                && s.backgrounds().is_some()),
            "RayX did not start: {:?}",
            gallery
                .screen
                .texts
                .values()
                .filter(|t| t.contains("RayX"))
                .collect::<Vec<_>>()
        );
        gallery
    }

    fn wait(&mut self, done: impl Fn(&Screen) -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !done(&self.screen) {
            match self.rx.recv_deadline(deadline) {
                Ok(batch) => self.screen.apply(&batch),
                Err(_) => return false,
            }
        }
        while let Ok(batch) = self.rx.recv_timeout(Duration::from_millis(50)) {
            self.screen.apply(&batch);
        }
        true
    }

    fn press(&self, test_id: &str) {
        let id = *self
            .screen
            .ids
            .get(test_id)
            .unwrap_or_else(|| panic!("no `{test_id}`"));
        for kind in [EventKind::PressDown, EventKind::PressUp, EventKind::Press] {
            self.engine
                .control()
                .dispatch_event(id, kind, EventPayload::None);
        }
    }

    /// Asserts that the page and the primary Button draw what `text` resolves under `picks`.
    fn draws(&mut self, text: &str, picks: &[(&str, &str)]) {
        let page = color(text, picks, "surface-app");
        let primary = color(text, picks, "intent-primary-bg");
        assert!(
            self.wait(|s| s.backgrounds() == Some((page, primary))),
            "under {picks:?}: the page and the primary Button draw {:08x?}, not {:08x?}",
            self.screen.backgrounds(),
            (page, primary)
        );
    }

    /// Asserts that the pickers list exactly the modifiers and contexts of `text` but the color
    /// mode's.
    fn lists(&mut self, text: &str) {
        let doc = document(text);
        let expected: BTreeMap<String, BTreeSet<String>> = doc
            .modifiers
            .iter()
            .filter(|(name, _)| *name != COLOR_MODE)
            .map(|(name, m)| {
                (
                    name.to_owned(),
                    m.contexts.keys().map(str::to_owned).collect(),
                )
            })
            .collect();
        assert!(
            self.wait(|s| s.pickers() == expected),
            "the pickers list {:?}, not {expected:?}",
            self.screen.pickers()
        );
    }
}

#[test]
fn under_a_dark_scheme_the_first_frame_draws_rayx_s_dark_values() {
    let mut gallery = Gallery::start(ColorScheme::Dark);
    let root = gallery.screen.root().unwrap();
    let save = gallery.screen.button("Save changes").unwrap();
    let dark = [("colorMode", "dark")];
    let first = |node: NodeId| {
        gallery
            .screen
            .drawn
            .get(&node)
            .and_then(|d| d.first().copied())
    };
    assert_eq!(
        first(root),
        Some(color(RAYX, &dark, "surface-app")),
        "the page's first background: {:08x?}; the Button's: {:08x?}",
        gallery.screen.drawn.get(&root),
        gallery.screen.drawn.get(&save)
    );
    assert_eq!(
        first(save),
        Some(color(RAYX, &dark, "intent-primary-bg")),
        "the primary Button's first background: {:08x?}",
        gallery.screen.drawn.get(&save)
    );
    gallery.draws(RAYX, &dark);
    gallery.lists(RAYX);
    // Light whatever the scheme, then the scheme's again.
    gallery.press("colormode-Light");
    gallery.draws(RAYX, &[("colorMode", "light")]);
    gallery.press("colormode-System");
    gallery.draws(RAYX, &dark);
    drop(gallery.engine);
}

#[test]
fn the_color_mode_follows_the_scheme_or_shows_the_choice() {
    let mut gallery = Gallery::start(ColorScheme::Light);
    gallery.draws(RAYX, &[("colorMode", "light")]);
    gallery.lists(RAYX);
    assert!(gallery.wait(|s| s.shows("rayx · light · cyan · desktop · windows")));
    gallery.press("colormode-Dark");
    gallery.draws(RAYX, &[("colorMode", "dark")]);
    gallery.lists(RAYX);
    assert!(gallery.wait(|s| s.shows("rayx · dark · cyan · desktop · windows")));
    gallery.press("colormode-Light");
    gallery.draws(RAYX, &[("colorMode", "light")]);
    gallery.lists(RAYX);
    gallery.press("colormode-Dark");
    gallery.draws(RAYX, &[("colorMode", "dark")]);
    gallery.lists(RAYX);
    gallery.press("colormode-System");
    gallery.draws(RAYX, &[("colorMode", "light")]);
    gallery.lists(RAYX);
    drop(gallery.engine);
}

#[test]
fn the_accent_and_density_pickers_pick_rayx_s_contexts() {
    let mut gallery = Gallery::start(ColorScheme::Light);
    gallery.lists(RAYX);
    gallery.press("accent-sky");
    gallery.draws(RAYX, &[("accent", "sky")]);
    gallery.press("accent-blue");
    gallery.draws(RAYX, &[("accent", "blue")]);
    gallery.press("density-touch");
    let touch = [("accent", "blue"), ("density", "touch")];
    gallery.draws(RAYX, &touch);
    let height = number(RAYX, &touch, "spacing-default-lvl5-height");
    let playground = gallery.screen.ids["playground"];
    assert!(
        gallery.wait(|s| s.heights.get(&playground) == Some(&height)),
        "Touch's default lvl5 height"
    );
    assert!(gallery.wait(|s| s.shows("rayx · light · blue · touch · windows")));
    gallery.lists(RAYX);
    drop(gallery.engine);
}

#[test]
fn the_theme_picker_switches_the_document_and_keeps_the_color_mode() {
    let mut gallery = Gallery::start(ColorScheme::Light);
    gallery.press("accent-sky");
    gallery.press("colormode-Dark");
    gallery.draws(RAYX, &[("colorMode", "dark"), ("accent", "sky")]);

    // The sample theme: its own modifiers, the color mode kept.
    gallery.press("theme-sample");
    gallery.draws(SAMPLE, &[("colorMode", "dark")]);
    gallery.lists(SAMPLE);
    assert!(gallery.wait(|s| s.shows("sample · dark · moss")));
    gallery.press("brand-ember");
    gallery.draws(SAMPLE, &[("colorMode", "dark"), ("brand", "ember")]);
    gallery.press("colormode-Light");
    gallery.draws(SAMPLE, &[("colorMode", "light"), ("brand", "ember")]);
    gallery.press("colormode-Dark");

    // Back to RayX's: its modifiers again, the color mode kept, the others at their defaults.
    gallery.press("theme-rayx");
    gallery.draws(RAYX, &[("colorMode", "dark")]);
    gallery.lists(RAYX);
    assert!(gallery.wait(|s| s.shows("rayx · dark · cyan · desktop · windows")));
    drop(gallery.engine);
}
