//! RayX's i18n service changed from many threads at once: the app's commands on its own threads
//! and the platform's languages and clock on another. Whatever the interleaving, the service
//! publishes its states in the order it changed them (never an older one after a newer), the
//! state it publishes last is its latest one, and what a subscriber heard last is that state.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Barrier, Mutex};
use std::thread;

use futures::executor::block_on;
use rayx_app::di::StartupInitializationContext;
use rayx_app::services::i18n::{I18nService, I18nState};
use rayx_app::services::platform::{PlatformInfoService, PlatformSnapshot};
use rayx_app::testing::{FakePlatform, add_host_fakes};
use rayx_app::{RayXAppBuilder, ServiceProvider};
use rayx_services_i18n::I18nAppBuilderExt as _;

const LANGUAGES: [&str; 4] = ["en", "de", "hr", "fr"];
const ZONES: [&str; 3] = ["Europe/Zagreb", "Asia/Tokyo", "America/New_York"];
const ROUNDS: usize = 200;

/// RayX's service with messages in every language, following a fake platform.
fn start() -> (ServiceProvider, Arc<dyn I18nService>, Arc<FakePlatform>) {
    let mut builder = RayXAppBuilder::new().add_i18n(|options| {
        options.auto_load_assets(false).follow_platform(true);
    });
    let platform = FakePlatform::new(PlatformSnapshot::default());
    builder
        .services
        .add_singleton_as::<dyn PlatformInfoService>(
            Arc::clone(&platform) as Arc<dyn PlatformInfoService>
        );
    let services = add_host_fakes(builder).build().unwrap();
    let i18n = services.required_service::<dyn I18nService>().unwrap();
    for language in LANGUAGES {
        block_on(i18n.add_resource(language.into(), None, r#"{ "hi": "Hi" }"#.into())).unwrap();
    }
    block_on(
        services
            .startup_registry()
            .initialize(StartupInitializationContext::new(services.clone())),
    )
    .unwrap();
    (services, i18n, platform)
}

/// The published state without its version.
fn unversioned(state: &I18nState) -> I18nState {
    I18nState {
        version: 0,
        ..state.clone()
    }
}

/// What the service holds now: adding an empty resource changes nothing but makes it publish
/// its state again.
fn held(i18n: &Arc<dyn I18nService>) -> Arc<I18nState> {
    block_on(i18n.add_resource("en".into(), None, "{}".into())).unwrap();
    i18n.i18n().get()
}

#[test]
fn concurrent_changes_leave_the_service_s_latest_state_published() {
    for _ in 0..25 {
        let (_services, i18n, platform) = start();
        let heard = Arc::new(Mutex::new(None::<Arc<I18nState>>));
        let older = Arc::new(Mutex::new(Vec::new()));
        let (last, out_of_order) = (Arc::clone(&heard), Arc::clone(&older));
        let _subscription = i18n.i18n().subscribe(move |state| {
            let mut last = last.lock().unwrap();
            if let Some(before) = last
                .as_ref()
                .filter(|before| before.version >= state.version)
            {
                out_of_order
                    .lock()
                    .unwrap()
                    .push((before.version, state.version));
            }
            *last = Some(Arc::clone(state));
        });

        let threads = 6;
        // The workers, the platform's reporter and this thread.
        let barrier = Arc::new(Barrier::new(threads + 2));
        let mut handles = Vec::new();
        for worker in 0..threads {
            let i18n = Arc::clone(&i18n);
            let barrier = Arc::clone(&barrier);
            handles.push(thread::spawn(move || {
                barrier.wait();
                for round in 0..ROUNDS {
                    let pick = worker + round;
                    match pick % 4 {
                        0 | 1 => {
                            block_on(i18n.set_language(LANGUAGES[pick % LANGUAGES.len()].into()))
                                .unwrap()
                        }
                        2 => block_on(i18n.follow_platform()).unwrap(),
                        _ => {
                            block_on(i18n.set_time_zone(ZONES[pick % ZONES.len()].into())).unwrap()
                        }
                    }
                }
            }));
        }
        // The platform reports other languages and zones meanwhile, as an engine thread would.
        let reporter = {
            let platform = Arc::clone(&platform);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                for round in 0..ROUNDS {
                    let mut snapshot = (*platform.platform().get()).clone();
                    snapshot.locale.languages = vec![LANGUAGES[round % LANGUAGES.len()].into()];
                    snapshot.locale.time_zone = ZONES[round % ZONES.len()].into();
                    platform.publish(snapshot);
                }
            })
        };
        barrier.wait();
        for handle in handles {
            handle.join().unwrap();
        }
        reporter.join().unwrap();

        assert_eq!(
            *older.lock().unwrap(),
            [],
            "states published after a newer one"
        );
        let published = i18n.i18n().get();
        let heard_last = heard.lock().unwrap().clone().unwrap();
        assert_eq!(
            heard_last, published,
            "a subscriber's last state is the cell's"
        );
        let latest = held(&i18n);
        assert_eq!(
            unversioned(&published),
            unversioned(&latest),
            "the published state is the service's latest"
        );
        assert!(latest.version > published.version);
    }
}
