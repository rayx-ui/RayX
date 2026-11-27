use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::glob::{
    GlobSpecificity, expand_namespace_template, glob_matches, is_glob, validate_pattern,
};
use super::path::{
    DENSITY_SCALES, is_hidden, normalize, normalize_namespace, split_density, variant_path,
};
use super::{AssetError, AssetKind, AssetManifest, AssetManifestEntry};

/// How bad a manifest problem is.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum AssetDiagnosticSeverity {
    Warning,
    Error,
}

/// A problem found reading the manifests.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AssetDiagnostic {
    pub severity: AssetDiagnosticSeverity,
    pub package: String,
    pub path: Option<String>,
    pub message: String,
}

/// An asset the catalog knows: where it is, what it is and its metadata.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AssetDescriptor {
    /// The package (crate) whose `assets/` directory holds it.
    pub package: String,
    /// Its path in that directory.
    pub path: String,
    pub kind: AssetKind,
    pub mime_type: Option<String>,
    /// Virtual namespace (`/`-separated), or `None` for the root (`Uncategorized`).
    #[serde(default)]
    pub namespace: Option<String>,
    /// Optional logical path override for mutually exclusive `when` entries.
    #[serde(default)]
    pub alias: Option<String>,
    /// The densities it ships in, ascending, `1` being the file itself; empty when the file is
    /// the only one.
    pub scales: Vec<f32>,
    pub tags: Vec<String>,
    pub locale: Option<String>,
    pub theme: Option<String>,
    pub platform: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Build-time inclusion predicate (`when = { platform = ["ios", "macos"], brand = "acme" }`).
    #[serde(default)]
    pub when: BTreeMap<String, Vec<String>>,
    pub custom: serde_json::Value,
}

impl AssetDescriptor {
    /// `@<package>/<path>`: its path among the app's assets.
    pub fn logical_path(&self) -> String {
        format!("@{}/{}", self.package, self.path)
    }

    /// `asset:/@<package>/<path>`: the URL NSH's elements and `fetch` load it by.
    pub fn uri(&self) -> String {
        format!("asset:/{}", self.logical_path())
    }

    /// The URL of the variant that fits `scale` device pixels per point: the smallest at
    /// least that dense, else the densest (as NSH's images pick).
    pub fn uri_for_scale(&self, scale: f32) -> String {
        let Some(chosen) = self
            .scales
            .iter()
            .copied()
            .find(|candidate| *candidate >= scale)
            .or_else(|| self.scales.last().copied())
        else {
            return self.uri();
        };
        if chosen == 1.0 {
            self.uri()
        } else {
            format!(
                "asset:/@{}/{}",
                self.package,
                variant_path(&self.path, chosen)
            )
        }
    }

    /// The path's extension, lowercase.
    pub fn extension(&self) -> Option<String> {
        let name = self.path.rsplit('/').next()?;
        name.rsplit_once('.')
            .map(|(_, extension)| extension.to_ascii_lowercase())
    }
}

/// Which assets [`AssetCatalog::query`] returns: every set criterion must match.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AssetQuery {
    pub packages: BTreeSet<String>,
    pub kinds: BTreeSet<AssetKind>,
    /// Every one of them.
    pub tags: BTreeSet<String>,
    pub locale: Option<String>,
    pub theme: Option<String>,
    pub platform: Option<String>,
    /// Lowercase, without the dot.
    pub extensions: BTreeSet<String>,
    /// A path prefix in the package's directory (`images/`).
    pub prefix: Option<String>,
    /// Virtual namespace to match (`icons/navigation`).
    pub namespace: Option<String>,
    /// When `true` and [`Self::namespace`] is set, also match descendant namespaces
    /// (`icons/navigation/arrows` under `icons/navigation`).
    pub namespace_recursive: bool,
}

impl AssetQuery {
    /// The assets of `kind`.
    pub fn kind(kind: AssetKind) -> Self {
        Self {
            kinds: BTreeSet::from([kind]),
            ..Self::default()
        }
    }

