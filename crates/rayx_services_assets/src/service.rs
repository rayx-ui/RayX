//! RayX's asset service: the catalog read from the packages' manifests (and the files the
//! platform lists), published once loaded, and the files read through an [`AssetReader`] with a
//! byte cache.

use std::sync::{Arc, Mutex, PoisonError};

use futures::FutureExt as _;
use futures::future::{BoxFuture, LocalBoxFuture};
use rayx_app::StateCell;
use rayx_app::di::{
    AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupInitializationState, StartupServiceError,
    StartupServiceType,
};
use rayx_app::services::assets::{
    AssetCatalog, AssetDiagnosticSeverity, AssetError, AssetManifest, AssetService, Result,
};

use crate::reader::AssetReader;
use crate::{AssetByteCache, AssetOptions, AssetValidationPolicy, BundledAssets};

/// RayX's asset service over a reader.
pub(crate) struct RayxAssets {
    options: AssetOptions,
    reader: Arc<dyn AssetReader>,
    bundled: Option<Arc<BundledAssets>>,
    catalog: StateCell<Option<AssetCatalog>>,
    cache: Mutex<AssetByteCache>,
    initialization: StartupInitializationState,
}

impl RayxAssets {
    pub(crate) fn new(
        options: AssetOptions,
        reader: Arc<dyn AssetReader>,
        bundled: Option<Arc<BundledAssets>>,
    ) -> Arc<Self> {
        let cache = AssetByteCache::new(options.byte_cache_budget);
        Arc::new(Self {
            options,
            reader,
            bundled,
            catalog: StateCell::new(None),
            cache: Mutex::new(cache),
            initialization: StartupInitializationState::default(),
        })
    }

    /// A service over `catalog`, loaded already.
    pub(crate) fn with_catalog(catalog: AssetCatalog, reader: Arc<dyn AssetReader>) -> Arc<Self> {
        let service = Self::new(AssetOptions::default(), reader, None);
        service.catalog.publish(Some(catalog));
        service
    }

    /// Reads the catalog: the files of the options' packages, then of the packages
    /// [`BundledAssets`] lists, then of the packages whose files the app's UI sets ship (the
    /// reader's [`served_files`](AssetReader::served_files): a library's set, served by the
    /// app's, ships its own), each with its manifest. A package whose files are listed has its
    /// manifest read only when it ships one; one whose files are not needs its manifest, which
    /// lists them.
    async fn read_catalog(&self) -> Result<AssetCatalog> {
        let mut builder = AssetCatalog::builder();
        let strict = self.options.validation == AssetValidationPolicy::Strict;
        let listed = self.listed_files().await;
        let mut packages = self.options.packages.clone();
        for package in listed.packages() {
            if !packages.contains(&package) {
                packages.push(package);
            }
        }
        for package in &packages {
            let files = listed.files(package);
            let ships_manifest = files.contains(&self.options.manifest);
            let manifest = if files.is_empty() || ships_manifest {
                self.read_manifest(package).await
            } else {
                Ok(AssetManifest::default())
            };
            let added = manifest.map(|manifest| {
                let available: Vec<String> = files
                    .iter()
                    .filter(|path| *path != &self.options.manifest)
                    .cloned()
                    .collect();
                builder.add_manifest_with_files(package, &manifest, &available);
                builder.add_unlisted_files(package, &available, Some(&manifest));
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
        Ok(catalog)
    }

    /// The files the platform lists: those the app added ([`BundledAssets`]), then those its UI
    /// sets ship, each once.
    async fn listed_files(&self) -> BundledAssets {
        let listed = BundledAssets::new();
        if let Some(bundled) = self.bundled.as_deref() {
            for package in bundled.packages() {
                for path in bundled.files(&package) {
                    listed.add(&format!("asset:/@{package}/{path}"));
                }
            }
        }
        for uri in self.reader.served_files().await {
            listed.add(&uri);
        }
        listed
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

    async fn bytes(&self, path: &str) -> Result<Arc<[u8]>> {
        let uri = {
            let catalog = self.catalog.get();
            let catalog = catalog.as_ref().as_ref().ok_or(AssetError::NotLoaded)?;
            catalog
                .get(path)
                .map(|descriptor| descriptor.uri())
                .ok_or_else(|| AssetError::NotFound(path.to_owned()))?
        };
        if let Some(bytes) = self.lock_cache().get(&uri) {
            return Ok(bytes);
        }
        let bytes: Arc<[u8]> = self.read(&uri).await?.into();
        self.lock_cache().insert(uri, Arc::clone(&bytes));
        Ok(bytes)
    }

    async fn read(&self, uri: &str) -> Result<Vec<u8>> {
        self.reader
            .read(uri)
            .await
            .map_err(|reason| AssetError::Read {
                uri: uri.to_owned(),
                reason,
            })
    }

    fn lock_cache(&self) -> std::sync::MutexGuard<'_, AssetByteCache> {
        self.cache.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl AssetService for RayxAssets {
    fn catalog(&self) -> &StateCell<Option<AssetCatalog>> {
        &self.catalog
    }

    fn load_catalog(&self) -> BoxFuture<'_, Result<(), AssetError>> {
        async move {
            let catalog = self.read_catalog().await?;
            self.catalog.publish(Some(catalog));
            Ok(())
        }
        .boxed()
    }

    fn load_bytes(&self, path: String) -> BoxFuture<'_, Result<Vec<u8>, AssetError>> {
        async move { self.bytes(&path).await.map(|bytes| bytes.to_vec()) }.boxed()
    }

    fn load_text(&self, path: String) -> BoxFuture<'_, Result<String, AssetError>> {
        async move {
            let bytes = self.bytes(&path).await?;
            String::from_utf8(bytes.to_vec()).map_err(|error| AssetError::Decode {
                path,
                expected: "UTF-8 text",
                reason: error.to_string(),
            })
        }
        .boxed()
    }

    fn clear_cache(&self) {
        self.lock_cache().clear();
    }
}

/// The service's startup role: it loads the catalog when the app starts.
pub(crate) struct Loading(pub(crate) Arc<RayxAssets>);

impl AsyncInitializable for Loading {
    fn is_initialized(&self) -> bool {
        self.0.initialization.is_initialized()
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let service = Arc::clone(&self.0);
        async move {
            if service.catalog.get().is_none() {
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

impl StartupInitializable for Loading {
    fn startup_service_type(&self) -> Option<StartupServiceType> {
        Some(StartupServiceType::of::<dyn AssetService>())
    }
}
