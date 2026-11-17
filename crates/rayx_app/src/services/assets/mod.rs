//! Assets: the app's files (images, fonts, messages, data) listed by its packages' manifests in
//! an [`AssetCatalog`], published by an [`AssetService`] implementation, and read from it by
//! logical path (`@<package>/<path>`), as bytes or text.
//!
//! RayX's implementation is `rayx_services_assets`' (`add_assets(..)`), which reads the files
//! with NSH; an app may register its own `dyn AssetService`, in Rust or in TS. Looking assets up
//! in the catalog ([`AssetCatalog::get`], [`AssetCatalog::query`], [`AssetDescriptor::uri`]) is
//! synchronous for any implementation, and so are the helpers on `dyn AssetService`.

mod catalog;
mod error;
mod kind;
mod manifest;
mod path;

pub use catalog::{
    AssetCatalog, AssetCatalogBuilder, AssetDescriptor, AssetDiagnostic, AssetDiagnosticSeverity,
    AssetQuery,
};
pub use error::{AssetError, Result};
pub use kind::AssetKind;
pub use manifest::{AssetManifest, AssetManifestEntry};
pub use path::{DENSITY_SCALES, normalize};

use serde::de::DeserializeOwned;

use crate::published::StateCell;

/// The asset service: the app's catalog, published once loaded (`None` until then), and the
/// files read from it.
#[rayx_app_macros::contract(ts = "AssetServiceContract")]
pub trait AssetService {
    /// The catalog, once loaded (`None` until then), with what reading the manifests found
    /// wrong.
    #[state(ts = "AssetCatalog | null")]
    fn catalog(&self) -> &StateCell<Option<AssetCatalog>>;

    /// Loads the catalog from the packages' manifests (the app's start does), publishing it.
    async fn load_catalog(&self) -> Result<(), AssetError>;

    /// The bytes of the asset at `path`.
    #[ts("(path: string): number[]")]
    async fn load_bytes(&self, path: String) -> Result<Vec<u8>, AssetError>;

    /// The UTF-8 text of the asset at `path`.
    async fn load_text(&self, path: String) -> Result<String, AssetError>;

    /// Forgets the bytes read so far (the next reads read the files again).
    fn clear_cache(&self);
}

impl dyn AssetService {
    /// The catalog now, or [`AssetError::NotLoaded`].
    pub fn loaded_catalog(&self) -> Result<std::sync::Arc<Option<AssetCatalog>>> {
        let catalog = self.catalog().get();
        if catalog.is_some() {
            Ok(catalog)
        } else {
            Err(AssetError::NotLoaded)
        }
    }

    /// The asset at `path` (a logical path, or a path in the first package that has it).
    pub fn descriptor(&self, path: &str) -> Result<AssetDescriptor> {
        let catalog = self.loaded_catalog()?;
        let catalog = catalog.as_ref().as_ref().ok_or(AssetError::NotLoaded)?;
        catalog
            .get(path)
            .map(|descriptor| (**descriptor).clone())
            .ok_or_else(|| AssetError::NotFound(path.to_owned()))
    }

    /// The assets `query` matches.
    pub fn query(&self, query: &AssetQuery) -> Result<Vec<AssetDescriptor>> {
        let catalog = self.loaded_catalog()?;
        let catalog = catalog.as_ref().as_ref().ok_or(AssetError::NotLoaded)?;
        Ok(catalog
            .query(query)
            .into_iter()
            .map(|descriptor| (*descriptor).clone())
            .collect())
    }

    /// What reading the manifests found wrong (nothing before the catalog loads).
    pub fn diagnostics(&self) -> Vec<AssetDiagnostic> {
        self.catalog()
            .get()
            .as_ref()
            .as_ref()
            .map(|catalog| catalog.diagnostics().to_vec())
            .unwrap_or_default()
    }

    /// The `asset:/` URL of the asset at `path` (an `<Image>`'s source).
    pub fn uri(&self, path: &str) -> Result<String> {
        self.descriptor(path).map(|descriptor| descriptor.uri())
    }

    /// The URL of the asset's variant for `scale` (its `@2x` file when it has one).
    pub fn uri_for_scale(&self, path: &str, scale: f32) -> Result<String> {
        self.descriptor(path)
            .map(|descriptor| descriptor.uri_for_scale(scale))
    }

    /// The asset at `path` as JSON, decoded into `T`.
    pub async fn load_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let text = self.load_text(path.to_owned()).await?;
        serde_json::from_str(&text).map_err(|error| AssetError::Decode {
            path: path.to_owned(),
            expected: "JSON",
            reason: error.to_string(),
        })
    }
}