    /// Whether `descriptor` matches.
    pub fn matches(&self, descriptor: &AssetDescriptor) -> bool {
        (self.packages.is_empty() || self.packages.contains(&descriptor.package))
            && (self.kinds.is_empty() || self.kinds.contains(&descriptor.kind))
            && self.tags.iter().all(|tag| descriptor.tags.contains(tag))
            && option_matches(&self.locale, &descriptor.locale)
            && option_matches(&self.theme, &descriptor.theme)
            && option_matches(&self.platform, &descriptor.platform)
            && (self.extensions.is_empty()
                || descriptor
                    .extension()
                    .is_some_and(|extension| self.extensions.contains(&extension)))
            && self
                .prefix
                .as_ref()
                .is_none_or(|prefix| descriptor.path.starts_with(prefix.as_str()))
            && namespace_matches(
                &self.namespace,
                self.namespace_recursive,
                &descriptor.namespace,
            )
    }
}

fn option_matches(wanted: &Option<String>, actual: &Option<String>) -> bool {
    wanted
        .as_ref()
        .is_none_or(|wanted| actual.as_deref() == Some(wanted.as_str()))
}

fn namespace_matches(wanted: &Option<String>, recursive: bool, actual: &Option<String>) -> bool {
    let Some(wanted) = wanted.as_deref() else {
        return true;
    };
    let wanted = wanted.trim_matches('/');
    let Some(actual) = actual.as_deref() else {
        return false;
    };
    if actual == wanted {
        return true;
    }
    if recursive && !wanted.is_empty() {
        return actual
            .strip_prefix(wanted)
            .is_some_and(|rest| rest.starts_with('/'));
    }
    false
}

/// The app's assets, from the manifests of its packages: looked up by logical path
/// (`@<package>/<path>`), by path in the first package that has it, or by query.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AssetCatalog {
    packages: Vec<String>,
    entries: BTreeMap<String, Arc<AssetDescriptor>>,
    #[serde(default)]
    namespaces: BTreeSet<String>,
    diagnostics: Vec<AssetDiagnostic>,
}

impl AssetCatalog {
    pub fn builder() -> AssetCatalogBuilder {
        AssetCatalogBuilder::default()
    }

    /// The packages, in the order their manifests were added.
    pub fn packages(&self) -> &[String] {
        &self.packages
    }

    /// Every virtual namespace declared in manifests or used by cataloged assets.
    pub fn namespaces(&self) -> &BTreeSet<String> {
        &self.namespaces
    }

    /// Every asset, by logical path.
    pub fn iter(&self) -> impl Iterator<Item = &Arc<AssetDescriptor>> {
        self.entries.values()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The asset at `path`: a logical path (`@<package>/<path>`), or a path in the first
    /// package (in manifest order) that has it. Also matches an entry's `alias` if no exact path
    /// matches.
    pub fn get(&self, path: &str) -> Option<&Arc<AssetDescriptor>> {
        if let Some(rest) = path.strip_prefix('@') {
            if let Some(entry) = self.entries.get(path) {
                return Some(entry);
            }
            if let Some((pkg, rel)) = rest.split_once('/') {
                let rel = normalize(rel).ok()?;
                return self
                    .entries
                    .values()
                    .find(|d| d.package == pkg && d.alias.as_deref() == Some(rel.as_str()));
            }
            return None;
        }
        let path = normalize(path).ok()?;
        if let Some(entry) = self
            .packages
            .iter()
            .find_map(|package| self.entries.get(&format!("@{package}/{path}")))
        {
            return Some(entry);
        }
        self.packages.iter().find_map(|package| {
            self.entries
                .values()
                .find(|d| d.package == *package && d.alias.as_deref() == Some(path.as_str()))
        })
    }

    /// The assets `query` matches, by logical path.
    pub fn query(&self, query: &AssetQuery) -> Vec<Arc<AssetDescriptor>> {
        self.entries
            .values()
            .filter(|descriptor| query.matches(descriptor))
            .cloned()
            .collect()
    }

    /// What reading the manifests found wrong.
    pub fn diagnostics(&self) -> &[AssetDiagnostic] {
        &self.diagnostics
    }

    /// The catalog without its diagnostics (when the app does not validate its manifests).
    pub fn without_diagnostics(mut self) -> Self {
        self.diagnostics.clear();
        self
    }

    /// Whether a diagnostic is an error.
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == AssetDiagnosticSeverity::Error)
    }
}

