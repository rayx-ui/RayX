use std::sync::{Arc, Mutex, PoisonError, RwLock};

use futures::FutureExt as _;
use futures::future::LocalBoxFuture;
use rayx_core::di::{
    AppService, AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupInitializationState, StartupServiceError,
    StartupServiceType,
};
use serde::de::DeserializeOwned;

use super::catalog::AssetDiagnosticSeverity;
use super::{
    AssetByteCache, AssetCatalog, AssetDescriptor, AssetDiagnostic, AssetError, AssetKind,
    AssetManifest, AssetOptions, AssetQuery, AssetReader, AssetValidationPolicy, BundledAssets,
    Result,
};

/// The app's assets: a catalog of the files the app's packages ship ([`BundledAssets`]) and
/// of their manifests, read when the app starts, and their bytes read through the platform's
/// [`AssetReader`] and kept within a byte budget. Assets do not change while the app runs (no
/// hot reload: a session restart reads them again).
#[derive(Clone, AppService)]
#[app_service(startup)]
pub struct AssetService {
    options: AssetOptions,
    reader: Option<Arc<dyn AssetReader>>,
    bundled: Option<Arc<BundledAssets>>,
    catalog: Arc<RwLock<Option<Arc<AssetCatalog>>>>,
    cache: Arc<Mutex<AssetByteCache>>,
    initialization: StartupInitializationState,
}

impl AssetService {
    /// A service reading `options`' manifests, and its assets, with `reader`.
    pub fn new(options: AssetOptions, reader: Option<Arc<dyn AssetReader>>) -> Self {
        let cache = AssetByteCache::new(options.byte_cache_budget);
        Self {
            options,
            reader,
            bundled: None,
            catalog: Arc::default(),
            cache: Arc::new(Mutex::new(cache)),
            initialization: StartupInitializationState::default(),
        }
    }

