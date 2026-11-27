//! The asset service for TSX.

use std::sync::Arc;

use nsh_core::prelude::*;
use rayx_app::services::assets::AssetService as Assets;
use rayx_app_nsh::ts::ServiceClass;

/// The app's assets, for TSX: `services.getRequiredService(AssetService).url("images/logo.png")`,
/// the URL of an asset of the catalog, for an `<Image source>` or a `fetch`.
#[class]
pub struct AssetService {
    service: Arc<dyn Assets>,
}

impl ServiceClass for AssetService {
    type Service = dyn Assets;

    fn wrap(service: Arc<dyn Assets>) -> Self {
        Self { service }
    }
}

#[class]
impl AssetService {
    /// The contract this class is the token of (TS types only: `null` at run time).
    #[getter]
    #[ts("(): AssetServiceContract | null")]
    pub fn contract(&self) -> Option<bool> {
        None
    }

    /// The URL of the asset at `path`.
    pub fn url(&self, path: String) -> Result<String, String> {
        self.service.uri(&path).map_err(|error| error.to_string())
    }

    /// The URL of the asset at `path` for the pixel ratio `scale` (its `@2x`, `@3x` variant).
    pub fn url_for_scale(&self, path: String, scale: f64) -> Result<String, String> {
        self.service
            .uri_for_scale(&path, scale as f32)
            .map_err(|error| error.to_string())
    }

    /// Every virtual namespace declared in the loaded catalog, sorted.
    pub fn namespaces(&self) -> Result<Vec<String>, String> {
        let catalog = self
            .service
            .loaded_catalog()
            .map_err(|error| error.to_string())?;
        let Some(catalog) = catalog.as_ref() else {
            return Err(rayx_app::services::assets::AssetError::NotLoaded.to_string());
        };
        Ok(catalog.namespaces().iter().cloned().collect())
    }

    /// The loaded asset catalog serialized as JSON.
    pub fn catalog_json(&self) -> Result<String, String> {
        let catalog = self
            .service
            .loaded_catalog()
            .map_err(|error| error.to_string())?;
        let Some(catalog) = catalog.as_ref() else {
            return Err(rayx_app::services::assets::AssetError::NotLoaded.to_string());
        };
        serde_json::to_string(catalog).map_err(|error| error.to_string())
    }
}
