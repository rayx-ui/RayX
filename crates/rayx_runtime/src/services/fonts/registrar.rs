use futures::future::LocalBoxFuture;
use rayx_core::di::AppService;

/// Registers a font with the platform while the app runs: TrueType, OpenType, WOFF or WOFF2
/// data in, the font's family name out (or why the data is no font). The platform's part of
/// the font service: `rayx_app` registers with NSH's `register_font`.
pub trait FontRegistrar: Send + Sync + 'static {
    fn register(&self, data: Vec<u8>) -> LocalBoxFuture<'static, Result<String, String>>;
}

impl AppService for dyn FontRegistrar {}
