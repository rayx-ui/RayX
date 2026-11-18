//! The fonts the app ships are registered once, by NSH before the first frame: the font service
//! only reads their families for its aliases. A font registered while the app runs goes
//! through NSH's `register_font`, from the engine's thread or any other (through the
//! surface's engine runner). The font service reads the shipped font through the app's
//! `dyn AssetService` (a fake listing it in its catalog).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{
    cell::RefCell,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use crossbeam_channel::{Receiver, Sender};
use futures::executor::block_on;
use nsh_core::element::{AssetFile, AssetKind, Registry};
use nsh_core::{
    ApproxTextEngine, AttributedText, MountBatch, Points, Size, TextEngine, TextLayout, prelude::*,
};
use nsh_engine::{App, Assets, EngineConfig, LocalEngine, MountSink};
use rayx_app::services::assets::{AssetCatalog, AssetService};
use rayx_app::services::fonts::FontService;
use rayx_app::testing::{FakeAssets, add_host_fakes};
use rayx_app_nsh::{
    AppState, RayXAppBuilder, ServiceProvider, StartAppBuilderExt as _, app_services, app_state,
};
use rayx_services_fonts::FontAppBuilderExt as _;

const PACIFICO: &[u8] = include_bytes!("fixtures/Pacifico-Regular.woff2");

thread_local! {
    static ROOT: RefCell<Option<ServiceProvider>> = const { RefCell::new(None) };
}

/// The fonts registered with the text engine, and how many there were once the app was ready.
static REGISTERED: AtomicUsize = AtomicUsize::new(0);
static AT_READY: Mutex<Option<usize>> = Mutex::new(None);

/// The app crate's files, as `assets!()` lists them for its UI set.
static FILES: &[AssetFile] = &[AssetFile {
    uri: "asset:/@app/fonts/Pacifico-Regular.woff2",
    source: "/nowhere/fonts/Pacifico-Regular.woff2",
    kind: AssetKind::Font,
}];

struct AppAssets;

impl Assets for AppAssets {
    fn read(&self, uri: &str) -> Result<Vec<u8>, String> {
        match uri {
            "asset:/@app/fonts/Pacifico-Regular.woff2" => Ok(PACIFICO.to_vec()),
            _ => Err(format!("{uri} is not bundled")),
        }
    }
}

/// Counts the fonts registered with the text engine.
struct CountingFonts(ApproxTextEngine);

impl TextEngine for CountingFonts {
    type Prepared = <ApproxTextEngine as TextEngine>::Prepared;

    fn prepare(&self, text: &AttributedText) -> Self::Prepared {
        self.0.prepare(text)
    }

    fn measure(&self, prepared: &Self::Prepared, max_width: Option<Points>) -> Size<Points> {
        self.0.measure(prepared, max_width)
    }

    fn layout(&self, prepared: &Self::Prepared, size: Size<Points>) -> TextLayout {
        self.0.layout(prepared, size)
    }

    fn register_font(&self, _data: &[u8]) -> Result<(), String> {
        REGISTERED.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

/// The app: RayX's font service over assets listing the shipped font, and fakes of the other
/// services the host requires.
fn build_app(builder: RayXAppBuilder) -> RayXAppBuilder {
    let mut catalog = AssetCatalog::builder();
    catalog
        .add_manifest_text("app", "files = [\"fonts/Pacifico-Regular.woff2\"]")
        .unwrap();
    let assets =
        FakeAssets::new(catalog.build()).file("@app/fonts/Pacifico-Regular.woff2", PACIFICO);
    let mut builder = builder.add_fonts(|_| {});
    builder
        .services
        .add_singleton_as::<dyn AssetService>(assets as Arc<dyn AssetService>);
    add_host_fakes(builder)
}

#[component]
fn Root() -> impl IntoElement {
    build_app(RayXAppBuilder::new()).start();
    dynamic(move || match app_state() {
        AppState::Ready => {
            let root = app_services().unwrap();
            ROOT.with_borrow_mut(|kept| *kept = Some(root.clone()));
            *AT_READY.lock().unwrap() = Some(REGISTERED.load(Ordering::SeqCst));
            // A font registered while the app runs.
            let fonts = root.required_service::<dyn FontService>().unwrap();
            drop(spawn(async move {
                fonts.register_font(PACIFICO.to_vec()).await.unwrap();
            }));
            text("ready").into_element()
        }
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

fn pump(
    engine: &mut LocalEngine<CountingFonts>,
    rx: &Receiver<MountBatch>,
    until: impl Fn() -> bool,
) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        engine.pump();
        while rx.recv_timeout(Duration::from_millis(5)).is_ok() {}
        if until() {
            return;
        }
    }
    panic!("the condition never held");
}

#[test]
fn shipped_fonts_are_registered_once_and_run_time_fonts_by_the_service() {
    let mut set = Registry::ui_set("app/native");
    set.assets(FILES);
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut engine = LocalEngine::start(
        EngineConfig {
            assets: Some(Arc::new(AppAssets) as Arc<dyn Assets>),
            ui_sets: vec![set],
            ..EngineConfig::new(App::Rust(nsh_core::element::Root::of::<Root>()))
        },
        CountingFonts(ApproxTextEngine),
        ChannelSink(tx),
    )
    .unwrap();
    pump(&mut engine, &rx, || ROOT.with_borrow(Option::is_some));
    let root = ROOT.with_borrow(|root| root.clone()).unwrap();
    let fonts = root.required_service::<dyn FontService>().unwrap();

    // NSH registered the shipped font; the font service only read its family.
    assert_eq!(*AT_READY.lock().unwrap(), Some(1));
    assert_eq!(fonts.registered_fonts()[0].family, "Pacifico");
    assert!(fonts.diagnostics().is_empty(), "{:?}", fonts.diagnostics());

    // The font registered while the app runs went to NSH.
    pump(&mut engine, &rx, || fonts.registered_fonts().len() == 2);
    assert_eq!(REGISTERED.load(Ordering::SeqCst), 2);

    // One registered from another thread goes to NSH through the engine runner, while the
    // engine's thread pumps.
    let registering = {
        let fonts = Arc::clone(&fonts);
        std::thread::spawn(move || block_on(fonts.register_font(PACIFICO.to_vec())))
    };
    pump(&mut engine, &rx, || fonts.registered_fonts().len() == 3);
    assert_eq!(registering.join().unwrap().unwrap(), "Pacifico");
    assert_eq!(REGISTERED.load(Ordering::SeqCst), 3);
    assert!(
        fonts
            .registered_fonts()
            .iter()
            .all(|face| face.family == "Pacifico")
    );
}
