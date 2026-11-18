//! `rayx nshx use <checkout|tag>` (design §4.6, fork development): builds a project's NSH from a
//! local NSH checkout, or moves it to a tag of the fork.
//!
//! The project's own NSH patch lines switch in place. Every `[patch.crates-io]` line of an NSH
//! crate, in the workspace's `Cargo.toml` and in each `.cargo/config.toml` under it (an app's, the
//! gallery's), becomes a path to that crate in the checkout, and a comment on the line keeps what it
//! was. A patch in a config file above the project would leave the committed files alone, but Cargo
//! merges it with the same crate's patch in a deeper config file and refuses the `git` and `path`
//! that result. `Cargo.lock` then locks NSH by path, which is how rayx reads a path pin and builds
//! the checkout's CLI. A tag puts the git lines back at that tag; in RayX's own repository it also
//! moves every tracked file that names the default pin (`project::pins`), as
//! `tests/nsh_pin_rayx.rs` requires.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

use super::pin::{self, Source};
use crate::host::{CommandSpec, Runner};

/// What `rayx nshx use` points a project at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// An NSH checkout: the folder that holds `crates/nsh-cli`.
    Checkout(PathBuf),
    /// A tag of the project's NSH repository (`nshx-0.5.0-8`).
    Tag(String),
}

/// Reads `value` as a checkout when it names a folder (against `cwd`, where rayx started), else as
/// a tag.
pub fn target(value: &str, cwd: &Path) -> Result<Target> {
    let path = cwd.join(value);
    if path.is_dir() {
        let checkout =
            plain(fs::canonicalize(&path).with_context(|| format!("reading {}", path.display()))?);
        if !checkout
            .join("crates")
            .join("nsh-cli")
            .join("Cargo.toml")
            .is_file()
        {
            bail!(
                "{} is not an NSH checkout: it has no crates/nsh-cli",
                checkout.display()
            );
        }
        return Ok(Target::Checkout(checkout));
    }
    let tag = !value.is_empty()
        && !value.starts_with(['-', '.'])
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'));
    if tag {
        Ok(Target::Tag(value.to_string()))
    } else {
        bail!("{value} is neither an NSH checkout folder nor a tag")
    }
}

