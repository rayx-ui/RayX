//! `useServices`, `useScreenScope` and `provideServices` from TSX components, headless, with Rust
//! components under them and TSX components under a Rust screen.
//!
//! Its first test proves what the hooks stand on: a Rust function called from a TSX component's
//! body provides and reads a context on that component's owner and registers a cleanup there,
//! through NSH's `CallCx::with_owner`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod headless;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use nsh_core::prelude::*;
use rayx_app_nsh::ts::{RayXAppBuilder, ServiceProvider};
use rayx_app_nsh::{
    ActivatableViewModel, AppService, ServiceAppBuilderExt, ViewModelActivator, services,
    use_screen_scope,
};

thread_local! {
    static PROBE_LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone)]
struct Label(String);

/// The probe: context and cleanup from a TSX component's body, through Rust.
#[module]
struct Owners;

#[module]
impl Owners {
    /// Provides `label` on the calling component's owner.
    pub(crate) fn provide(label: String, cx: CallCx) -> String {
        cx.with_owner(|| provide_context(Label(label)));
        "provided".into()
    }

    /// The nearest label provided above the calling component.
    pub(crate) fn read(cx: CallCx) -> String {
        cx.with_owner(|| use_context::<Label>().map_or_else(|| "none".into(), |label| label.0))
    }

    /// Logs `label` when the calling component's owner cleans up (Rust's graph).
    pub(crate) fn rust_cleanup(label: String, cx: CallCx) -> String {
        cx.with_owner(|| {
            cleanup(move || PROBE_LOG.with_borrow_mut(|log| log.push(format!("rust {label}"))));
        });
        "registered".into()
    }

    /// Logs `label` when the calling component's owner cleans up (the call's JS owner).
    pub(crate) fn js_cleanup(label: String, cx: CallCx) {
        drop(cx.on_cleanup(move || {
            PROBE_LOG.with_borrow_mut(|log| log.push(format!("js {label}")));
        }));
    }

    /// What the cleanups logged.
    pub(crate) fn log() -> String {
        PROBE_LOG.with_borrow(|log| log.join(","))
    }
}

const PROBE: &str = r#"import { mount, signal, Text, View } from "nsh";
import { Owners } from "test/app";

const shown = signal("waiting");
const lines = [];
const open = signal(true);

function Inner() {
  lines.push(`inner ${Owners.read()}`);
  return <Text>inner</Text>;
}

function Outer() {
  lines.push(`provide ${Owners.provide("outer")}`);
  lines.push(`rust cleanup ${Owners.rustCleanup("outer")}`);
  Owners.jsCleanup("outer");
  lines.push(`outer ${Owners.read()}`);
  return <Inner />;
}

function Sibling() {
  lines.push(`sibling ${Owners.read()}`);
  return <Text>sibling</Text>;
}

setTimeout(() => {
  open.set(false);
  setTimeout(() => shown.set(`done ${lines.join("|")}|log ${Owners.log()}`), 0);
}, 0);

mount(() => (
  <View>
    <If when={open()}>
      <Outer />
    </If>
    <If when={open()}>
      <Sibling />
    </If>
    <Text>{shown()}</Text>
  </View>
));
"#;

#[test]
fn a_rust_function_called_from_a_tsx_body_provides_reads_and_cleans_up_on_its_owner() {
    let set = headless::set(|set| {
        set.module::<Owners>();
    });
    assert_eq!(
        headless::run(set, PROBE),
        "done provide provided|rust cleanup registered|outer outer|inner outer|sibling none\
         |log js outer,rust outer"
    );
}

// --- The hooks ----------------------------------------------------------------------------

static DROPPED: AtomicUsize = AtomicUsize::new(0);
static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A scoped service: one per scope, counted when it goes.
#[derive(AppService)]
struct PageState {
    id: usize,
}

impl Drop for PageState {
    fn drop(&mut self) {
        DROPPED.fetch_add(1, Ordering::SeqCst);
    }
}

/// A scoped view model.
#[derive(AppService)]
struct PageViewModel {
    activator: ViewModelActivator,
    _page: Arc<PageState>,
}

impl ActivatableViewModel for PageViewModel {
    fn activator(&self) -> &ViewModelActivator {
        &self.activator
    }
}

