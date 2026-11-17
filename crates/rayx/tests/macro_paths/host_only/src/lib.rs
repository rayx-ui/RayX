//! A crate on `rayx_app_nsh` alone: the derive names `::rayx_app_nsh::di::AppService`, and
//! `#[rayx_app_nsh::contract]` and `#[rayx_app_nsh::app_extension]` name `::rayx_app_nsh`.

use rayx_app_nsh::ts::{ContractError, Exception};
use rayx_app_nsh::{AppService, RayXAppBuilder, ServiceAppBuilderExt as _};

#[derive(AppService)]
pub struct Greeting(pub &'static str);

fn is_app_service<T: ::rayx_app_nsh::di::AppService>() {}

pub fn check() {
    is_app_service::<Greeting>();
}

/// Why a forecast failed.
pub enum ForecastError {
    /// The call did not reach TS.
    Contract(ContractError),
    /// What TS threw.
    Js(String),
}

impl From<ContractError> for ForecastError {
    fn from(error: ContractError) -> Self {
        Self::Contract(error)
    }
}

impl From<Exception> for ForecastError {
    fn from(exception: Exception) -> Self {
        Self::Js(exception.message)
    }
}

/// Forecasts, implementable in TS.
#[rayx_app_nsh::contract]
pub trait Forecasts {
    /// The forecast for `city`.
    async fn forecast(&self, city: String) -> Result<String, ForecastError>;
    /// Notes `line`.
    fn note(&self, line: String);
}

/// Registers the greeting.
#[rayx_app_nsh::app_extension]
pub fn add_greeting(builder: RayXAppBuilder) -> RayXAppBuilder {
    builder.add_singleton(Greeting("hello"))
}