/// `path` without the `\\?\` prefix Windows' canonical paths carry (a UNC path keeps it).
fn plain(path: PathBuf) -> PathBuf {
    match path.to_str().and_then(|text| text.strip_prefix(r"\\?\")) {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => path,
    }
}

/// `path` with `/` separators, as Cargo reads it on every system.
fn slashed(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// The comment `rayx nshx use` leaves on a patch line it points at a checkout, before what the
/// line was.
pub const WAS: &str = "# rayx nshx use, was ";

/// An NSH crate's line of a `[patch.crates-io]` table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatchLine {
    pub indent: String,
    pub name: String,
    /// The inline table, braces included: `{ git = "…", tag = "…" }`.
    pub value: String,
    /// What the line was before `rayx nshx use` pointed it at a checkout, braces included.
    pub was: Option<String>,
}

/// Whether a patched package is one of NSH's crates (not an app's own `nsh-app-*`).
fn is_nsh_patch(name: &str) -> bool {
    name.starts_with("nsh-") && !name.starts_with("nsh-app-")
}

/// The NSH patch line `line` holds, if it holds one.
fn parse_line(line: &str) -> Option<PatchLine> {
    let rest = line.trim_start();
    let indent = &line[..line.len() - rest.len()];
    let (name, rest) = rest.split_once('=')?;
    let name = name.trim().trim_matches('"');
    if !is_nsh_patch(name) {
        return None;
    }
    let rest = rest.trim_start();
    if !rest.starts_with('{') {
        return None;
    }
    let close = rest.find('}')?;
    let was = rest[close + 1..]
        .trim()
        .strip_prefix(WAS)
        .map(|was| was.trim().to_string());
    Some(PatchLine {
        indent: indent.to_string(),
        name: name.to_string(),
        value: rest[..=close].to_string(),
        was,
    })
}

/// The keys of an inline table (`{ git = "…", tag = "…" }`).
fn fields(value: &str) -> Option<toml::Table> {
    match format!("x = {value}")
        .parse::<toml::Table>()
        .ok()?
        .remove("x")?
    {
        toml::Value::Table(table) => Some(table),
        _ => None,
    }
}

/// The git source a patch line names (or named before it was pointed at a checkout).
fn git_of(line: &PatchLine) -> Option<(String, Option<String>)> {
    let fields = fields(line.was.as_deref().unwrap_or(&line.value))?;
    let git = fields.get("git")?.as_str()?.to_string();
    let tag = fields
        .get("tag")
        .and_then(toml::Value::as_str)
        .map(str::to_string);
    Some((git, tag))
}

/// `line` pointed at `dir`, keeping what it was.
pub fn checkout_line(line: &PatchLine, dir: &Path) -> String {
    let was = line.was.as_ref().unwrap_or(&line.value);
    format!(
        "{}{} = {{ path = \"{}\" }} {WAS}{was}",
        line.indent,
        line.name,
        slashed(dir)
    )
}

/// `line` at `tag` of its git source; `None` for a path of the project's own, which rayx leaves.
pub fn tag_line(line: &PatchLine, tag: &str) -> Option<String> {
    let (git, _) = git_of(line)?;
    Some(format!(
        "{}{} = {{ git = \"{git}\", tag = \"{tag}\" }}",
        line.indent, line.name
    ))
}

/// What [`rewrite_patch`] did to a text.
#[derive(Debug, PartialEq, Eq)]
pub struct Rewritten {
    pub text: String,
    /// The NSH patch lines the text has.
    pub seen: usize,
    /// Those `rewrite` changed.
    pub changed: usize,
}

/// `text` with each NSH line of its `[patch.crates-io]` table replaced by what `rewrite` returns
/// for it (`None` keeps the line).
pub fn rewrite_patch(
    text: &str,
    mut rewrite: impl FnMut(&PatchLine) -> Option<String>,
) -> Rewritten {
    let mut out = String::with_capacity(text.len());
    let (mut seen, mut changed) = (0, 0);
    let mut in_patch = false;
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\r', '\n']);
        let ending = &line[body.len()..];
        let trimmed = body.trim();
        if trimmed.starts_with('[') {
            in_patch = trimmed == "[patch.crates-io]";
        } else if in_patch && let Some(parsed) = parse_line(body) {
            seen += 1;
            if let Some(new) = rewrite(&parsed)
                && new != body
            {
                changed += 1;
                out.push_str(&new);
                out.push_str(ending);
                continue;
            }
        }
        out.push_str(line);
    }
    Rewritten {
        text: out,
        seen,
        changed,
    }
}

/// The packages of `checkout` by name, each with its folder: the `Cargo.toml` files one and two
/// levels under `crates/` (NSH keeps a second workspace in a folder of its own there).
pub fn checkout_crates(checkout: &Path) -> BTreeMap<String, PathBuf> {
    let mut found = BTreeMap::new();
    let mut dirs = vec![checkout.join("crates")];
    for _ in 0..2 {
        let mut next = Vec::new();
        for dir in dirs {
            for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                if let Some(name) = package_name(&path.join("Cargo.toml")) {
                    found.entry(name).or_insert_with(|| path.clone());
                }
                next.push(path);
            }
        }
        dirs = next;
    }
    found
}

fn package_name(manifest: &Path) -> Option<String> {
    let table: toml::Table = fs::read_to_string(manifest).ok()?.parse().ok()?;
    Some(table.get("package")?.get("name")?.as_str()?.to_string())
}

/// Folders a project's NSH patch never lives in: build output, dependencies, test fixtures.
const SKIPPED: &[&str] = &["target", "node_modules", "tests", "artifacts-temp"];

/// The files that may patch the project's NSH: the workspace's `Cargo.toml` and every
/// `.cargo/config.toml` under the workspace root, outside hidden folders and [`SKIPPED`] ones.
pub fn patch_files(root: &Path) -> Vec<PathBuf> {
    let mut files = vec![root.join("Cargo.toml")];
    let mut stack = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        let config = dir.join(".cargo").join("config.toml");
        if config.is_file() {
            files.push(config);
        }
        if depth == 4 {
            continue;
        }
        for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if entry.path().is_dir() && !name.starts_with('.') && !SKIPPED.contains(&name.as_ref())
            {
                stack.push((entry.path(), depth + 1));
            }
        }
    }
    files.sort();
    files
}