thread_local! {
    /// The view model `Pages.activate` activated last.
    static VIEW_MODEL: RefCell<Option<Arc<PageViewModel>>> = const { RefCell::new(None) };
    /// The page id each Rust component's `services()` saw, by name.
    static RUST_PAGES: RefCell<BTreeMap<String, usize>> = const { RefCell::new(BTreeMap::new()) };
}

/// The page id of the scope `services` is (resolved there now).
fn page_in(services: &rayx_app_nsh::ServiceProvider) -> usize {
    services.required_service::<PageState>().unwrap().id
}

/// The tests' services and what they check of them.
#[module]
struct Pages;

#[module]
impl Pages {
    /// Adds the tests' services to `builder`.
    pub(crate) fn add_pages(
        builder: Instance<RayXAppBuilder>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        builder
            .with_mut(|builder| {
                builder.extend(|builder| {
                    builder
                        .add_scoped(|_| PageState {
                            id: NEXT.fetch_add(1, Ordering::SeqCst),
                        })
                        .add_scoped(|provider| PageViewModel {
                            activator: ViewModelActivator::default(),
                            _page: provider.required_service::<PageState>().unwrap(),
                        })
                })
            })
            .map_err(|error| error.to_string())?;
        Ok(builder)
    }

    /// The page id of `services`' scope.
    pub(crate) fn page_of(services: Instance<ServiceProvider>) -> u32 {
        services.with(|services| page_in(services.services())) as u32
    }

    /// Activates `services`' view model until it is disposed, keeping it.
    pub(crate) fn activate(services: Instance<ServiceProvider>) -> Result<(), String> {
        let view_model = services
            .with_mut(|services| services.view_model::<PageViewModel>())
            .map_err(|error| error.to_string())?;
        VIEW_MODEL.with_borrow_mut(|kept| *kept = Some(view_model));
        Ok(())
    }

    /// Whether the view model `activate` activated last is active.
    pub(crate) fn active() -> bool {
        VIEW_MODEL.with_borrow(|kept| {
            kept.as_ref()
                .is_some_and(|view_model| view_model.activator().is_active())
        })
    }

    /// How many page states went so far.
    pub(crate) fn dropped() -> u32 {
        DROPPED.load(Ordering::SeqCst) as u32
    }

    /// The page id the Rust component named `name` saw.
    pub(crate) fn rust_page(name: String) -> i32 {
        RUST_PAGES.with_borrow(|pages| pages.get(&name).map_or(-1, |&id| id as i32))
    }
}

/// Notes the page id its `services()` gives, under its name.
#[derive(Debug, Clone, PartialEq, Props)]
struct RustScopeProps {
    /// What it notes the page under.
    #[prop(default)]
    name: Prop<SharedString>,
}

/// A Rust component under a TSX screen ([`RustScopeProps`]).
#[component]
fn RustScope(props: RustScopeProps) -> impl IntoElement {
    let page = services().map(|services| page_in(&services));
    if let Ok(page) = page {
        RUST_PAGES
            .with_borrow_mut(|pages| pages.insert(props.name.get_untracked().to_string(), page));
    }
    view()
}

/// A Rust screen whose TSX children are made once it places them.
#[derive(Debug, Clone, PartialEq, Props)]
struct RustScreenProps {
    /// Made once it places them.
    #[prop(lazy)]
    children: Children,
}

/// A Rust screen ([`RustScreenProps`]): its own scope, noted as `rust screen`.
#[component]
fn RustScreen(props: RustScreenProps) -> impl IntoElement {
    if let Ok(scope) = use_screen_scope()
        && let Some(services) = scope.services()
    {
        let page = page_in(&services);
        RUST_PAGES.with_borrow_mut(|pages| pages.insert("rust screen".into(), page));
    }
    view().child(props.children)
}

const HOOKS: &str = r#"import { mount, signal, Text, View } from "nsh";
import {
  GreetingService, Pages, RayXAppBuilder, RustScope, RustScreen, provideServices,
  useScreenScope, useServices,
} from "test/app";

const shown = signal("waiting");
const lines = [];
const starting = signal(false);
const ready = signal(false);
const open = signal(true);
let root;
let provided;
const pages = {};
const thrown = (f) => { try { f(); return "no throw"; } catch (e) { return e.message; } };

