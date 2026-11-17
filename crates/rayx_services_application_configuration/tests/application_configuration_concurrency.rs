//! RayX's application configuration changed from many threads at once: the app's settings
//! replaced on its own threads and the platform's motion setting on another. Whatever the
//! interleaving, the service publishes its snapshots in the order it changed them (never an
//! older revision after a newer), the snapshot it publishes last is its latest one (its revision
//! counts every change), and what a subscriber heard last is that snapshot.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Barrier, Mutex};
use std::thread;

use futures::executor::block_on;
use rayx_app::services::application_configuration::{
    ApplicationConfigurationService, ApplicationConfigurationSnapshot,
};
use rayx_app::services::platform::{PlatformInfoService, PlatformSnapshot};
use rayx_app::testing::FakePlatform;
use rayx_app::{AppBuilder as _, RayXAppBuilder};
use rayx_services_application_configuration::ApplicationConfigurationAppBuilderExt as _;

const THREADS: usize = 6;
const ROUNDS: usize = 200;

fn start() -> (Arc<dyn ApplicationConfigurationService>, Arc<FakePlatform>) {
    let platform = FakePlatform::new(PlatformSnapshot::default());
    let mut builder = RayXAppBuilder::new().add_application_configuration(|_| {});
    builder
        .services_mut()
        .add_singleton_as::<dyn PlatformInfoService>(
            Arc::clone(&platform) as Arc<dyn PlatformInfoService>
        );
    let services = builder.build().unwrap();
    services.validate_options_on_start().unwrap();
    (
        services
            .required_service::<dyn ApplicationConfigurationService>()
            .unwrap(),
        platform,
    )
}

#[test]
fn concurrent_changes_leave_the_latest_snapshot_published() {
    for _ in 0..20 {
        let (configuration, platform) = start();
        let heard = Arc::new(Mutex::new(None::<Arc<ApplicationConfigurationSnapshot>>));
        let older = Arc::new(Mutex::new(Vec::new()));
        let (last, out_of_order) = (Arc::clone(&heard), Arc::clone(&older));
        let _subscription = configuration.configuration().subscribe(move |snapshot| {
            let mut last = last.lock().unwrap();
            if let Some(before) = last
                .as_ref()
                .filter(|before| before.revision >= snapshot.revision)
            {
                out_of_order
                    .lock()
                    .unwrap()
                    .push((before.revision, snapshot.revision));
            }
            *last = Some(Arc::clone(snapshot));
        });

        // The workers, the platform's reporter and this thread.
        let barrier = Arc::new(Barrier::new(THREADS + 2));
        let workers: Vec<_> = (0..THREADS)
            .map(|worker| {
                let configuration = Arc::clone(&configuration);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    let mut changed = 0_u64;
                    for round in 0..ROUNDS {
                        let mut options = configuration.options();
                        options.tooltips.show_delay_ms = (worker * ROUNDS + round) as u64 + 1;
                        if block_on(configuration.replace_options(options)).unwrap() {
                            changed += 1;
                        }
                    }
                    changed
                })
            })
            .collect();
        // The platform reports the motion setting meanwhile, as an engine thread would: each
        // report changes it.
        let reporter = {
            let platform = Arc::clone(&platform);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                for round in 0..ROUNDS {
                    let mut snapshot = (*platform.platform().get()).clone();
                    snapshot.reduced_motion = round % 2 == 0;
                    platform.publish(snapshot);
                }
            })
        };
        barrier.wait();
        let changed: u64 = workers.into_iter().map(|w| w.join().unwrap()).sum();
        reporter.join().unwrap();

        assert_eq!(
            *older.lock().unwrap(),
            [],
            "snapshots published after a newer one"
        );
        let published = configuration.configuration().get();
        assert_eq!(
            published.revision,
            changed + ROUNDS as u64,
            "the published snapshot has every change"
        );
        assert_eq!(
            published.platform_reduced_motion,
            platform.platform().get().reduced_motion
        );
        let heard_last = heard.lock().unwrap().clone().unwrap();
        assert_eq!(
            heard_last, published,
            "a subscriber's last snapshot is the cell's"
        );
    }
}