/// A file's text before and after an edit.
struct Edit {
    path: PathBuf,
    before: String,
    after: String,
}

/// The edits `rewrite` makes to `files`, and how many NSH patch lines they have.
fn edits(
    files: &[PathBuf],
    mut rewrite: impl FnMut(&PatchLine) -> Option<String>,
) -> (Vec<Edit>, usize) {
    let mut edits = Vec::new();
    let mut seen = 0;
    for path in files {
        let Ok(before) = fs::read_to_string(path) else {
            continue;
        };
        let rewritten = rewrite_patch(&before, &mut rewrite);
        seen += rewritten.seen;
        if rewritten.changed > 0 {
            edits.push(Edit {
                path: path.clone(),
                before,
                after: rewritten.text,
            });
        }
    }
    (edits, seen)
}

fn write_all(edits: &[Edit], after: bool) -> Result<()> {
    for edit in edits {
        let text = if after { &edit.after } else { &edit.before };
        fs::write(&edit.path, text).with_context(|| format!("writing {}", edit.path.display()))?;
    }
    Ok(())
}

/// Lets Cargo lock the project as its patch now says (`cargo metadata` writes `Cargo.lock`).
fn relock(runner: &mut Runner, root: &Path) -> Result<()> {
    let outcome = runner
        .query(&pin::cargo(
            "cargo",
            root,
            &["metadata", "--format-version", "1"],
        ))
        .context("running `cargo metadata`")?;
    if !outcome.is_success() {
        bail!("{}", outcome.stderr.trim());
    }
    Ok(())
}

/// Writes `edits` and relocks; on failure puts every file back as it was, `Cargo.lock` included.
fn apply(runner: &mut Runner, root: &Path, edits: &[Edit]) -> Result<()> {
    let lock = root.join("Cargo.lock");
    let lock_before = fs::read(&lock).ok();
    write_all(edits, true)?;
    if let Err(error) = relock(runner, root) {
        write_all(edits, false)?;
        if let Some(bytes) = lock_before {
            let _ = fs::write(&lock, bytes);
        }
        return Err(error);
    }
    Ok(())
}

fn relative(root: &Path, path: &Path) -> String {
    slashed(path.strip_prefix(root).unwrap_or(path))
}

/// `rayx nshx use <value>` for the project around `dir`.
pub fn use_in(
    runner: &mut Runner,
    dir: &Path,
    cwd: &Path,
    value: &str,
    out: &mut dyn Write,
) -> Result<u8> {
    let target = target(value, cwd)?;
    let root = pin::workspace_root(runner, dir).ok_or_else(|| {
        anyhow!(
            "{} is in no Cargo workspace; `rayx nshx use` moves a Cargo project's NSH",
            dir.display()
        )
    })?;
    match target {
        Target::Checkout(checkout) => use_checkout(runner, &root, &checkout, out),
        Target::Tag(tag) => use_tag(runner, &root, &tag, out),
    }
}

fn use_checkout(
    runner: &mut Runner,
    root: &Path,
    checkout: &Path,
    out: &mut dyn Write,
) -> Result<u8> {
    let crates = checkout_crates(checkout);
    let mut missing = Vec::new();
    let mut back = None;
    let (edits, seen) = edits(&patch_files(root), |line| {
        let (_, tag) = git_of(line)?;
        back = back.take().or(tag);
        match crates.get(&line.name) {
            Some(dir) => Some(checkout_line(line, dir)),
            None => {
                missing.push(line.name.clone());
                None
            }
        }
    });
    if seen == 0 {
        bail!(
            "{} patches no NSH crate in [patch.crates-io]; `rayx nshx use` moves those lines",
            root.display()
        );
    }
    if !missing.is_empty() {
        missing.sort();
        missing.dedup();
        bail!(
            "{} has none of {}, which {} patches; is it a checkout of the project's NSH fork?",
            checkout.display(),
            missing.join(", "),
            root.display()
        );
    }
    apply(runner, root, &edits).context("Cargo cannot build NSH from the checkout")?;
    let pin = pin::resolve(runner, root)?;
    match &pin.source {
        Source::Path { checkout: locked } if plain_eq(locked, checkout) => {}
        _ => bail!(
            "{} still locks NSH as {} after the switch",
            root.display(),
            pin.label()
        ),
    }
    let state = checkout_state(runner, checkout);
    let _ = writeln!(
        out,
        "rayx: {} builds NSH from {}{}",
        root.display(),
        checkout.display(),
        state.describe()
    );
    let mut changed: Vec<String> = edits.iter().map(|e| relative(root, &e.path)).collect();
    changed.push("Cargo.lock".into());
    let back = back.map_or_else(
        || "rayx nshx use <tag>".to_string(),
        |tag| format!("rayx nshx use {tag}"),
    );
    let _ = writeln!(
        out,
        "rayx: changed {}; commit none of them while they point at the checkout: `{back}` moves them back",
        changed.join(", ")
    );
    Ok(0)
}

