//! Registering fonts on NSH, through the surface's engine runner (so from any thread).

use futures::FutureExt as _;
use futures::future::BoxFuture;
use nsh_core::prelude::register_font;
use rayx_app_nsh::EngineRunner;

use crate::registrar::FontRegistrar;

/// Registers fonts with NSH's `register_font` (TrueType, OpenType, WOFF, WOFF2) on the engine
/// thread, which reads their family names. The fonts the app ships NSH registered before the
/// first frame (every font of its UI sets' crates' `assets/`), so for those it only reads the
/// family.
#[derive(Clone, Debug)]
pub(crate) struct NshFontRegistrar {
    /// The surface's runner (none outside an app's start, where nothing can be registered).
    pub(crate) runner: Option<EngineRunner>,
}

impl FontRegistrar for NshFontRegistrar {
    fn register(&self, data: Vec<u8>) -> BoxFuture<'static, Result<String, String>> {
        let Some(runner) = self.runner.clone() else {
            return futures::future::ready(Err(
                "the app has not started on an NSH surface".to_owned()
            ))
            .boxed();
        };
        let registered = runner.run(move || register_font(data));
        async move { registered.await.map_err(|gone| gone.to_string())? }.boxed()
    }

    fn register_asset(
        &self,
        _uri: &str,
        data: Vec<u8>,
    ) -> BoxFuture<'static, Result<String, String>> {
        futures::future::ready(nsh_core::font_family(&data)).boxed()
    }
}