/// Staged metadata for a single file while expanding and merging a manifest's entries in
/// specificity order.
#[derive(Clone, Debug, Default)]
struct StagedEntry {
    kind: Option<AssetKind>,
    namespace: Option<String>,
    alias: Option<String>,
    scales: Vec<f32>,
    tags: Vec<String>,
    locale: Option<String>,
    theme: Option<String>,
    platform: Option<String>,
    title: Option<String>,
    description: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    when: BTreeMap<String, Vec<String>>,
    custom: toml::Table,
}

impl StagedEntry {
    fn from_manifest_entry(
        entry: &AssetManifestEntry,
        namespace: Option<String>,
        alias: Option<String>,
    ) -> Self {
        Self {
            kind: entry.kind,
            namespace,
            alias,
            scales: entry.scales.clone(),
            tags: entry.tags.clone(),
            locale: entry.locale.clone(),
            theme: entry.theme.clone(),
            platform: entry.platform.clone(),
            title: entry.title.clone(),
            description: entry.description.clone(),
            width: entry.width,
            height: entry.height,
            when: entry
                .when
                .iter()
                .map(|(k, v)| (k.clone(), v.0.clone()))
                .collect(),
            custom: entry.custom.clone(),
        }
    }

    /// Merges a more-specific or later entry on top of `self`.
    /// Scalar fields override when present; `tags` merge as a deduped union; `custom` key-merges.
    fn merge_from(&mut self, overlay: Self) {
        if overlay.kind.is_some() {
            self.kind = overlay.kind;
        }
        if overlay.namespace.is_some() {
            self.namespace = overlay.namespace;
        }
        if overlay.alias.is_some() {
            self.alias = overlay.alias;
        }
        if !overlay.scales.is_empty() {
            self.scales = overlay.scales;
        }
        for tag in overlay.tags {
            if !self.tags.contains(&tag) {
                self.tags.push(tag);
            }
        }
        if overlay.locale.is_some() {
            self.locale = overlay.locale;
        }
        if overlay.theme.is_some() {
            self.theme = overlay.theme;
        }
        if overlay.platform.is_some() {
            self.platform = overlay.platform;
        }
        if overlay.title.is_some() {
            self.title = overlay.title;
        }
        if overlay.description.is_some() {
            self.description = overlay.description;
        }
        if overlay.width.is_some() {
            self.width = overlay.width;
        }
        if overlay.height.is_some() {
            self.height = overlay.height;
        }
        if !overlay.when.is_empty() {
            self.when = overlay.when;
        }
        for (key, value) in overlay.custom {
            self.custom.insert(key, value);
        }
    }
}

/// Builds an [`AssetCatalog`] from manifests.
#[derive(Debug, Default)]
pub struct AssetCatalogBuilder {
    catalog: AssetCatalog,
}

impl AssetCatalogBuilder {
    /// Adds the manifest of `package` from its TOML text; a manifest that does not parse is
    /// an error, problems with its entries are diagnostics.
    pub fn add_manifest_text(
        &mut self,
        package: &str,
        text: &str,
    ) -> Result<&mut Self, AssetError> {
        let manifest =
            AssetManifest::parse(text).map_err(|reason| AssetError::InvalidManifest {
                package: package.to_owned(),
                reason,
            })?;
        Ok(self.add_manifest(package, &manifest))
    }

    /// Adds the manifest of `package` without an available file list (exact entries only; any
    /// glob patterns emit a zero-match warning).
    pub fn add_manifest(&mut self, package: &str, manifest: &AssetManifest) -> &mut Self {
        self.add_manifest_with_files(package, manifest, &[])
    }

