//! The asset service for TSX.

use std::sync::Arc;

use nsh_core::prelude::*;
use rayx_runtime::services::assets::AssetService as Assets;

use crate::di::provider_ts::ServiceClass;

/// The app's assets, for TSX: `services.getRequiredService(AssetService).url("images/logo.png")`,
/// the URL of an asset of the catalog, for an `<Image source>` or a `fetch`.
#[class]
pub struct AssetService {
    service: Arc<Assets>,
}

impl ServiceClass for AssetService {
    type Service = Assets;

    fn wrap(service: Arc<Assets>) -> Self {
        Self { service }
    }
}

#[class]
impl AssetService {
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
}
