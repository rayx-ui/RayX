//! Glob matching and `{subdir}` capture for asset manifests (`assets.toml`).
//!
//! Supports `*` (any characters within one path segment), `**` (zero or more directory segments),
//! `?` (one character within a segment), and `{a,b}` (brace alternation, including nested braces),
//! mirroring NSH's glob matcher on `/`-separated relative paths.

use std::cmp::Reverse;

/// Whether `pattern` contains glob syntax (`*`, `?`, or `{`).
pub fn is_glob(pattern: &str) -> bool {
    pattern.contains(['*', '?', '{'])
}

/// Whether `path` (`/`-separated, relative to `assets/`) matches `pattern`.
pub fn glob_matches(pattern: &str, path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return false;
    }
    for expanded in expand_braces(pattern) {
        let segments: Vec<&str> = expanded.split('/').filter(|s| !s.is_empty()).collect();
        if path_matches(&segments, &parts) {
            return true;
        }
    }
    false
}

/// Specificity ranking for manifest entries so broader globs apply first, more specific globs
/// override them, and exact paths override everything.
///
/// Ordering (ascending = applied earlier, descending = higher precedence):
/// 1. Tier: `0` = recursive glob (`**`), `1` = non-recursive glob (`*`, `?`, `{a,b}`), `2` = exact path.
/// 2. Literal (non-wildcard) segment count (more literal segments = more specific).
/// 3. Fewer wildcard segments (`Reverse(wildcard_segments)`).
/// 4. Literal character count (more fixed characters = more specific).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct GlobSpecificity {
    tier: u8,
    literal_segments: usize,
    fewer_wildcards: Reverse<usize>,
    literal_chars: usize,
}

impl GlobSpecificity {
    /// Computes the specificity of `pattern`.
    pub fn of(pattern: &str) -> Self {
        if !is_glob(pattern) {
            let segments: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
            return Self {
                tier: 2,
                literal_segments: segments.len(),
                fewer_wildcards: Reverse(0),
                literal_chars: pattern.chars().count(),
            };
        }

        let segments: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
        let has_globstar = segments.contains(&"**");
        let tier = if has_globstar { 0 } else { 1 };
        let mut literal_segments = 0usize;
        let mut wildcard_segments = 0usize;
        for segment in &segments {
            if is_glob(segment) {
                wildcard_segments += 1;
            } else {
                literal_segments += 1;
            }
        }
        let literal_chars = pattern
            .chars()
            .filter(|c| !matches!(c, '*' | '?' | '{' | '}' | ',' | '/'))
            .count();
        Self {
            tier,
            literal_segments,
            fewer_wildcards: Reverse(wildcard_segments),
            literal_chars,
        }
    }
}

/// Extracts the relative subdirectory captured under `pattern`'s leading static directory prefix
/// for `matched_path`.
///
/// For example:
/// - `pattern = "icons/**/*.svg"`, `matched_path = "icons/nav/home.svg"` → `"nav"`
/// - `pattern = "icons/**/*.svg"`, `matched_path = "icons/nav/arrows/left.svg"` → `"nav/arrows"`
/// - `pattern = "icons/**/*.svg"`, `matched_path = "icons/close.svg"` → `""`
/// - `pattern = "fonts/{sans,mono}/*.ttf"`, `matched_path = "fonts/sans/Inter.ttf"` → `"sans"`
pub fn capture_subdir(pattern: &str, matched_path: &str) -> String {
    let pattern_segments: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
    let path_segments: Vec<&str> = matched_path.split('/').filter(|s| !s.is_empty()).collect();
    if path_segments.len() <= 1 {
        return String::new();
    }
    let path_dirs = &path_segments[..path_segments.len() - 1];

    // Leading static directory segments of the pattern (stop at the first wildcard segment or the
    // final filename segment).
    let static_prefix_len = pattern_segments
        .iter()
        .take(pattern_segments.len().saturating_sub(1))
        .take_while(|seg| !is_glob(seg))
        .count();

    let mut matched_prefix = 0usize;
    while matched_prefix < static_prefix_len
        && matched_prefix < path_dirs.len()
        && pattern_segments[matched_prefix] == path_dirs[matched_prefix]
    {
        matched_prefix += 1;
    }

    path_dirs[matched_prefix..].join("/")
}