    /// A service over a catalog built already (its startup reads no manifest).
    pub fn with_catalog(catalog: AssetCatalog, reader: Option<Arc<dyn AssetReader>>) -> Self {
        let service = Self::new(AssetOptions::default(), reader);
        *service
            .catalog
            .write()
            .unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(catalog));
        service
    }

    /// The same service, cataloging the files `bundled` lists besides its manifests.
    pub fn with_bundled(mut self, bundled: Option<Arc<BundledAssets>>) -> Self {
        self.bundled = bundled;
        self
    }

    pub fn options(&self) -> &AssetOptions {
        &self.options
    }

    /// Reads the catalog (the startup does): the files of the options' packages and of the
    /// packages [`BundledAssets`] lists (after them), with each package's manifest. A package
    /// whose files are listed has its manifest read only when it ships one; one whose files are
    /// not needs its manifest, which lists them.
    pub async fn load_catalog(&self) -> Result<Arc<AssetCatalog>> {
        let mut builder = AssetCatalog::builder();
        let strict = self.options.validation == AssetValidationPolicy::Strict;
        let mut packages = self.options.packages.clone();
        let bundled = self.bundled.as_deref();
        for package in bundled.map(BundledAssets::packages).unwrap_or_default() {
            if !packages.contains(&package) {
                packages.push(package);
            }
        }
        for package in &packages {
            let files = bundled
                .map(|bundled| bundled.files(package))
                .unwrap_or_default();
            let ships_manifest = files.contains(&self.options.manifest);
            let manifest = if files.is_empty() || ships_manifest {
                self.read_manifest(package).await
            } else {
                Ok(AssetManifest::default())
            };
            let added = manifest.map(|mut manifest| {
                let listed: Vec<String> = manifest.entries().map(|entry| entry.path).collect();
                for path in files {
                    if path != self.options.manifest && !listed.contains(&path) {
                        // A file only the platform lists: its kind from its name, quietly.
                        manifest.assets.push(super::AssetManifestEntry {
                            kind: Some(AssetKind::infer_from_path(&path)),
                            path,
                            ..super::AssetManifestEntry::default()
                        });
                    }
                }
                builder.add_manifest(package, &manifest);
            });
            if let Err(error) = added {
                if strict {
                    return Err(error);
                }
                builder.diagnose_package(
                    AssetDiagnosticSeverity::Error,
                    package,
                    error.to_string(),
                );
            }
        }
        let mut catalog = builder.build();
        if strict && catalog.has_errors() {
            let first = catalog
                .diagnostics()
                .iter()
                .find(|diagnostic| diagnostic.severity == AssetDiagnosticSeverity::Error);
            return Err(AssetError::InvalidManifest {
                package: first
                    .map(|diagnostic| diagnostic.package.clone())
                    .unwrap_or_default(),
                reason: first
                    .map(|diagnostic| diagnostic.message.clone())
                    .unwrap_or_default(),
            });
        }
        if self.options.validation == AssetValidationPolicy::Disabled {
            catalog = catalog.without_diagnostics();
        }
        let catalog = Arc::new(catalog);
        *self.catalog.write().unwrap_or_else(PoisonError::into_inner) = Some(Arc::clone(&catalog));
        Ok(catalog)
    }

    /// `package`'s manifest, read and parsed.
    async fn read_manifest(&self, package: &str) -> Result<AssetManifest> {
        let uri = self.options.manifest_uri(package);
        let bytes = self.read(&uri).await?;
        let text = String::from_utf8(bytes).map_err(|error| AssetError::Decode {
            path: uri.clone(),
            expected: "UTF-8 text",
            reason: error.to_string(),
        })?;
        AssetManifest::parse(&text).map_err(|reason| AssetError::InvalidManifest {
            package: package.to_owned(),
            reason,
        })
    }

    /// The catalog, once loaded.
    pub fn catalog(&self) -> Result<Arc<AssetCatalog>> {
        self.catalog
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or(AssetError::NotLoaded)
    }

    /// The asset at `path` (`@<package>/<path>`, or a path in the first package that has it).
    pub fn descriptor(&self, path: &str) -> Result<Arc<AssetDescriptor>> {
        self.catalog()?
            .get(path)
            .cloned()
            .ok_or_else(|| AssetError::NotFound(path.to_owned()))
    }

    /// The assets `query` matches.
    pub fn query(&self, query: &AssetQuery) -> Result<Vec<Arc<AssetDescriptor>>> {
        Ok(self.catalog()?.query(query))
    }

    /// What reading the manifests found wrong.
    pub fn diagnostics(&self) -> Vec<AssetDiagnostic> {
        self.catalog()
            .map(|catalog| catalog.diagnostics().to_vec())
            .unwrap_or_default()
    }

    /// The `asset:/` URL of the asset at `path`, for NSH's elements (an image loads the
    /// density that fits the screen from it).
    pub fn uri(&self, path: &str) -> Result<String> {
        Ok(self.descriptor(path)?.uri())
    }

    /// The URL of the asset's variant that fits `scale` device pixels per point.
    pub fn uri_for_scale(&self, path: &str, scale: f32) -> Result<String> {
        Ok(self.descriptor(path)?.uri_for_scale(scale))
    }

    /// The bytes of the asset at `path`, from memory when they were read recently.
    pub async fn load_bytes(&self, path: &str) -> Result<Arc<[u8]>> {
        let uri = self.uri(path)?;
        if let Some(bytes) = self.lock_cache().get(&uri) {
            return Ok(bytes);
        }
        let bytes: Arc<[u8]> = self.read(&uri).await?.into();
        self.lock_cache().insert(uri, Arc::clone(&bytes));
        Ok(bytes)
    }

    /// The asset at `path` as UTF-8 text.
    pub async fn load_text(&self, path: &str) -> Result<String> {
        let bytes = self.load_bytes(path).await?;
        String::from_utf8(bytes.to_vec()).map_err(|error| AssetError::Decode {
            path: path.to_owned(),
            expected: "UTF-8 text",
            reason: error.to_string(),
        })
    }

    /// The asset at `path` read as JSON into `T`.
    pub async fn load_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let bytes = self.load_bytes(path).await?;
        serde_json::from_slice(&bytes).map_err(|error| AssetError::Decode {
            path: path.to_owned(),
            expected: "JSON of the expected shape",
            reason: error.to_string(),
        })
    }

    /// Drops the loaded bytes kept in memory.
    pub fn clear_cache(&self) {
        self.lock_cache().clear();
    }

    async fn read(&self, uri: &str) -> Result<Vec<u8>> {
        let reader = self.reader.as_ref().ok_or_else(|| AssetError::Read {
            uri: uri.to_owned(),
            reason: "no AssetReader is registered (rayx_app registers NSH's)".to_owned(),
        })?;
        reader.read(uri).await.map_err(|reason| AssetError::Read {
            uri: uri.to_owned(),
            reason,
        })
    }

    fn lock_cache(&self) -> std::sync::MutexGuard<'_, AssetByteCache> {
        self.cache.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl AsyncInitializable for AssetService {
    fn is_initialized(&self) -> bool {
        self.initialization.is_initialized()
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let service = self.clone();
        async move {
            let loaded = service.catalog().is_ok();
            if !loaded {
                service
                    .load_catalog()
                    .await
                    .map_err(|error| StartupServiceError::new(error.to_string()))?;
            }
            service.initialization.mark_initialized();
            Ok(())
        }
        .boxed_local()
    }
}

impl StartupInitializable for AssetService {
    fn startup_service_type(&self) -> StartupServiceType {
        StartupServiceType::of::<Self>()
    }
}