    /// Adds the manifest of `package`, expanding glob entries (`*`, `**`, `?`, `{a,b}`) against
    /// `available_files` (respecting `exclude`, ignoring `assets.toml` and hidden files).
    ///
    /// Rules are applied in specificity order: broad globs (`**`) → more specific globs → exact
    /// entries, with later/more-specific entries overriding scalar fields and merging `tags` and
    /// `custom`.
    pub fn add_manifest_with_files(
        &mut self,
        package: &str,
        manifest: &AssetManifest,
        available_files: &[String],
    ) -> &mut Self {
        if !self.catalog.packages.iter().any(|known| known == package) {
            self.catalog.packages.push(package.to_owned());
        }

        // 1. Validate & register explicit top-level namespaces.
        for ns in &manifest.namespaces {
            match normalize_namespace(ns) {
                Ok(normalized) => {
                    self.catalog.namespaces.insert(normalized);
                }
                Err(error) => {
                    self.diagnose(
                        AssetDiagnosticSeverity::Error,
                        package,
                        Some(ns),
                        error.to_string(),
                    );
                }
            }
        }

        // 2. Clean & deduplicate available files (skip `assets.toml` and hidden files).
        let mut candidate_set = BTreeSet::new();
        for file in available_files {
            if let Ok(normalized) = normalize(file)
                && normalized != "assets.toml"
                && !is_hidden(&normalized)
            {
                candidate_set.insert(normalized);
            }
        }
        let candidates: Vec<String> = candidate_set.into_iter().collect();

        // 3. Validate entries, reject duplicate exact entries, and sort by specificity.
        struct PreparedEntry {
            specificity: GlobSpecificity,
            order: usize,
            is_glob: bool,
            normalized_path: String,
            alias: Option<String>,
            entry: AssetManifestEntry,
        }

        let mut prepared = Vec::new();
        let mut seen_exact = BTreeSet::new();

        for (order, entry) in manifest.entries().enumerate() {
            let glob_entry = is_glob(&entry.path);
            let normalized_path = if glob_entry {
                if let Err(reason) = validate_pattern(&entry.path) {
                    self.diagnose(
                        AssetDiagnosticSeverity::Error,
                        package,
                        Some(&entry.path),
                        AssetError::InvalidPath {
                            path: entry.path.clone(),
                            reason,
                        }
                        .to_string(),
                    );
                    continue;
                }
                entry.path.trim().to_owned()
            } else {
                match normalize(&entry.path) {
                    Ok(path) => path,
                    Err(error) => {
                        self.diagnose(
                            AssetDiagnosticSeverity::Error,
                            package,
                            Some(&entry.path),
                            error.to_string(),
                        );
                        continue;
                    }
                }
            };

            // Validate exclude patterns.
            let mut excludes_valid = true;
            for exclude in &entry.exclude {
                if let Err(reason) = validate_pattern(exclude) {
                    self.diagnose(
                        AssetDiagnosticSeverity::Error,
                        package,
                        Some(exclude),
                        AssetError::InvalidPath {
                            path: exclude.clone(),
                            reason,
                        }
                        .to_string(),
                    );
                    excludes_valid = false;
                }
            }
            if !excludes_valid {
                continue;
            }

            // Validate alias if present.
            let alias = match &entry.alias {
                Some(raw_alias) => match normalize(raw_alias) {
                    Ok(normalized) => Some(normalized),
                    Err(error) => {
                        self.diagnose(
                            AssetDiagnosticSeverity::Error,
                            package,
                            Some(&entry.path),
                            error.to_string(),
                        );
                        continue;
                    }
                },
                None => None,
            };

            if !glob_entry && !seen_exact.insert(normalized_path.clone()) {
                self.diagnose(
                    AssetDiagnosticSeverity::Error,
                    package,
                    Some(&normalized_path),
                    "the manifest lists it twice".to_owned(),
                );
                continue;
            }

            let specificity = GlobSpecificity::of(&normalized_path);
            prepared.push(PreparedEntry {
                specificity,
                order,
                is_glob: glob_entry,
                normalized_path,
                alias,
                entry,
            });
        }

        prepared.sort_by_key(|item| (item.specificity, item.order));

        // 4. Expand globs and merge entries in specificity order.
        let mut staged: BTreeMap<String, StagedEntry> = BTreeMap::new();
        let mut variants: Vec<(String, f32, String, StagedEntry)> = Vec::new();

        for item in prepared {
            if item.is_glob {
                let matched: Vec<&String> = candidates
                    .iter()
                    .filter(|file| {
                        glob_matches(&item.normalized_path, file)
                            && !item.entry.exclude.iter().any(|ex| glob_matches(ex, file))
                    })
                    .collect();

                if matched.is_empty() {
                    self.diagnose(
                        AssetDiagnosticSeverity::Warning,
                        package,
                        Some(&item.normalized_path),
                        "glob pattern matched no files".to_owned(),
                    );
                    continue;
                }

                for file in matched {
                    let resolved_ns = match &item.entry.namespace {
                        Some(template) => {
                            match expand_namespace_template(template, &item.normalized_path, file) {
                                Some(expanded) => match normalize_namespace(&expanded) {
                                    Ok(ns) => Some(ns),
                                    Err(error) => {
                                        self.diagnose(
                                            AssetDiagnosticSeverity::Error,
                                            package,
                                            Some(file),
                                            error.to_string(),
                                        );
                                        None
                                    }
                                },
                                None => None,
                            }
                        }
                        None => None,
                    };

                    let overlay = StagedEntry::from_manifest_entry(
                        &item.entry,
                        resolved_ns,
                        item.alias.clone(),
                    );
                    if let Some((base, scale)) = split_density(file) {
                        variants.push((base, scale, file.clone(), overlay));
                        continue;
                    }
                    staged
                        .entry(file.clone())
                        .and_modify(|existing| existing.merge_from(overlay.clone()))
                        .or_insert(overlay);
                }
            } else {
                let resolved_ns = match &item.entry.namespace {
                    Some(raw_ns) => {
                        match expand_namespace_template(
                            raw_ns,
                            &item.normalized_path,
                            &item.normalized_path,
                        ) {
                            Some(expanded) => match normalize_namespace(&expanded) {
                                Ok(ns) => Some(ns),
                                Err(error) => {
                                    self.diagnose(
                                        AssetDiagnosticSeverity::Error,
                                        package,
                                        Some(&item.normalized_path),
                                        error.to_string(),
                                    );
                                    None
                                }
                            },
                            None => None,
                        }
                    }
                    None => None,
                };
                let overlay =
                    StagedEntry::from_manifest_entry(&item.entry, resolved_ns, item.alias);
                if let Some((base, scale)) = split_density(&item.normalized_path) {
                    variants.push((base, scale, item.normalized_path, overlay));
                    continue;
                }
                staged
                    .entry(item.normalized_path)
                    .and_modify(|existing| existing.merge_from(overlay.clone()))
                    .or_insert(overlay);
            }
        }

        // 5. Commit staged entries into the catalog.
        for (path, entry) in staged {
            self.insert_staged(package, path, entry);
        }

        // 6. Fold density variants into their bases (or insert standalone if no base exists).
        let mut seen_standalone_variants = BTreeSet::new();
        for (base, scale, path, fallback_entry) in variants {
            let key = format!("@{package}/{base}");
            match self.catalog.entries.get_mut(&key) {
                Some(descriptor) => {
                    let descriptor = Arc::make_mut(descriptor);
                    if descriptor
                        .kind
                        .has_density_variants_for_path(&descriptor.path)
                    {
                        add_scale(&mut descriptor.scales, scale);
                    }
                }
                // A file named with a density suffix and no base is that file alone.
                None => {
                    if seen_standalone_variants.insert(path.clone()) {
                        self.insert_staged(package, path, fallback_entry);
                    }
                }
            }
        }

        self
    }