fn plain_eq(a: &Path, b: &Path) -> bool {
    let canonical = |p: &Path| {
        fs::canonicalize(p)
            .map(plain)
            .unwrap_or_else(|_| p.to_path_buf())
    };
    canonical(a) == canonical(b)
}

fn use_tag(runner: &mut Runner, root: &Path, tag: &str, out: &mut dyn Write) -> Result<u8> {
    let (edits, seen) = edits(&patch_files(root), |line| tag_line(line, tag));
    if seen == 0 {
        bail!(
            "{} patches no NSH crate in [patch.crates-io]; `rayx nshx use` moves those lines",
            root.display()
        );
    }
    apply(runner, root, &edits).with_context(|| format!("Cargo cannot lock NSH at {tag}"))?;
    let pin = pin::resolve(runner, root)?;
    let Source::Git {
        reference, commit, ..
    } = &pin.source
    else {
        bail!(
            "{} locks NSH as {} after moving it to {tag}",
            root.display(),
            pin.label()
        );
    };
    if reference.tag() != Some(tag) {
        bail!(
            "{} locks NSH as {} after moving it to {tag}",
            root.display(),
            pin.label()
        );
    }
    let mut changed: Vec<String> = edits.iter().map(|e| relative(root, &e.path)).collect();
    changed.push("Cargo.lock".into());
    if let Some(old) = rayx_default(root)? {
        let new = PinText {
            tag: tag.to_string(),
            commit: commit.clone(),
            version: pin.version.clone().unwrap_or_else(|| old.version.clone()),
        };
        if old != new {
            for path in move_default(runner, root, &old, &new)? {
                if !changed.contains(&path) {
                    changed.push(path);
                }
            }
        }
    }
    let short: String = commit.chars().take(12).collect();
    let _ = writeln!(
        out,
        "rayx: {} builds NSH from {tag} ({short})",
        root.display()
    );
    let _ = writeln!(out, "rayx: changed {}", changed.join(", "));
    Ok(0)
}

/// A pin as `project::pins` spells it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PinText {
    pub tag: String,
    pub commit: String,
    pub version: String,
}

/// Where RayX's own repository keeps its default pin.
pub const PINS: &str = "crates/rayx_cli/src/project/pins.rs";

/// The default pin of RayX's own repository at `root`, read from `project::pins`' source; `None`
/// for any other project.
pub fn rayx_default(root: &Path) -> Result<Option<PinText>> {
    let path = root.join(PINS);
    let Ok(text) = fs::read_to_string(&path) else {
        return Ok(None);
    };
    let block = text
        .split_once("nsh: DefaultNsh {")
        .map(|(_, rest)| rest)
        .ok_or_else(|| anyhow!("{} has no default nsh", path.display()))?;
    let field = |name: &str| {
        quoted_field(block, name)
            .ok_or_else(|| anyhow!("{} names no default nsh {name}", path.display()))
    };
    Ok(Some(PinText {
        tag: field("tag")?,
        commit: field("commit")?,
        version: field("version")?,
    }))
}

fn quoted_field(block: &str, name: &str) -> Option<String> {
    let key = format!("{name}: \"");
    let start = block.find(&key)? + key.len();
    let end = block[start..].find('"')?;
    Some(block[start..start + end].to_string())
}

