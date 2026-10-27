use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::path::{DENSITY_SCALES, normalize, split_density, variant_path};
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
    }
}

fn option_matches(wanted: &Option<String>, actual: &Option<String>) -> bool {
    wanted
        .as_ref()
        .is_none_or(|wanted| actual.as_deref() == Some(wanted.as_str()))
}

/// The app's assets, from the manifests of its packages: looked up by logical path
/// (`@<package>/<path>`), by path in the first package that has it, or by query.
#[derive(Clone, Debug, Default)]
pub struct AssetCatalog {
    packages: Vec<String>,
    entries: BTreeMap<String, Arc<AssetDescriptor>>,
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
    /// package (in manifest order) that has it.
    pub fn get(&self, path: &str) -> Option<&Arc<AssetDescriptor>> {
        if path.starts_with('@') {
            return self.entries.get(path);
        }
        let path = normalize(path).ok()?;
        self.packages
            .iter()
            .find_map(|package| self.entries.get(&format!("@{package}/{path}")))
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

    pub(super) fn without_diagnostics(mut self) -> Self {
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

    /// Adds the manifest of `package`.
    pub fn add_manifest(&mut self, package: &str, manifest: &AssetManifest) -> &mut Self {
        if !self.catalog.packages.iter().any(|known| known == package) {
            self.catalog.packages.push(package.to_owned());
        }
        let mut variants = Vec::new();
        for entry in manifest.entries() {
            let path = match normalize(&entry.path) {
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
            };
            if let Some((base, scale)) = split_density(&path) {
                variants.push((base, scale, path));
                continue;
            }
            self.insert(package, path, entry);
        }
        for (base, scale, path) in variants {
            let key = format!("@{package}/{base}");
            match self.catalog.entries.get_mut(&key) {
                Some(descriptor) => {
                    let descriptor = Arc::make_mut(descriptor);
                    add_scale(&mut descriptor.scales, scale);
                }
                // A file named with a density suffix and no base is that file alone.
                None => self.insert(
                    package,
                    path.clone(),
                    AssetManifestEntry {
                        path,
                        ..AssetManifestEntry::default()
                    },
                ),
            }
        }
        self
    }

    pub fn build(self) -> AssetCatalog {
        self.catalog
    }

    fn insert(&mut self, package: &str, path: String, entry: AssetManifestEntry) {
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
        if kind == AssetKind::Unknown {
            self.diagnose(
                AssetDiagnosticSeverity::Warning,
                package,
                Some(&path),
                "its kind is not known from its name; set `kind`".to_owned(),
            );
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
        if !scales.is_empty() && !kind.has_density_variants() {
            self.diagnose(
                AssetDiagnosticSeverity::Warning,
                package,
                Some(&path),
                format!("{kind:?} assets have no density variants; its scales are ignored"),
            );
            scales.clear();
        }
        let descriptor = AssetDescriptor {
            package: package.to_owned(),
            mime_type: AssetKind::mime_type_for_path(&path).map(str::to_owned),
            path,
            kind,
            scales,
            tags: entry.tags,
            locale: entry.locale,
            theme: entry.theme,
            platform: entry.platform,
            title: entry.title,
            description: entry.description,
            width: entry.width,
            height: entry.height,
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

    pub(super) fn diagnose_package(
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
