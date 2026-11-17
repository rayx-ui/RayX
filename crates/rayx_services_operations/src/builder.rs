//! `add_operations`: in Rust ([`OperationsAppBuilderExt`]) and in TS ([`OperationsBuilder`], a
//! class extension of the host's `RayXAppBuilder`).

use std::sync::Arc;

use nsh_core::prelude::*;
use rayx_app::AppBuilder;
use rayx_app::services::operations::OperationService;
use rayx_app_nsh::ts::RayXAppBuilder;

use crate::service::RayxOperations;

/// Registers RayX's operation service on an app builder.
pub trait OperationsAppBuilderExt: AppBuilder {
    /// Registers RayX's operation service as the app's `dyn OperationService`, replacing any
    /// other: it applies operations through their targets' handlers, in transactions, and
    /// publishes them for the app's history and peers. Nothing to initialize: it is ready once
    /// made.
    fn add_operations(mut self) -> Self {
        self.services_mut()
            .replace_service_factory_as::<dyn OperationService>(
                |_| Arc::new(RayxOperations::new()),
            );
        self
    }
}

impl<T> OperationsAppBuilderExt for T where T: AppBuilder {}

/// The methods `rayx_services_operations` adds to TS's `RayXAppBuilder`.
pub struct OperationsBuilder;

#[class_extension(RayXAppBuilder)]
impl OperationsBuilder {
    /// Registers the app's operation service (the commands its history undoes and redoes).
    pub fn add_operations(
        this: Instance<RayXAppBuilder>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        RayXAppBuilder::extend_with(this, OperationsAppBuilderExt::add_operations)
    }
}
