//! `add_assets`: in Rust ([`AssetAppBuilderExt`]) and in TS ([`AssetBuilder`], a class extension
//! of the host's `RayXAppBuilder`).

use std::sync::Arc;

use nsh_core::element::AssetFile;
use nsh_core::prelude::*;
use rayx_app::services::assets::AssetService;
use rayx_app::{AppBuilder, ServiceAppBuilderExt as _};
use rayx_app_nsh::EngineRunner;
use rayx_app_nsh::ts::{RayXAppBuilder, record};

use crate::nsh_reader::NshAssetReader;
use crate::options_ts::AssetOptions as TsAssetOptions;
use crate::reader::AssetReader;
use crate::service::{Loading, RayxAssets};
use crate::{ASSET_OPTIONS_SECTION, AssetOptions, BundledAssets};

/// Registers RayX's asset service on an app builder.
pub trait AssetAppBuilderExt: AppBuilder {
    /// Registers RayX's asset service as the app's `dyn AssetService`, replacing any other: a
    /// startup service that reads the packages' manifests and the files the app's UI sets ship
    /// (a library's set, which the app's serves, ships its own) when the app starts, and the
    /// files with NSH (from any thread). Its [`AssetOptions`] bind from [`ASSET_OPTIONS_SECTION`],
    /// then `configure` changes them (`options.package("my-app")`).
    fn add_assets(self, configure: impl Fn(&mut AssetOptions) + Send + Sync + 'static) -> Self {
        register(self, configure, None)
    }

    /// Adds the files at `uris` (`asset:/@<package>/<path>`: the files a package ships, as the
    /// platform lists them) to the asset catalog, which then needs no manifest listing them;
    /// a package's manifest still adds metadata when the package ships one. Packages not in
    /// the asset options come after theirs, in the order their files are added, and before the
    /// packages of the files the app's UI sets ship (which the catalog takes without this).
    fn add_bundled_assets<I>(mut self, uris: I) -> Self
    where
        I: IntoIterator,
        I::Item: AsRef<str>,
    {
        let services = self.services_mut();
        services.try_add_service(BundledAssets::new());
        if let Some(bundled) = services.service::<BundledAssets>() {
            for uri in uris {
                bundled.add(uri.as_ref());
            }
        }
        self
    }

    /// Adds the files a crate ships, as NSH's `assets!()` lists them (the list its UI set
    /// registers), to the asset catalog: `builder.add_asset_files(assets!())`. The crate's
    /// manifest (`assets.toml`) is then optional, for metadata (tags, locale, theme, ...).
    fn add_asset_files(self, files: &[AssetFile]) -> Self {
        self.add_bundled_assets(files.iter().map(|file| file.uri))
    }
}

impl<T> AssetAppBuilderExt for T where T: AppBuilder {}

/// `add_assets`, reading the files through `reader` when one is given (a test's), else with
/// NSH through the surface's engine runner.
pub(crate) fn register<B: AppBuilder>(
    mut builder: B,
    configure: impl Fn(&mut AssetOptions) + Send + Sync + 'static,
    reader: Option<Arc<dyn AssetReader>>,
) -> B {
    builder = builder.add_options::<AssetOptions>(|options| {
        options
            .bind(ASSET_OPTIONS_SECTION)
            .configure(configure)
            .validate(
                |options| !options.manifest.trim().is_empty(),
                "the asset manifest's file name is empty",
            )
            .validate_on_start();
    });
    let services = builder.services_mut();
    services.try_add_service(BundledAssets::new());
    services.replace_service_factory_as::<RayxAssets>(move |services| {
        let options = services
            .options::<AssetOptions>()
            .unwrap_or_else(|error| panic!("the asset options are invalid: {error}"))
            .cloned();
        let reader = reader.clone().unwrap_or_else(|| {
            Arc::new(NshAssetReader {
                runner: services
                    .service::<EngineRunner>()
                    .map(|runner| (*runner).clone()),
            })
        });
        RayxAssets::new(options, reader, services.service::<BundledAssets>())
    });
    services.replace_service_factory_as::<dyn AssetService>(|services| {
        services
            .required_service::<RayxAssets>()
            .unwrap_or_else(|error| panic!("RayX's asset service is registered: {error}"))
    });
    services.add_startup_resolved_async(
        rayx_app::di::StartupServiceType::of::<dyn AssetService>(),
        |context| {
            let assets = context.services().required_service::<RayxAssets>();
            Box::pin(async move {
                let assets =
                    assets.map_err(|error| rayx_app::di::StartupInitializationFailure {
                        service: rayx_app::di::StartupServiceType::of::<dyn AssetService>(),
                        message: error.to_string(),
                        timed_out: false,
                    })?;
                Ok(Arc::new(Loading(assets)) as rayx_app::di::StartupService)
            })
        },
    );
    builder
}

/// The methods `rayx_services_assets` adds to TS's `RayXAppBuilder`.
pub struct AssetBuilder;

#[class_extension(RayXAppBuilder)]
impl AssetBuilder {
    /// Registers RayX's asset service; `configure` adds its packages
    /// (`options => { options.package("my-app"); }`).
    pub fn add_assets(
        this: Instance<RayXAppBuilder>,
        #[args(options)] configure: Option<Callback<Instance<TsAssetOptions>>>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        let packages = record(TsAssetOptions::new(), configure, TsAssetOptions::take)?;
        RayXAppBuilder::extend_with(this, move |builder| {
            builder.add_assets(move |options| {
                for package in &packages {
                    TsAssetOptions::apply(package, options);
                }
            })
        })
    }
}
