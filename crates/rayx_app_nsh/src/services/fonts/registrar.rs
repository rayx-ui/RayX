//! Registering fonts on NSH.

use futures::FutureExt as _;
use futures::future::LocalBoxFuture;
use nsh_core::prelude::register_font;
use rayx_runtime::services::fonts::FontRegistrar;

/// Registers fonts with NSH's `register_font` (TrueType, OpenType, WOFF, WOFF2), which reads
/// their family names. The fonts the app ships NSH registered before the first frame (every
/// font of its UI sets' crates' `assets/`), so for those it only reads the family.
#[derive(Clone, Copy, Debug, Default)]
pub struct NshFontRegistrar;

impl FontRegistrar for NshFontRegistrar {
    fn register(&self, data: Vec<u8>) -> LocalBoxFuture<'static, Result<String, String>> {
        async move { register_font(data).await }.boxed_local()
    }

    fn register_asset(
        &self,
        _uri: &str,
        data: Vec<u8>,
    ) -> LocalBoxFuture<'static, Result<String, String>> {
        async move { nsh_core::font_family(&data) }.boxed_local()
    }
}
