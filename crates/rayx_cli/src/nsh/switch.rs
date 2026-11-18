//! `rayx nshx use <checkout|local|tag|latest>` (design §4.6, fork development): builds a
//! project's NSH from a local NSH checkout (`local`: the one in a folder above the project), or
//! moves it to a tag of the fork (`latest`: its newest `nshx-*` tag).
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
//!
//! The project's NSH version requirements follow (`nsh-core = "0.5.0"`, in `[workspace.dependencies]`
//! or a member's dependencies): one the new NSH does not meet moves to its version, the checkout
//! crate's own, or the one a fork tag names (`nshx-0.6.0-2` is NSH 0.6.0). Going back to the old tag
//! therefore moves them back.

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
    /// The newest `nshx-*` tag of the project's NSH repository.
    Latest,
    /// The local NSH checkout: the first of [`LOCAL_CHECKOUTS`] in a folder above the project.
    Local,
}

/// The word that names [`Target::Latest`]; a folder of that name is `./latest`.
pub const LATEST: &str = "latest";

/// The word that names [`Target::Local`]; a folder of that name is `./local`.
pub const LOCAL: &str = "local";

/// Where `rayx nshx use local` looks for the NSH checkout, in every folder above the project,
/// nearest first: the fork's checkout in the owner's projects folder (`…/projects/dXn/nsh/nsh`
/// beside `…/projects/RayX/RayX`), then a plain `nsh/nsh`.
pub const LOCAL_CHECKOUTS: &[&str] = &["dXn/nsh/nsh", "nsh/nsh"];

