// A test harness: it panics on a broken fixture by design.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod harness {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use crossbeam_channel::{Receiver, Sender};
    use nsh_core::{ApproxTextEngine, MountBatch, MountItem, element::Registry};
    use nsh_engine::{App, Assets, Bundle, Engine, EngineConfig, MountSink};

    struct ChannelSink(Sender<MountBatch>);

    impl MountSink for ChannelSink {
        fn submit(&self, batch: MountBatch) {
            let _ = self.0.send(batch);
        }
    }

    /// The set `test/app`: RayX's TS classes and `builder.addHostFakes()` ([`HostFakes`]),
    /// then what `extra` adds.
    pub fn set(extra: impl FnOnce(&mut Registry)) -> Registry {
        let mut set = Registry::ui_set("test/app");
        ::rayx_app_nsh::ts::register(&mut set);
        set.class_extension::<HostFakes>();
        extra(&mut set);
        set
    }

    /// `builder.addHostFakes()`: registers fakes of the services the host requires (platform,
    /// suspension, assets and fonts) where the app registered none
    /// ([`rayx_app::testing::add_host_fakes`]).
    pub struct HostFakes;

    #[::nsh_core::prelude::class_extension(::rayx_app_nsh::ts::RayXAppBuilder)]
    impl HostFakes {
        /// Registers fakes of the services the host requires where the app registered none.
        pub fn add_host_fakes(
            this: ::nsh_core::prelude::Instance<::rayx_app_nsh::ts::RayXAppBuilder>,
        ) -> Result<::nsh_core::prelude::Instance<::rayx_app_nsh::ts::RayXAppBuilder>, String>
        {
            ::rayx_app_nsh::ts::RayXAppBuilder::extend_with(this, |builder| {
                ::rayx_app::testing::add_host_fakes(builder)
            })
        }
    }

    /// The set's TypeScript declarations.
    pub fn declarations(set: &Registry) -> String {
        nsh_core::element::declarations(set, &[])
    }

    /// A running app: its engine and what it shows.
    pub struct Running {
        _engine: Engine,
        rx: Receiver<MountBatch>,
    }

    impl Running {
        /// The app's engine.
        pub fn engine(&self) -> &Engine {
            &self._engine
        }

        /// The last text shown once one starts with `prefix` (or after 10 s).
        pub fn shown(&self, prefix: &str) -> String {
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            let mut last = String::new();
            while let Ok(batch) = self.rx.recv_deadline(deadline) {
                for item in &batch.items {
                    if let MountItem::Text { text, .. } = item {
                        last = text.text.clone();
                    }
                }
                if last.starts_with(prefix) {
                    break;
                }
            }
            last
        }
    }

    /// Starts `tsx` (the app's entry) with `set` and the app's files `assets`.
    pub fn start(set: Registry, tsx: &str, assets: Option<Arc<dyn Assets>>) -> Running {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "rayx-app-headless-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("App.tsx");
        std::fs::write(&entry, tsx).unwrap();
        let ui_sets = vec![set];
        let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
            entry,
            ui_sets: ui_sets.iter().map(Into::into).collect(),
            // The rest as by default: not minified, not split, no app info, the entry's own
            // folder as the app's.
            ..Default::default()
        })
        .unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let (tx, rx) = crossbeam_channel::unbounded();
        let app = App::Js(Bundle::source(bundle.code));
        let config = EngineConfig {
            ui_sets,
            assets,
            ..EngineConfig::new(app)
        };
        let engine = Engine::start(config, || ApproxTextEngine, ChannelSink(tx)).unwrap();
        Running {
            _engine: engine,
            rx,
        }
    }

    /// Runs `tsx` with `set`: the last text once one starts with `done`.
    pub fn run(set: Registry, tsx: &str) -> String {
        start(set, tsx, None).shown("done")
    }

    /// Runs `tsx` with `set` and the app's files `assets`: the last text once one starts with
    /// `done`.
    pub fn run_with_assets(set: Registry, tsx: &str, assets: Arc<dyn Assets>) -> String {
        start(set, tsx, Some(assets)).shown("done")
    }
}

pub use harness::*;
