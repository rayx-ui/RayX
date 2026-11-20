//! Reading the app's files on NSH: NSH's `fetch` of their `asset:/` URLs, and the list of the
//! files the app's UI sets ship (`served_asset_files()`, a library's among them), on the engine
//! thread, through the surface's engine runner (so the asset service reads from any thread).

use futures::FutureExt as _;
use futures::future::BoxFuture;
use nsh_core::element::served_asset_files;
use nsh_core::prelude::fetch;
use rayx_app_nsh::EngineRunner;

use crate::reader::AssetReader;

/// Reads the app's files with NSH's `fetch` of their `asset:/` URLs (from the app's package on
/// device, the dev server during `nsh run`, the page's assets on the web).
#[derive(Clone, Debug)]
pub(crate) struct NshAssetReader {
    /// The surface's runner (none outside an app's start, where nothing can be read).
    pub(crate) runner: Option<EngineRunner>,
}

impl AssetReader for NshAssetReader {
    fn read(&self, uri: &str) -> BoxFuture<'static, Result<Vec<u8>, String>> {
        let uri = uri.to_owned();
        let Some(runner) = self.runner.clone() else {
            return futures::future::ready(Err(format!(
                "{uri} could not be read: the app has not started on an NSH surface"
            )))
            .boxed();
        };
        let read = runner.run(move || async move {
            let response = fetch(uri.as_str()).await?;
            if response.ok() {
                Ok(response.body)
            } else {
                Err(format!("{uri} answered {}", response.status()))
            }
        });
        async move { read.await.map_err(|gone| gone.to_string())? }.boxed()
    }

    /// The files the engine's UI sets ship, read from its context on its thread (empty off it):
    /// none outside an app's start, or once the engine stopped.
    fn served_files(&self) -> BoxFuture<'static, Vec<String>> {
        let Some(runner) = self.runner.clone() else {
            return futures::future::ready(Vec::new()).boxed();
        };
        let files = runner.run(|| async {
            served_asset_files()
                .iter()
                .map(|file| file.uri.to_owned())
                .collect::<Vec<String>>()
        });
        async move { files.await.unwrap_or_default() }.boxed()
    }
}
