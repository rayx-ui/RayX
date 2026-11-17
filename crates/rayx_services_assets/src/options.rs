use serde::{Deserialize, Serialize};

/// The configuration section [`AssetOptions`] bind from.
pub const ASSET_OPTIONS_SECTION: &str = "RayX:Services:Assets";

/// The default byte budget of loaded assets kept in memory: 16 MiB.
pub const DEFAULT_ASSET_BYTE_CACHE_BUDGET: u64 = 16 * 1024 * 1024;

/// How manifest problems weigh.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum AssetValidationPolicy {
    /// A missing or invalid manifest, or an error in one, fails startup.
    Strict,
    /// Problems are diagnostics; the assets that are fine load.
    #[default]
    WarnOnly,
    /// Problems are dropped.
    Disabled,
}

/// Where the asset service finds its manifests, and how it keeps what it loads.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AssetOptions {
    /// The packages (crates) whose `assets/` directories hold the app's assets, the app's own
    /// first: a path without a package is looked up in this order.
    pub packages: Vec<String>,
    /// The manifest's file name in each package's `assets/` directory.
    pub manifest: String,
    pub validation: AssetValidationPolicy,
    /// How many bytes of loaded assets stay in memory.
    pub byte_cache_budget: u64,
}

impl Default for AssetOptions {
    fn default() -> Self {
        Self {
            packages: Vec::new(),
            manifest: "assets.toml".to_owned(),
            validation: AssetValidationPolicy::WarnOnly,
            byte_cache_budget: DEFAULT_ASSET_BYTE_CACHE_BUDGET,
        }
    }
}

impl AssetOptions {
    /// Adds `package` to the packages whose manifests are read.
    pub fn package(&mut self, package: impl Into<String>) -> &mut Self {
        let package = package.into();
        if !self.packages.contains(&package) {
            self.packages.push(package);
        }
        self
    }

    /// The URL of `package`'s manifest.
    pub fn manifest_uri(&self, package: &str) -> String {
        format!("asset:/@{package}/{}", self.manifest)
    }
}
