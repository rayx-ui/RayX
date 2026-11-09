//! The platform service during startup: startup services see NSH's environment (the
//! surface's size, the user's languages and time zone), not the snapshot's defaults.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{sync::Mutex, time::Duration};

use crossbeam_channel::{Receiver, Sender};
use futures::FutureExt as _;
use futures::future::LocalBoxFuture;
use nsh_core::geometry::{Points, Size};
use nsh_core::platform::{Locale, LocaleSettings, TimeZone};
use nsh_core::{ApproxTextEngine, MountBatch, MountItem, prelude::*};
use nsh_engine::{App, EngineConfig, Environment, LocalEngine, MountSink};
use rayx_app::di::{
    AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupInitializationState, StartupServiceType,
};
use rayx_app::rayx_runtime::prelude::*;
use rayx_app::rayx_runtime::services::platform::{PlatformInfoService, PlatformSnapshot};
use rayx_app::{
    AppService, AppState, RayXAppBuilder, ServiceAppBuilderExt as _, app_state, set_app,
};

/// What the platform service held when startup ran.
static SEEN: Mutex<Option<PlatformSnapshot>> = Mutex::new(None);

/// A startup service that reads the platform service while it initializes.
#[derive(Clone, Default, AppService)]
struct ReadsPlatform {
    initialized: StartupInitializationState,
}

impl AsyncInitializable for ReadsPlatform {
    fn is_initialized(&self) -> bool {
        self.initialized.is_initialized()
    }

    fn initialize(
        &self,
        context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let platform = context
            .services()
            .required_service::<PlatformInfoService>()
            .unwrap();
        *SEEN.lock().unwrap() = Some(platform.current());
        let initialized = self.initialized.clone();
        async move {
            initialized.mark_initialized();
            Ok(())
        }
        .boxed_local()
    }
}

impl StartupInitializable for ReadsPlatform {
    fn startup_service_type(&self) -> StartupServiceType {
        StartupServiceType::of::<Self>()
    }
}

fn build_app(builder: RayXAppBuilder) -> RayXAppBuilder {
    builder
        .add_platform()
        .add_startup_singleton(ReadsPlatform::default())
}

/// A Rust root that only starts the app: nothing keeps the platform current but the start.
#[component]
fn Root() -> impl IntoElement {
    rayx_app::start();
    dynamic(move || match app_state() {
        AppState::Ready => text("ready").into_element(),
        AppState::Failed(message) => text(format!("failed: {message}")).into_element(),
        _ => text("starting").into_element(),
    })
}

struct ChannelSink(Sender<MountBatch>);

impl MountSink for ChannelSink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.0.send(batch);
    }
}

fn pump_until(engine: &mut LocalEngine<ApproxTextEngine>, rx: &Receiver<MountBatch>, shown: &str) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut texts = Vec::new();
    while std::time::Instant::now() < deadline {
        engine.pump();
        while let Ok(batch) = rx.recv_timeout(Duration::from_millis(5)) {
            for item in &batch.items {
                if let MountItem::Text { text, .. } = item {
                    texts.push(text.text.to_string());
                }
            }
        }
        if texts.iter().any(|text| text == shown) {
            return;
        }
    }
    panic!("`{shown}` never showed: {texts:?}");
}

#[test]
fn startup_services_see_the_device_not_the_defaults() {
    set_app(build_app);
    let locale = Locale::parse("de-DE").unwrap();
    let mut locale_settings = LocaleSettings::of(locale);
    locale_settings.time_zone = TimeZone::parse("Europe/Zagreb").unwrap();
    let environment = Environment {
        locale,
        locale_settings,
        ..Environment::default()
    };
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut engine = LocalEngine::start(
        EngineConfig {
            viewport: Size::new(Points(800.0), Points(600.0)),
            environment,
            ..EngineConfig::new(App::Rust(nsh_core::element::Root::of::<Root>()))
        },
        ApproxTextEngine,
        ChannelSink(tx),
    )
    .unwrap();
    pump_until(&mut engine, &rx, "ready");

    let seen = SEEN.lock().unwrap().clone().expect("startup ran");
    assert_eq!((seen.viewport.width, seen.viewport.height), (800.0, 600.0));
    assert_eq!(seen.locale.languages, ["de-DE"]);
    assert_eq!(seen.locale.time_zone, "Europe/Zagreb");
    assert_eq!(seen.locale.first_day_of_week, "monday");
}