    /// Adds served/bundled files that were not already added by `package`'s manifest, quietly
    /// inferring their kind from their path (and folding any `@2x`/`@3x` density variants).
    ///
    /// Files matching any `exclude` pattern in `manifest` are skipped.
    pub fn add_unlisted_files(
        &mut self,
        package: &str,
        files: &[String],
        manifest: Option<&AssetManifest>,
    ) -> &mut Self {
        if !self.catalog.packages.iter().any(|known| known == package) {
            self.catalog.packages.push(package.to_owned());
        }
        let mut variants = Vec::new();
        for raw in files {
            let Ok(path) = normalize(raw) else {
                continue;
            };
            if path == "assets.toml" || is_hidden(&path) {
                continue;
            }
            if manifest.is_some_and(|m| m.is_excluded(&path)) {
                continue;
            }
            if let Some((base, scale)) = split_density(&path) {
                variants.push((base, scale, path));
                continue;
            }
            let key = format!("@{package}/{path}");
            if self.catalog.entries.contains_key(&key) {
                continue;
            }
            let kind = AssetKind::infer_from_path(&path);
            let staged = StagedEntry {
                kind: Some(kind),
                ..StagedEntry::default()
            };
            self.insert_staged(package, path, staged);
        }
        for (base, scale, path) in variants {
            let key = format!("@{package}/{base}");
            match self.catalog.entries.get_mut(&key) {
                Some(descriptor) => {
                    let descriptor = Arc::make_mut(descriptor);
                    if descriptor
                        .kind
                        .has_density_variants_for_path(&descriptor.path)
                    {
                        add_scale(&mut descriptor.scales, scale);
                    }
                }
                None => {
                    let standalone_key = format!("@{package}/{path}");
                    if !self.catalog.entries.contains_key(&standalone_key) {
                        let kind = AssetKind::infer_from_path(&path);
                        let staged = StagedEntry {
                            kind: Some(kind),
                            ..StagedEntry::default()
                        };
                        self.insert_staged(package, path, staged);
                    }
                }
            }
        }
        self
    }

