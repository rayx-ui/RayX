//! Reading the app's files on NSH.

use futures::FutureExt as _;
use futures::future::LocalBoxFuture;
use nsh_core::element::AssetFile;
use nsh_core::prelude::fetch;
use rayx_app::AppBuilder;
use rayx_runtime::services::assets::AssetAppBuilderExt as _;
use rayx_runtime::services::assets::AssetReader;

/// Reads the app's files with NSH's `fetch` of their `asset:/` URLs (from the app's package on
/// device, the dev server during `nsh run`, the page's assets on the web).
#[derive(Clone, Copy, Debug, Default)]
pub struct NshAssetReader;

impl AssetReader for NshAssetReader {
    fn read(&self, uri: &str) -> LocalBoxFuture<'static, Result<Vec<u8>, String>> {
        let uri = uri.to_owned();
        async move {
            let response = fetch(uri.as_str()).await?;
            if response.ok() {
                Ok(response.body)
            } else {
                Err(format!("{uri} answered {}", response.status()))
            }
        }
        .boxed_local()
    }
}

/// Adds the files a crate ships, as NSH's `assets!()` lists them, to the asset catalog
/// ([`NshServicesAppBuilderExt::add_asset_files`](crate::NshServicesAppBuilderExt::add_asset_files)).
pub(crate) fn add_asset_files<B: AppBuilder>(builder: B, files: &[AssetFile]) -> B {
    builder.add_bundled_assets(files.iter().map(|file| file.uri))
}
