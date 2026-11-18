//! Ready hooks on the platform: `on_ready_when_platform` runs its hook only when the platform
//! service's snapshot (a fake's here) satisfies the predicate once the app is ready.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use futures::executor::block_on;
use rayx_app::prelude::*;
use rayx_app::services::platform::{
    FormFactor, PlatformInfoService, PlatformReadyAppBuilderExt as _, PlatformSnapshot,
};
use rayx_app::testing::FakePlatform;

fn run(form_factor: FormFactor) -> Vec<&'static str> {
    let log = Arc::new(Mutex::new(Vec::new()));
    let handheld = log.clone();
    let desktop = log.clone();
    let mut builder = RayXAppBuilder::new();
    builder
        .services_mut()
        .add_singleton_as::<dyn PlatformInfoService>(FakePlatform::new(PlatformSnapshot {
            form_factor,
            ..PlatformSnapshot::default()
        }) as Arc<dyn PlatformInfoService>);
    let services = builder
        .on_ready_when_platform(
            |platform| platform.form_factor.is_handheld(),
            move |_| {
                let log = handheld.clone();
                async move {
                    log.lock().unwrap().push("handheld");
                    Ok(())
                }
            },
        )
        .on_ready_when_platform(
            |platform| platform.form_factor == FormFactor::Desktop,
            move |_| {
                let log = desktop.clone();
                async move {
                    log.lock().unwrap().push("desktop");
                    Ok(())
                }
            },
        )
        .build()
        .unwrap();
    let hooks = services.required_service::<ReadyHooks>().unwrap();
    block_on(hooks.run(&services)).unwrap();
    log.lock().unwrap().clone()
}

#[test]
fn platform_ready_hooks_run_for_the_matching_device() {
    assert_eq!(run(FormFactor::Phone), ["handheld"]);
    assert_eq!(run(FormFactor::Desktop), ["desktop"]);
}

#[test]
fn platform_ready_hooks_need_the_platform_service() {
    let services = RayXAppBuilder::new()
        .on_ready_when_platform(|_| true, |_| async { Err(ReadyError::new("ran")) })
        .build()
        .unwrap();
    let hooks = services.required_service::<ReadyHooks>().unwrap();
    assert_eq!(block_on(hooks.run(&services)), Ok(()));
}
