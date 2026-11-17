//! Ready hooks written in TS: JS functions in the app's one [`ReadyHooks`] list, run in order
//! with Rust's once every startup service initialized, each awaited in JS's next turn.
//!
//! [`ReadyHooks`]: rayx_app::ReadyHooks

use std::sync::Arc;

use nsh_core::prelude::*;
use rayx_app::di::ThreadBound;
use rayx_app::{RayXAppBuilder, ReadyAppBuilderExt as _, ReadyError};

use super::provider::{ServiceProvider, root_of};

/// A TS ready hook: the context in, nothing (or a promise of nothing) out.
pub(crate) type Hook = Callback<Instance<ReadyContext>, ()>;

/// A TS `onReadyWhen` predicate: the root `ServiceProvider` in, whether the hook runs out.
pub(crate) type Predicate = Callback<Instance<ServiceProvider>, bool>;

/// What a ready hook gets: the app's root `ServiceProvider`, its startup services initialized.
#[class]
pub struct ReadyContext {
    services: Instance<ServiceProvider>,
}

#[class]
impl ReadyContext {
    /// The app's root provider (the one `start()` resolves with).
    #[getter]
    pub fn services(&self) -> Instance<ServiceProvider> {
        self.services.clone()
    }
}

/// A TS hook as the `Send + Sync` hook list keeps it.
pub(crate) struct TsHook {
    predicate: Option<Predicate>,
    hook: Hook,
    sys: System,
}

/// A TS hook, kept on the engine thread for the `Send + Sync` hook list.
pub(crate) type SharedHook = Arc<ThreadBound<TsHook>>;

/// The TS `hook` (run when `predicate`, if any, holds), for [`add`].
pub(crate) fn hook(
    predicate: Option<Predicate>,
    hook: Hook,
    sys: &System,
) -> Result<SharedHook, String> {
    ThreadBound::new(
        TsHook {
            predicate,
            hook,
            sys: sys.clone(),
        },
        "a TS ready hook",
    )
    .map(Arc::new)
    .map_err(|error| error.to_string())
}

/// `builder` with the TS `hook` added to its ready hooks.
pub(crate) fn add(builder: RayXAppBuilder, hook: SharedHook) -> RayXAppBuilder {
    builder.on_ready(move |context| {
        let hook = hook.get();
        async move {
            let hook = hook.map_err(|error| ReadyError::new(error.to_string()))?;
            let services = root_of(&hook.sys, context.services().clone());
            if let Some(predicate) = &hook.predicate
                && !predicate
                    .call_async(services.clone())
                    .await
                    .map_err(ReadyError::new)?
            {
                return Ok(());
            }
            let context = Instance::new(ReadyContext { services });
            hook.hook.call_async(context).await.map_err(ReadyError::new)
        }
    })
}