/// Expands a `namespace` template containing `{subdir}` for `matched_path` matched by `pattern`.
///
/// Returns `None` when the template is just `{subdir}` (or collapses cleanly to empty) and
/// `matched_path` has no captured subdirectory, leaving the asset at the root namespace.
/// Preserves invalid segments (such as `..` or empty segments written in the template itself) so
/// [`super::normalize_namespace`] can report them.
pub fn expand_namespace_template(
    template: &str,
    pattern: &str,
    matched_path: &str,
) -> Option<String> {
    if !template.contains("{subdir}") {
        return Some(template.to_owned());
    }
    let subdir = capture_subdir(pattern, matched_path);
    let mut out_segments: Vec<String> = Vec::new();
    for segment in template.split('/') {
        if segment == "{subdir}" {
            if !subdir.is_empty() {
                out_segments.extend(subdir.split('/').map(str::to_owned));
            }
        } else if segment.contains("{subdir}") {
            let replaced = segment.replace("{subdir}", &subdir);
            if !replaced.is_empty() {
                out_segments.push(replaced);
            }
        } else {
            out_segments.push(segment.to_owned());
        }
    }
    if out_segments.is_empty() {
        None
    } else {
        Some(out_segments.join("/"))
    }
}

/// Validates a glob pattern used in `assets.toml` (`path` or `exclude`).
pub(crate) fn validate_pattern(pattern: &str) -> Result<(), String> {
    let trimmed = pattern.trim();
    if trimmed.is_empty() {
        return Err("it is empty".to_owned());
    }
    if trimmed.contains('\\') {
        return Err("it uses `\\`; asset paths are `/`-separated".to_owned());
    }
    if trimmed.starts_with('/') || trimmed.contains(':') {
        return Err("it is absolute; asset paths are relative to `assets/`".to_owned());
    }
    for segment in trimmed.split('/') {
        if segment == ".." {
            return Err("it leaves `assets/`".to_owned());
        }
    }
    Ok(())
}

/// Whether `parts` matches `segments` (`**` matches zero or more segments).
fn path_matches(segments: &[&str], parts: &[&str]) -> bool {
    match (segments.first(), parts.first()) {
        (None, None) => true,
        (Some(&"**"), _) => {
            path_matches(&segments[1..], parts)
                || (!parts.is_empty() && path_matches(segments, &parts[1..]))
        }
        (Some(s), Some(p)) => segment_matches(s, p) && path_matches(&segments[1..], &parts[1..]),
        _ => false,
    }
}

/// Whether `name` matches `pattern` (`*`, `?`; braces already expanded).
fn segment_matches(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    let (mut i, mut j) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while j < n.len() {
        if i < p.len() && (p[i] == '?' || p[i] == n[j]) {
            i += 1;
            j += 1;
        } else if i < p.len() && p[i] == '*' {
            star = Some(i);
            mark = j;
            i += 1;
        } else if let Some(s) = star {
            i = s + 1;
            mark += 1;
            j = mark;
        } else {
            return false;
        }
    }
    while i < p.len() && p[i] == '*' {
        i += 1;
    }
    i == p.len()
}

/// Expands `{a,b}` brace groups in `pattern`, recursively.
fn expand_braces(pattern: &str) -> Vec<String> {
    let Some(open) = pattern.find('{') else {
        return vec![pattern.to_owned()];
    };
    let mut depth = 0;
    let mut close = None;
    for (i, c) in pattern[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(open + i);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(close) = close else {
        return vec![pattern.to_owned()];
    };
    let (head, body, tail) = (
        &pattern[..open],
        &pattern[open + 1..close],
        &pattern[close + 1..],
    );
    let mut options = Vec::new();
    let (mut depth, mut from) = (0, 0);
    for (i, c) in body.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                options.push(&body[from..i]);
                from = i + 1;
            }
            _ => {}
        }
    }
    options.push(&body[from..]);
    options
        .into_iter()
        .flat_map(|option| expand_braces(&format!("{head}{option}{tail}")))
        .collect()
}