/// Reads `value` as `latest` or `local`, as a checkout when it names a folder (against `cwd`,
/// where rayx started), else as a tag.
pub fn target(value: &str, cwd: &Path) -> Result<Target> {
    if value == LATEST {
        return Ok(Target::Latest);
    }
    if value == LOCAL {
        return Ok(Target::Local);
    }
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

/// The NSH version a fork tag names: `nshx-0.6.0-2` (the version, then the fork's build number)
/// and `v0.6.0` name 0.6.0. `None` for a tag of another shape, whose version requirements stay.
pub fn tag_version(tag: &str) -> Option<semver::Version> {
    if let Some(rest) = tag.strip_prefix("nshx-") {
        let version = match rest.rsplit_once('-') {
            Some((version, build))
                if !build.is_empty() && build.chars().all(|c| c.is_ascii_digit()) =>
            {
                version
            }
            _ => rest,
        };
        return semver::Version::parse(version).ok();
    }
    semver::Version::parse(tag.strip_prefix('v')?).ok()
}

/// The version of the package in `dir`: its own, or, when it inherits it, its workspace's (the
/// nearest folder above with a `[workspace.package]` version).
pub fn crate_version(dir: &Path) -> Option<semver::Version> {
    let manifest: toml::Table = fs::read_to_string(dir.join("Cargo.toml"))
        .ok()?
        .parse()
        .ok()?;
    let version = manifest.get("package")?.get("version")?;
    if let Some(text) = version.as_str() {
        return semver::Version::parse(text).ok();
    }
    dir.ancestors().skip(1).find_map(|above| {
        let table: toml::Table = fs::read_to_string(above.join("Cargo.toml"))
            .ok()?
            .parse()
            .ok()?;
        let text = table
            .get("workspace")?
            .get("package")?
            .get("version")?
            .as_str()?;
        semver::Version::parse(text).ok()
    })
}

/// The byte range of a dependency line's version requirement, quotes left out: the whole value of
/// `nsh-core = "0.5.0"`, or the `version` key of `nsh-engine = { version = "0.5.0", … }`.
fn requirement_span(line: &str) -> Option<(String, std::ops::Range<usize>)> {
    let (key, value) = line.split_once('=')?;
    let name = key.trim().trim_matches('"');
    // `nsh-core.workspace = true` and dotted keys inherit or say something else.
    if !is_nsh_patch(name) || name.contains('.') {
        return None;
    }
    let value_at = key.len() + 1;
    let value_trimmed = value.trim_start();
    let offset = value_at + value.len() - value_trimmed.len();
    let quoted = if let Some(rest) = value_trimmed.strip_prefix('"') {
        Some((offset + 1, rest))
    } else if value_trimmed.starts_with('{') {
        let mut found = None;
        let mut search = 0;
        while let Some(at) = value_trimmed[search..].find("version") {
            let at = search + at;
            let before = value_trimmed[..at].chars().next_back();
            let after = value_trimmed[at + "version".len()..].trim_start();
            if !before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                && let Some(rest) = after.strip_prefix('=')
                && let Some(rest) = rest.trim_start().strip_prefix('"')
            {
                let start = offset + value_trimmed.len() - rest.len();
                found = Some((start, rest));
                break;
            }
            search = at + "version".len();
        }
        found
    } else {
        None
    }?;
    let (start, rest) = quoted;
    let end = start + rest.find('"')?;
    Some((name.to_string(), start..end))
}

/// `text` with every NSH version requirement of its dependency tables that `version_of` (by crate
/// name) does not meet replaced by that version; returns the text and how many lines it moved.
pub fn rewrite_requirements(
    text: &str,
    version_of: impl Fn(&str) -> Option<semver::Version>,
) -> (String, usize) {
    let mut out = String::with_capacity(text.len());
    let mut moved = 0;
    let mut in_dependencies = false;
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\r', '\n']);
        let trimmed = body.trim();
        if trimmed.starts_with('[') {
            let table = trimmed.trim_start_matches('[').trim_end_matches(']').trim();
            in_dependencies = table.ends_with("dependencies");
        } else if in_dependencies
            && let Some((name, span)) = requirement_span(body)
            && let Some(version) = version_of(&name)
            && let Ok(requirement) = semver::VersionReq::parse(&body[span.clone()])
            && !requirement.matches(&version)
        {
            moved += 1;
            out.push_str(&body[..span.start]);
            out.push_str(&version.to_string());
            out.push_str(&line[span.end..]);
            continue;
        }
        out.push_str(line);
    }
    (out, moved)
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

/// The files that may patch or require the project's NSH: every `Cargo.toml` and
/// `.cargo/config.toml` under the workspace root, outside hidden folders and [`SKIPPED`] ones.
pub fn project_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        for file in [
            dir.join("Cargo.toml"),
            dir.join(".cargo").join("config.toml"),
        ] {
            if file.is_file() {
                files.push(file);
            }
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

/// What a switch changes in the project's files.
struct Plan {
    edits: Vec<Edit>,
    /// The NSH patch lines the files have.
    patched: usize,
    /// The NSH version requirements it moves.
    requirements: usize,
}

/// The edits `rewrite` makes to the patch lines of `files` and `version_of` to their NSH version
/// requirements.
fn plan(
    files: &[PathBuf],
    mut rewrite: impl FnMut(&PatchLine) -> Option<String>,
    version_of: impl Fn(&str) -> Option<semver::Version>,
) -> Plan {
    let mut plan = Plan {
        edits: Vec::new(),
        patched: 0,
        requirements: 0,
    };
    for path in files {
        let Ok(before) = fs::read_to_string(path) else {
            continue;
        };
        let rewritten = rewrite_patch(&before, &mut rewrite);
        plan.patched += rewritten.seen;
        let (after, moved) = if path.file_name().is_some_and(|name| name == "Cargo.toml") {
            rewrite_requirements(&rewritten.text, &version_of)
        } else {
            (rewritten.text, 0)
        };
        plan.requirements += moved;
        if after != before {
            plan.edits.push(Edit {
                path: path.clone(),
                before,
                after,
            });
        }
    }
    plan
}

/// The sentence that names how many NSH version requirements a switch moved.
fn moved_requirements(count: usize, to: &str) -> String {
    match count {
        0 => String::new(),
        1 => format!("; moved 1 NSH version requirement to {to}"),
        n => format!("; moved {n} NSH version requirements to {to}"),
    }
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
/// Returns the files it changed, relative to `root`, `Cargo.lock` among them when Cargo changed it.
fn apply(runner: &mut Runner, root: &Path, edits: &[Edit]) -> Result<Vec<String>> {
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
    let mut changed: Vec<String> = edits.iter().map(|e| relative(root, &e.path)).collect();
    if fs::read(&lock).ok() != lock_before {
        changed.push("Cargo.lock".into());
    }
    Ok(changed)
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
        Target::Latest => {
            let url = fork_url(&root);
            let tag = latest_tag(runner, &url)?;
            let _ = writeln!(out, "rayx: the newest nshx tag of {url} is {tag}");
            use_tag(runner, &root, &tag, out)
        }
        Target::Local => {
            let checkout = local_checkout(&root)?;
            use_checkout(runner, &root, &checkout, out)
        }
    }
}

/// The NSH checkout `rayx nshx use local` names: the first of [`LOCAL_CHECKOUTS`] holding
/// `crates/nsh-cli` in a folder above `root`, nearest first.
pub fn local_checkout(root: &Path) -> Result<PathBuf> {
    for above in root.ancestors().skip(1) {
        for relative in LOCAL_CHECKOUTS {
            let checkout = relative
                .split('/')
                .fold(above.to_path_buf(), |path, part| path.join(part));
            if checkout
                .join("crates")
                .join("nsh-cli")
                .join("Cargo.toml")
                .is_file()
            {
                return Ok(plain(fs::canonicalize(&checkout).unwrap_or(checkout)));
            }
        }
    }
    bail!(
        "no local NSH checkout: no folder above {} holds {}; name the checkout's folder instead",
        root.display(),
        LOCAL_CHECKOUTS.join(" or ")
    )
}

/// The NSH repository the project's patch lines name (or named before a checkout), else rayx's
/// default's.
pub fn fork_url(root: &Path) -> String {
    let mut found = None;
    for path in project_files(root) {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        rewrite_patch(&text, |line| {
            if found.is_none() {
                found = git_of(line).map(|(git, _)| git);
            }
            None
        });
        if found.is_some() {
            break;
        }
    }
    found.unwrap_or_else(|| crate::project::pins::DEFAULTS.nsh.git.to_string())
}

/// The newest `nshx-*` tag of `listing`, `git ls-remote --tags` output: by NSH version, then by
/// the fork's build number (`nshx-0.6.0-1` after `nshx-0.5.0-10`, `nshx-0.5.0-10` after `-9`).
pub fn newest_nshx(listing: &str) -> Option<String> {
    listing
        .lines()
        .filter_map(|line| line.split_whitespace().nth(1))
        .filter_map(|reference| reference.strip_prefix("refs/tags/"))
        .map(|tag| tag.trim_end_matches("^{}"))
        .filter_map(|tag| {
            let (version, build) = tag.strip_prefix("nshx-")?.rsplit_once('-')?;
            let key = (
                semver::Version::parse(version).ok()?,
                build.parse::<u64>().ok()?,
            );
            Some((key, tag.to_string()))
        })
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, tag)| tag)
}