class Slow {
  constructor() { this.done = false; }
  isInitialized() { return this.done; }
  async initialize() {
    await new Promise((resolve) => setTimeout(resolve, 100));
    this.done = true;
  }
}

function Early(props) {
  lines.push(`${props.name} ${thrown(() => useServices())}`);
  return <Text>{props.name}</Text>;
}

function AtRoot() {
  const services = useServices();
  lines.push(`root ${services === root} ${services === useServices()}`);
  return <Text>root</Text>;
}

function Under(props) {
  const services = useServices();
  const page = Pages.pageOf(services);
  pages[props.name] = page;
  const greeting = services.getRequiredService(GreetingService);
  lines.push(`${props.name} ${services === props.screen} ${greeting === props.screen.getRequiredService(GreetingService)}`);
  return <RustScope name={props.name} />;
}

function Inner(props) {
  const screen = useScreenScope();
  lines.push(`nested ${screen !== props.outer} ${Pages.pageOf(screen) !== Pages.pageOf(props.outer)}`);
  return <Under name="inner" screen={screen} />;
}

function Screen(props) {
  const screen = useScreenScope();
  pages[`${props.name} screen`] = Pages.pageOf(screen);
  if (props.nested) Pages.activate(screen);
  return (
    <View>
      <Under name={`${props.name} a`} screen={screen} />
      <Under name={`${props.name} b`} screen={screen} />
      <If when={props.nested}>
        <Inner outer={screen} />
      </If>
    </View>
  );
}

function InRustScreen() {
  pages["in rust screen"] = Pages.pageOf(useServices());
  return <RustScope name="in rust screen" />;
}

function Provided() {
  provided = root.createScope();
  provideServices(provided);
  return <ProvidedChild />;
}

function ProvidedChild() {
  lines.push(`provided ${useServices() === provided}`);
  return <RustScope name="provided" />;
}

const builder = Pages.addPages(new RayXAppBuilder().addHostFakes().addGreeting())
  .addStartupSingleton(Slow, () => new Slow());

