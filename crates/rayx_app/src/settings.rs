//! The app's bundled settings (`appsettings.json`) as a configuration source.

use futures::FutureExt as _;
use nsh_core::prelude::fetch;
use rayx_core::AppBuilder;
use rayx_core::configuration::{ConfigurationSource, SourceLoad, Value};

/// The file name of the app's settings.
pub const APP_SETTINGS_FILE: &str = "appsettings.json";

/// A JSON file the app ships, read with NSH's bundled-file reading (`fetch` of an `asset:/`
/// URL) whenever the configuration builds or reloads. It loads asynchronously: build the app
/// with [`RayXAppBuilder::build_async`](rayx_core::RayXAppBuilder::build_async), as
/// [`start_app`](crate::start_app) does, in an NSH task.
#[derive(Clone, Debug)]
pub struct AppSettingsSource {
    uri: String,
    optional: bool,
}

impl AppSettingsSource {
    /// The JSON file at `uri` (`asset:/@<package>/<path>`).
    pub fn new(uri: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            optional: false,
        }
    }

    /// `appsettings.json` in `package`'s `assets/` directory.
    pub fn for_package(package: &str) -> Self {
        Self::new(format!("asset:/@{package}/{APP_SETTINGS_FILE}"))
    }

    /// The same source, adding nothing when the file is missing (rather than failing).
    pub fn optional(mut self) -> Self {
        self.optional = true;
        self
    }

    /// The file's URL.
    pub fn uri(&self) -> &str {
        &self.uri
    }
}

impl ConfigurationSource for AppSettingsSource {
    fn name(&self) -> String {
        self.uri.clone()
    }

    fn load(&self) -> SourceLoad {
        let uri = self.uri.clone();
        let optional = self.optional;
        // `fetch` runs when the load is polled: in the task that builds the configuration.
        SourceLoad::Pending(
            async move {
                match fetch(uri.as_str()).await {
                    Ok(response) if response.ok() => {
                        serde_json::from_slice(&response.body).map_err(|error| error.to_string())
                    }
                    Ok(response) if optional && response.status() == 404 => Ok(Value::Null),
                    Ok(response) => Err(format!("{uri} answered {}", response.status())),
                    Err(_) if optional => Ok(Value::Null),
                    Err(error) => Err(error),
                }
            }
            .boxed_local(),
        )
    }
}

/// Adds the app's bundled settings to an app builder's configuration.
pub trait AppSettingsAppBuilderExt: AppBuilder {
    /// Adds `appsettings.json` from `package`'s `assets/` directory (the app's crate) as a
    /// configuration source: a missing file fails startup.
    fn add_app_settings(mut self, package: &str) -> Self {
        self.configuration_mut()
            .add_source(AppSettingsSource::for_package(package));
        self
    }

    /// Adds the JSON file at `uri` as a configuration source, adding nothing when it is
    /// missing (environment-specific settings, `appsettings.Development.json`).
    fn add_optional_app_settings(mut self, uri: impl Into<String>) -> Self {
        self.configuration_mut()
            .add_source(AppSettingsSource::new(uri).optional());
        self
    }
}

impl<T> AppSettingsAppBuilderExt for T where T: AppBuilder {}
