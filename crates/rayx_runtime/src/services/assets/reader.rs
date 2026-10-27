use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use futures::FutureExt as _;
use futures::future::LocalBoxFuture;
use rayx_core::di::AppService;

/// Reads the files the app ships by their `asset:/` URL: the platform's part of the asset
/// service (`rayx_app` reads them with NSH's `fetch`). A failure is a message naming why.
pub trait AssetReader: Send + Sync + 'static {
    fn read(&self, uri: &str) -> LocalBoxFuture<'static, Result<Vec<u8>, String>>;
}

impl AppService for dyn AssetReader {}

/// An [`AssetReader`] over files held in memory: for tests, and for apps that compile their
/// files in (`include_bytes!`).
#[derive(Clone, Debug, Default)]
pub struct MemoryAssetReader {
    files: Arc<Mutex<HashMap<String, Arc<[u8]>>>>,
}

impl MemoryAssetReader {
    pub fn new() -> Self {
        Self::default()
    }

    /// Holds `bytes` at `uri` (`asset:/@<package>/<path>`).
    pub fn insert(&self, uri: impl Into<String>, bytes: impl Into<Arc<[u8]>>) -> &Self {
        self.files
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(uri.into(), bytes.into());
        self
    }

    /// The same, built up: `MemoryAssetReader::new().with(uri, bytes)`.
    pub fn with(self, uri: impl Into<String>, bytes: impl Into<Arc<[u8]>>) -> Self {
        self.insert(uri, bytes);
        self
    }
}

impl AssetReader for MemoryAssetReader {
    fn read(&self, uri: &str) -> LocalBoxFuture<'static, Result<Vec<u8>, String>> {
        let found = self
            .files
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(uri)
            .map(|bytes| bytes.to_vec());
        let uri = uri.to_owned();
        async move { found.ok_or_else(|| format!("no file at {uri}")) }.boxed_local()
    }
}
