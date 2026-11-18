//! The asset manifest, `assets.toml` by default: the files of a crate's `assets/` directory the
//! app looks up, with what they are. Neutral: no platform, framework or tool in it.
//!
//! ```toml
//! # Shorthand: files with nothing more to say than their path.
//! files = ["fonts/Inter.woff2", "locales/en.json"]
//!
//! [[asset]]
//! path = "images/logo.png"
//! scales = [2, 3]          # logo@2x.png and logo@3x.png ship too
//! tags = ["brand"]
//! theme = "dark"
//!
//! [[asset]]
//! path = "images/hero@2x.png"   # a variant listed on its own folds into `hero.png`
//! ```

use serde::Deserialize;

use super::AssetKind;

/// One manifest, as written.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetManifest {
    /// Files with nothing more to say than their path.
    #[serde(default)]
    pub files: Vec<String>,
    /// Files with metadata.
    #[serde(default, rename = "asset")]
    pub assets: Vec<AssetManifestEntry>,
}

/// A file of the manifest.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetManifestEntry {
    /// Relative to the crate's `assets/` directory.
    pub path: String,
    /// What it is; from the file name when left out.
    pub kind: Option<AssetKind>,
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
}