/// Moves every tracked text file of RayX's repository that names `old` to `new` (as the pin bumps
/// before it did by hand): the tag, the commit in full and as the 12-, 9- and 7-digit prefixes the
/// docs and tests use, and the default's version in `project::pins`. `CHANGELOG.md` keeps its
/// history, AgentX's records their own, and `Cargo.lock` is Cargo's. Returns the files it changed.
pub fn move_default(
    runner: &mut Runner,
    root: &Path,
    old: &PinText,
    new: &PinText,
) -> Result<Vec<String>> {
    let listed = runner
        .query(&CommandSpec::new("git").args(["ls-files", "-z"]).cwd(root))
        .context("listing the repository's files")?;
    if !listed.is_success() {
        bail!(
            "`git ls-files` failed in {}: {}",
            root.display(),
            listed.stderr.trim()
        );
    }
    let mut moved = Vec::new();
    for relative in listed.stdout.split('\0').filter(|p| !p.is_empty()) {
        if relative == "CHANGELOG.md"
            || relative == "Cargo.lock"
            || relative.starts_with(".agentx/")
        {
            continue;
        }
        let path = root.join(relative);
        // A binary or unreadable file names no pin.
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let mut after = moved_text(&text, old, new);
        if relative == PINS && old.version != new.version {
            after = after.replace(
                &format!("version: \"{}\"", old.version),
                &format!("version: \"{}\"", new.version),
            );
        }
        if after != text {
            fs::write(&path, &after).with_context(|| format!("writing {}", path.display()))?;
            moved.push(relative.to_string());
        }
    }
    Ok(moved)
}

/// `text` with `old`'s tag and commit (in full, and its 12-, 9- and 7-digit prefixes standing
/// alone) replaced by `new`'s.
pub fn moved_text(text: &str, old: &PinText, new: &PinText) -> String {
    let mut text = replace_alone(text, &old.tag, &new.tag, |c| c.is_ascii_alphanumeric());
    for len in [40, 12, 9, 7] {
        if let (Some(from), Some(to)) = (old.commit.get(..len), new.commit.get(..len)) {
            text = replace_alone(&text, from, to, |c| c.is_ascii_hexdigit());
        }
    }
    text
}

/// `text` with every `old` that no `word` character touches on either side replaced by `new`.
fn replace_alone(text: &str, old: &str, new: &str, word: impl Fn(char) -> bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(old) {
        out.push_str(&rest[..at]);
        let after = &rest[at + old.len()..];
        let alone =
            !out.chars().next_back().is_some_and(&word) && !after.chars().next().is_some_and(&word);
        out.push_str(if alone { new } else { old });
        rest = after;
    }
    out.push_str(rest);
    out
}

/// What `rayx nshx status` and `use` say of a checkout: its commit and its uncommitted files.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CheckoutState {
    pub commit: Option<String>,
    pub uncommitted: Option<usize>,
}

impl CheckoutState {
    /// ` (at c9bf1fe9a2b1, 2 uncommitted files)`, or nothing when git cannot say.
    pub fn describe(&self) -> String {
        match (&self.commit, self.uncommitted) {
            (Some(commit), Some(0)) => format!(" (at {commit})"),
            (Some(commit), Some(1)) => format!(" (at {commit}, 1 uncommitted file)"),
            (Some(commit), Some(n)) => format!(" (at {commit}, {n} uncommitted files)"),
            (Some(commit), None) => format!(" (at {commit})"),
            (None, _) => String::new(),
        }
    }
}

/// The checkout's commit and uncommitted files, from git.
pub fn checkout_state(runner: &mut Runner, checkout: &Path) -> CheckoutState {
    let mut git = |args: &[&str]| {
        runner
            .query(
                &CommandSpec::new("git")
                    .args(args.iter().copied())
                    .cwd(checkout),
            )
            .ok()
            .filter(|outcome| outcome.is_success())
            .map(|outcome| outcome.stdout)
    };
    CheckoutState {
        commit: git(&["rev-parse", "--short=12", "HEAD"]).map(|s| s.trim().to_string()),
        uncommitted: git(&["status", "--porcelain"]).map(|s| s.lines().count()),
    }
}

/// The tag the project's NSH patch named before `rayx nshx use` pointed it at a checkout.
pub fn tag_before_checkout(root: &Path) -> Option<String> {
    let mut found = None;
    for path in patch_files(root) {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        rewrite_patch(&text, |line| {
            if found.is_none() && line.was.is_some() {
                found = git_of(line).and_then(|(_, tag)| tag);
            }
            None
        });
        if found.is_some() {
            break;
        }
    }
    found
}