/// The newest `nshx-*` tag of `url`, from `git ls-remote`.
fn latest_tag(runner: &mut Runner, url: &str) -> Result<String> {
    let listed = runner
        .query(&CommandSpec::new("git").args(["ls-remote", "--tags", "--refs", url]))
        .with_context(|| format!("listing the tags of {url}"))?;
    if !listed.is_success() {
        bail!("`git ls-remote` failed for {url}: {}", listed.stderr.trim());
    }
    newest_nshx(&listed.stdout).ok_or_else(|| anyhow!("{url} has no nshx-* tag"))
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
    let Plan {
        edits,
        patched,
        requirements,
    } = plan(
        &project_files(root),
        |line| {
            let (_, tag) = git_of(line)?;
            back = back.take().or(tag);
            match crates.get(&line.name) {
                Some(dir) => Some(checkout_line(line, dir)),
                None => {
                    missing.push(line.name.clone());
                    None
                }
            }
        },
        |name| crates.get(name).and_then(|dir| crate_version(dir)),
    );
    if patched == 0 {
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
    let changed =
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
    let version = crates
        .get("nsh-core")
        .and_then(|dir| crate_version(dir))
        .map_or_else(|| "the checkout's".to_string(), |v| v.to_string());
    let _ = writeln!(
        out,
        "rayx: {} builds NSH from {}{}{}",
        root.display(),
        checkout.display(),
        state.describe(),
        moved_requirements(requirements, &version)
    );
    let back = back.map_or_else(
        || "rayx nshx use <tag>".to_string(),
        |tag| format!("rayx nshx use {tag}"),
    );
    let said = if changed.is_empty() {
        "rayx: nothing changed: the project already pointed at this checkout".to_string()
    } else {
        format!("rayx: changed {}", changed.join(", "))
    };
    let _ = writeln!(
        out,
        "{said}; commit none of the patch lines or Cargo.lock while they point at the checkout: `{back}` moves them back"
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
    let version = tag_version(tag);
    let Plan {
        edits,
        patched,
        requirements,
    } = plan(
        &project_files(root),
        |line| tag_line(line, tag),
        |_| version.clone(),
    );
    if patched == 0 {
        bail!(
            "{} patches no NSH crate in [patch.crates-io]; `rayx nshx use` moves those lines",
            root.display()
        );
    }
    let mut changed =
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
    let to = version.map_or_else(|| tag.to_string(), |v| v.to_string());
    let _ = writeln!(
        out,
        "rayx: {} builds NSH from {tag} ({short}){}",
        root.display(),
        moved_requirements(requirements, &to)
    );
    if changed.is_empty() {
        let _ = writeln!(
            out,
            "rayx: nothing changed: the project was already at {tag}"
        );
    } else {
        let _ = writeln!(out, "rayx: changed {}", changed.join(", "));
    }
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
    for path in project_files(root) {
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
