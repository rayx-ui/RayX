use futures::future::LocalBoxFuture;
use rayx_core::di::AppService;

/// Registers a font with the platform while the app runs: TrueType, OpenType, WOFF or WOFF2
/// data in, the font's family name out (or why the data is no font). The platform's part of
/// the font service: `rayx_app` registers with NSH's `register_font`.
pub trait FontRegistrar: Send + Sync + 'static {
    fn register(&self, data: Vec<u8>) -> LocalBoxFuture<'static, Result<String, String>>;

    /// A font the app ships (a font asset of the catalog, `uri` its `asset:/` URL): its family
    /// name, registering it unless the platform registered the app's fonts already. The
    /// default registers it; `rayx_app` only reads its family, as NSH registers the fonts of
    /// the packages' `assets/` before the first frame.
    fn register_asset(
        &self,
        uri: &str,
        data: Vec<u8>,
    ) -> LocalBoxFuture<'static, Result<String, String>> {
        let _ = uri;
        self.register(data)
    }
}

impl AppService for dyn FontRegistrar {}
