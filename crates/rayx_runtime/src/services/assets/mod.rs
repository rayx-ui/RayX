//! Assets: the files the app's packages ship in their `assets/` directories, listed in a
//! neutral manifest (`assets.toml` by default) with what they are, density variants
//! (`@2x`, `@3x`) and metadata. The [`AssetService`] catalogs them when the app starts, gives
//! their `asset:/` URLs for NSH's elements, and loads their bytes through the platform's
//! [`AssetReader`] within a byte budget.

mod app_ext;
mod cache;
mod catalog;
mod error;
mod kind;
mod manifest;
mod options;
mod path;
mod reader;
mod service;

pub use app_ext::AssetAppBuilderExt;
pub use cache::AssetByteCache;
pub use catalog::{
    AssetCatalog, AssetCatalogBuilder, AssetDescriptor, AssetDiagnostic, AssetDiagnosticSeverity,
    AssetQuery,
};
pub use error::{AssetError, Result};
pub use kind::AssetKind;
pub use manifest::{AssetManifest, AssetManifestEntry};
pub use options::{
    ASSET_OPTIONS_SECTION, AssetOptions, AssetValidationPolicy, DEFAULT_ASSET_BYTE_CACHE_BUDGET,
};
pub use path::DENSITY_SCALES;
pub use reader::{AssetReader, MemoryAssetReader};
pub use service::AssetService;