setTimeout(async () => {
  try {
    const started = builder.start();
    setTimeout(() => starting.set(true), 20);
    root = await started;
    ready.set(true);
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  setTimeout(() => {
    lines.push(`outside ${thrown(() => useServices())}`);
    lines.push(`active ${Pages.active()}`);
    const same = (a, b) => pages[a] === pages[b] && pages[a] === Pages.rustPage(b);
    lines.push(`screens ${same("first screen", "first a")} ${same("first screen", "first b")} ${same("second screen", "second a")} ${pages["first screen"] !== pages["second screen"]} ${pages.inner === Pages.rustPage("inner") && pages.inner !== pages["first screen"]}`);
    lines.push(`rust screen ${pages["in rust screen"] === Pages.rustPage("rust screen") && pages["in rust screen"] === Pages.rustPage("in rust screen")}`);
    lines.push(`provided page ${Pages.rustPage("provided") === Pages.pageOf(provided)}`);
    const before = Pages.dropped();
    open.set(false);
    setTimeout(() => {
      lines.push(`closed ${Pages.dropped() - before} ${Pages.active()} ${provided.getService(GreetingService) !== null}`);
      shown.set(`done ${lines.join("|")}`);
    }, 50);
  }, 50);
}, 0);

mount(() => (
  <View>
    <Early name="not started" />
    <If when={starting() && !ready()}>
      <Early name="starting" />
    </If>
    <If when={ready()}>
      <AtRoot />
      <If when={open()}>
        <Screen name="first" nested={true} />
      </If>
      <If when={open()}>
        <Screen name="second" nested={false} />
      </If>
      <If when={open()}>
        <RustScreen>
          <InRustScreen />
        </RustScreen>
      </If>
      <If when={open()}>
        <Provided />
      </If>
    </If>
    <Text>{shown()}</Text>
  </View>
));
"#;

#[test]
fn screens_in_tsx_and_rust_share_their_scopes() {
    let set = headless::set(|set| {
        set.module::<Pages>()
            .register::<RustScope>()
            .register::<RustScreen>();
        headless::greeting::register(set);
    });
    assert_eq!(
        headless::run(set, HOOKS),
        "done not started the app has not started on this surface: start its RayXAppBuilder \
         (`builder.start()`)\
         |starting the app is still starting on this surface\
         |root true true\
         |first a true true|first b true true|nested true true|inner true true\
         |second a true true|second b true true\
         |provided true\
         |outside a RayX hook is called in a component's body: this call has no reactive owner \
         (a timer's or an event handler's own code, a module's top level)\
         |active true\
         |screens true true true true true\
         |rust screen true\
         |provided page true\
         |closed 3 false true"
    );
}

#[test]
fn the_declarations_type_the_hooks() {
    let set = headless::set(|_| {});
    let ts = headless::declarations(&set);
    for declared in [
        "export declare function useServices(): ServiceProvider;",
        "export declare function useScreenScope(): ServiceProvider;",
        "export declare function provideServices(provider: ServiceProvider): void;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
    assert_eq!(set.undocumented(), Vec::<String>::new());
}

// --- appState -----------------------------------------------------------------------------

const APP_STATE: &str = r#"import { effect, mount, signal, Text } from "nsh";
import { RayXAppBuilder, appState } from "test/app";

const shown = signal("waiting");
const state = appState();
const seen = [];
effect(() => {
  const now = state();
  seen.push(now.state === "failed" ? `failed: ${now.message}` : now.state);
});

class Slow {
  constructor() { this.done = false; }
  isInitialized() { return this.done; }
  async initialize() {
    await new Promise((resolve) => setTimeout(resolve, 20));
    if (FAIL) throw new Error("the slow service broke");
    this.done = true;
  }
}

setTimeout(async () => {
  const same = appState() === state;
  try {
    await new RayXAppBuilder().addHostFakes().addStartupSingleton(Slow, () => new Slow()).start();
  } catch (error) {}
  setTimeout(() => shown.set(`done ${same} ${seen.join("|")}`), 20);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

/// `appState()` follows the app's start on its surface, the same signal every time; a new
/// surface (a hot restart, another window) starts from `notStarted` again.
#[test]
fn app_state_follows_the_start_on_each_surface() {
    let ready = APP_STATE.replace("FAIL", "false");
    assert_eq!(
        headless::run(headless::set(|_| {}), &ready),
        "done true notStarted|starting|ready"
    );
    let failing = APP_STATE.replace("FAIL", "true");
    let shown = headless::run(headless::set(|_| {}), &failing);
    assert!(
        shown.starts_with("done true notStarted|starting|failed: the app did not start: ")
            && shown.contains("the slow service broke"),
        "{shown}"
    );
}

#[test]
fn the_declarations_type_app_state() {
    let set = headless::set(|_| {});
    let ts = headless::declarations(&set);
    for declared in [
        "export type AppState = { state: \"notStarted\" | \"starting\" | \"ready\" } | { state: \"failed\"; message: string };",
        "export declare function appState(): import(\"nsh\").Accessor<AppState>;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
}

const RUST_SCREEN_FIRST: &str = r#"import { mount, signal, Text, View } from "nsh";
import { Pages, RayXAppBuilder, RustScreen, useServices } from "test/app";

const shown = signal("waiting");
let seen = -1;

function InRustScreen() {
  seen = Pages.pageOf(useServices());
  return <Text>in rust screen</Text>;
}

setTimeout(async () => {
  await Pages.addPages(new RayXAppBuilder().addHostFakes()).start();
  mount(() => (
    <View>
      <RustScreen>
        <InRustScreen />
      </RustScreen>
      <Text>{shown()}</Text>
    </View>
  ));
  setTimeout(() => shown.set(`done ${seen !== -1 && seen === Pages.rustPage("rust screen")}`), 50);
}, 0);
"#;

/// A Rust screen mounted before any hook ran: the first `useServices()`, in its lazy TSX
/// children, finds the screen's scope, not the surface root.
#[test]
fn a_rust_screen_mounted_before_any_hook_gives_its_lazy_children_its_scope() {
    let set = headless::set(|set| {
        set.module::<Pages>().register::<RustScreen>();
    });
    assert_eq!(headless::run(set, RUST_SCREEN_FIRST), "done true");
}