    pub fn build(self) -> AssetCatalog {
        self.catalog
    }

    fn insert_staged(&mut self, package: &str, path: String, entry: StagedEntry) {
        let key = format!("@{package}/{path}");
        if self.catalog.entries.contains_key(&key) {
            self.diagnose(
                AssetDiagnosticSeverity::Error,
                package,
                Some(&path),
                "the manifest lists it twice".to_owned(),
            );
            return;
        }
        let kind = entry
            .kind
            .unwrap_or_else(|| AssetKind::infer_from_path(&path));
        // `kind = "unknown"`, set on purpose (as for the files the platform lists), is no
        // problem.
        if kind == AssetKind::Unknown && entry.kind.is_none() {
            self.diagnose(
                AssetDiagnosticSeverity::Warning,
                package,
                Some(&path),
                "its kind is not known from its name; set `kind`".to_owned(),
            );
        }
        if let Err(reason) = kind.validate_extension(&path) {
            self.diagnose(AssetDiagnosticSeverity::Error, package, Some(&path), reason);
        }
        let mut scales = Vec::new();
        for scale in &entry.scales {
            if *scale == 1.0 {
                continue;
            }
            if !DENSITY_SCALES.contains(scale) {
                self.diagnose(
                    AssetDiagnosticSeverity::Error,
                    package,
                    Some(&path),
                    format!("{scale} is not a density NSH loads (1.5, 2, 3 or 4)"),
                );
                continue;
            }
            add_scale(&mut scales, *scale);
        }
        if !scales.is_empty() && !kind.has_density_variants_for_path(&path) {
            self.diagnose(
                AssetDiagnosticSeverity::Warning,
                package,
                Some(&path),
                format!(
                    "{kind:?} assets at `{path}` have no density variants; its scales are ignored"
                ),
            );
            scales.clear();
        }
        if let Some(ns) = &entry.namespace {
            self.catalog.namespaces.insert(ns.clone());
        }
        let descriptor = AssetDescriptor {
            package: package.to_owned(),
            mime_type: AssetKind::mime_type_for_path(&path).map(str::to_owned),
            path,
            kind,
            namespace: entry.namespace,
            alias: entry.alias,
            scales,
            tags: entry.tags,
            locale: entry.locale,
            theme: entry.theme,
            platform: entry.platform,
            title: entry.title,
            description: entry.description,
            width: entry.width,
            height: entry.height,
            when: entry.when,
            custom: serde_json::to_value(entry.custom).unwrap_or(serde_json::Value::Null),
        };
        self.catalog.entries.insert(key, Arc::new(descriptor));
    }

    fn diagnose(
        &mut self,
        severity: AssetDiagnosticSeverity,
        package: &str,
        path: Option<&str>,
        message: String,
    ) {
        self.catalog.diagnostics.push(AssetDiagnostic {
            severity,
            package: package.to_owned(),
            path: path.map(str::to_owned),
            message,
        });
    }

    /// Records a diagnostic of `package`'s manifest (one that could not be read, say).
    pub fn diagnose_package(
        &mut self,
        severity: AssetDiagnosticSeverity,
        package: &str,
        message: String,
    ) {
        if !self.catalog.packages.iter().any(|known| known == package) {
            self.catalog.packages.push(package.to_owned());
        }
        self.diagnose(severity, package, None, message);
    }
}

/// Adds `scale` to `scales`, with the file itself (1) first, ascending, once.
fn add_scale(scales: &mut Vec<f32>, scale: f32) {
    if scales.is_empty() {
        scales.push(1.0);
    }
    if !scales.contains(&scale) {
        scales.push(scale);
        scales.sort_by(f32::total_cmp);
    }
}
