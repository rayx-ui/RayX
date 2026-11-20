use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use futures::FutureExt as _;
use futures::future::BoxFuture;

/// Reads the files the app ships by their `asset:/` URL: the platform's part of RayX's asset
/// service (on NSH, NSH's `fetch` through the engine). A failure is a message naming why.
/// Callable from any thread.
pub trait AssetReader: Send + Sync + 'static {
    fn read(&self, uri: &str) -> BoxFuture<'static, Result<Vec<u8>, String>>;

    /// The files every UI set the app serves ships (`asset:/@<package>/<path>`), a library's
    /// among them: on NSH, `served_asset_files()` read on the engine thread. None by default.
    fn served_files(&self) -> BoxFuture<'static, Vec<String>> {
        futures::future::ready(Vec::new()).boxed()
    }
}

/// An [`AssetReader`] over files held in memory: for tests, and for apps that compile their
/// files in (`include_bytes!`). Files held with [`serve`](Self::serve) are also the files the
/// app's UI sets ship ([`AssetReader::served_files`]), as a library's set ships its own.
#[derive(Clone, Debug, Default)]
pub struct MemoryAssetReader {
    files: Arc<Mutex<HashMap<String, Arc<[u8]>>>>,
    served: Arc<Mutex<Vec<String>>>,
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

    /// Holds `bytes` at `uri` (`asset:/@<package>/<path>`) as a file one of the app's UI sets
    /// ships: listed by [`AssetReader::served_files`] too.
    pub fn serve(&self, uri: impl Into<String>, bytes: impl Into<Arc<[u8]>>) -> &Self {
        let uri = uri.into();
        {
            let mut served = self.served.lock().unwrap_or_else(PoisonError::into_inner);
            if !served.contains(&uri) {
                served.push(uri.clone());
            }
        }
        self.insert(uri, bytes)
    }

    /// The same, built up: `MemoryAssetReader::new().serving(uri, bytes)`.
    pub fn serving(self, uri: impl Into<String>, bytes: impl Into<Arc<[u8]>>) -> Self {
        self.serve(uri, bytes);
        self
    }
}

impl AssetReader for MemoryAssetReader {
    fn read(&self, uri: &str) -> BoxFuture<'static, Result<Vec<u8>, String>> {
        let found = self
            .files
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(uri)
            .map(|bytes| bytes.to_vec());
        let uri = uri.to_owned();
        async move { found.ok_or_else(|| format!("no file at {uri}")) }.boxed()
    }

    fn served_files(&self) -> BoxFuture<'static, Vec<String>> {
        let served = self
            .served
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        futures::future::ready(served).boxed()
    }
}
