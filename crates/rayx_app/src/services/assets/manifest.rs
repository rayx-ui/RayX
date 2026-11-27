//! The asset manifest, `assets.toml` by default: the files of a crate's `assets/` directory the
//! app looks up, with what they are. Neutral: no platform, framework or tool in it.
//!
//! ```toml
//! # Optional explicit virtual directories (persists empty namespaces).
//! namespaces = ["icons/navigation", "illustrations/empty"]
//!
//! # Shorthand: literal paths or globs with nothing more to say than their path.
//! files = ["fonts/Inter.woff2", "locales/*.json"]
//!
//! [[asset]]
//! path = "icons/**/*.svg"
//! exclude = ["icons/draft/**"]
//! namespace = "icons/{subdir}"
//! kind = "icon"
//! tags = ["ui"]
//!
//! [[asset]]
//! path = "images/logo.png"
//! scales = [2, 3]          # logo@2x.png and logo@3x.png ship too
//! tags = ["brand"]
//! theme = "dark"
//! when = { platform = ["ios", "macos"], brand = "acme" }
//! alias = "brand/logo.svg"
//! ```

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};

use super::AssetKind;

/// A `when` predicate value in `assets.toml`: deserializes from either a single string
/// (`brand = "acme"`) or a list of strings (`platform = ["ios", "macos"]`), normalized to
/// `Vec<String>`.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct WhenValue(pub Vec<String>);

impl WhenValue {
    /// The predicate values as a slice.
    pub fn as_slice(&self) -> &[String] {
        &self.0
    }

    /// Consumes `self`, returning the normalized list of values.
    pub fn into_vec(self) -> Vec<String> {
        self.0
    }
}

impl From<String> for WhenValue {
    fn from(value: String) -> Self {
        Self(vec![value])
    }
}

impl From<Vec<String>> for WhenValue {
    fn from(values: Vec<String>) -> Self {
        Self(values)
    }
}

impl<'de> Deserialize<'de> for WhenValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum OneOrMany {
            One(String),
            Many(Vec<String>),
        }
        match OneOrMany::deserialize(deserializer)? {
            OneOrMany::One(value) => Ok(Self(vec![value])),
            OneOrMany::Many(values) => Ok(Self(values)),
        }
    }
}

/// One manifest, as written.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetManifest {
    /// Optional explicit virtual namespaces (persists empty namespaces).
    #[serde(default)]
    pub namespaces: Vec<String>,
    /// Files (or glob patterns) with nothing more to say than their path.
    #[serde(default)]
    pub files: Vec<String>,
    /// Files (or glob patterns) with metadata.
    #[serde(default, rename = "asset")]
    pub assets: Vec<AssetManifestEntry>,
}

/// A file or glob rule of the manifest.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetManifestEntry {
    /// Relative to the crate's `assets/` directory; may be a literal path or a glob
    /// (`*`, `**`, `?`, `{a,b}`).
    pub path: String,
    /// Optional glob patterns excluded from this entry's matches.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Optional virtual namespace (`/`-separated; supports `{subdir}` capture on globs).
    pub namespace: Option<String>,
    /// What it is; from the file name when left out.
    pub kind: Option<AssetKind>,
    /// Optional logical path override for mutually exclusive `when` entries.
    pub alias: Option<String>,
    /// The densities its `@Nx` variants ship in, besides the file itself (1.5, 2, 3, 4).
    #[serde(default)]
    pub scales: Vec<f32>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub locale: Option<String>,
    pub theme: Option<String>,
    pub platform: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    /// Its size in points, for images.
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Build-time inclusion predicate (`AND` across keys, `OR` within a list, `!val` negation).
    #[serde(default)]
    pub when: BTreeMap<String, WhenValue>,
    /// Anything else, for the app.
    #[serde(default)]
    pub custom: toml::Table,
}

impl AssetManifest {
    /// Reads a manifest's TOML.
    pub fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|error| error.to_string())
    }

    /// Every entry, the shorthand files included.
    pub fn entries(&self) -> impl Iterator<Item = AssetManifestEntry> + '_ {
        self.files
            .iter()
            .map(|path| AssetManifestEntry {
                path: path.clone(),
                ..AssetManifestEntry::default()
            })
            .chain(self.assets.iter().cloned())
    }

    /// Whether `path` matches any `exclude` glob declared on any `[[asset]]` entry of this
    /// manifest.
    pub fn is_excluded(&self, path: &str) -> bool {
        self.assets.iter().any(|entry| {
            entry
                .exclude
                .iter()
                .any(|pattern| super::glob::glob_matches(pattern, path))
        })
    }
}
