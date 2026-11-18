//! The platform's part of the font service: registering a font's data.

use futures::future::BoxFuture;

/// Registers a font with the platform while the app runs: TrueType, OpenType, WOFF or WOFF2
/// data in, the font's family name out (or why the data is no font). Callable from any thread.
pub trait FontRegistrar: Send + Sync + 'static {
    fn register(&self, data: Vec<u8>) -> BoxFuture<'static, Result<String, String>>;

    /// A font the app ships (a font asset of the catalog, `uri` its `asset:/` URL): its family
    /// name, registering it unless the platform registered the app's fonts already. The
    /// default registers it; NSH's only reads its family, as NSH registers the fonts of the
    /// packages' `assets/` before the first frame.
    fn register_asset(
        &self,
        uri: &str,
        data: Vec<u8>,
    ) -> BoxFuture<'static, Result<String, String>> {
        let _ = uri;
        self.register(data)
    }
}
