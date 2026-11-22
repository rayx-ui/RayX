//! TS `builder.restart()`, headless: the app started afresh on its surface with another builder,
//! as a design tool does when the app's composition changed while it runs. The old root and every
//! scope made from it with `createScope` are disposed (using them throws), `appState()` goes
//! through `starting` to `ready` again (or `failed`), and the new root resolves the new builder's
//! registrations; `start()` then gives the new root. A startup under way ends first; a builder
//! started already is refused and changes nothing; on a surface where nothing started it starts;
//! a failed restart can be followed by another.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod headless;

const RESTART: &str = r#"import { effect, mount, signal, Text } from "nsh";
import { RayXAppBuilder, ServiceToken, appState } from "test/app";

const shown = signal("waiting");
const lines = [];
const seen = [];
const state = appState();
effect(() => {
  const now = state();
  seen.push(now.state === "failed" ? "failed" : now.state);
});
const thrown = (f) => { try { f(); return "used"; } catch (e) { return String(e.message ?? e).includes("disposed") ? "disposed" : `threw ${e}`; } };

const Clock = new ServiceToken("Clock");
class Fixed { constructor(n) { this.n = n; } }
class Slow {
  constructor(fail) { this.done = false; this.fail = fail; }
  isInitialized() { return this.done; }
  async initialize() {
    await new Promise((resolve) => setTimeout(resolve, 20));
    if (this.fail) throw new Error("the slow service broke");
    this.done = true;
  }
}
const app = (n, fail = false) =>
  new RayXAppBuilder().addHostFakes()
    .addSingletonAs(Clock, () => new Fixed(n))
    .addStartupSingleton(Slow, () => new Slow(fail));

setTimeout(async () => {
  try {
    CASE
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  setTimeout(() => shown.set(`done ${lines.join("|")} | ${seen.join(",")}`), 20);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

fn run(case: &str) -> String {
    headless::run(headless::set(|_| {}), &RESTART.replace("CASE", case))
}

#[test]
fn a_restart_disposes_the_old_root_and_its_scopes_and_starts_the_new_builder() {
    let shown = run(r#"
    const old = await app(1).start();
    const scope = old.createScope();
    const inner = scope.createScope((s) => s.addSingletonAs(Clock, () => new Fixed(9)));
    const gone = old.createScope();
    gone.dispose();
    lines.push(`before ${old.getRequiredService(Clock).n} ${inner.getRequiredService(Clock).n}`);
    const fresh = await app(2).restart();
    lines.push(`new ${fresh !== old} ${fresh.getRequiredService(Clock).n}`);
    lines.push(`old ${thrown(() => old.getService(Clock))} ${thrown(() => scope.getService(Clock))} ${thrown(() => inner.getService(Clock))}`);
    const again = await app(3).start();
    lines.push(`start gives it ${again === fresh} ${again.getRequiredService(Clock).n}`);
    "#);
    assert_eq!(
        shown,
        "done before 1 9|new true 2|old disposed disposed disposed|start gives it true 2 \
         | notStarted,starting,ready,starting,ready"
    );
}

#[test]
fn a_failed_restart_says_so_and_another_can_follow() {
    let shown = run(r#"
    const old = await app(1).start();
    try {
      await app(2, true).restart();
      lines.push("restarted");
    } catch (error) {
      lines.push(`rejected ${String(error).includes("the slow service broke")}`);
    }
    lines.push(`old ${thrown(() => old.getService(Clock))}`);
    const fresh = await app(4).restart();
    lines.push(`then ${fresh.getRequiredService(Clock).n}`);
    "#);
    assert_eq!(
        shown,
        "done rejected true|old disposed|then 4 \
         | notStarted,starting,ready,starting,failed,starting,ready"
    );
}

#[test]
fn a_startup_under_way_ends_first() {
    let shown = run(r#"
    const first = app(1).start();
    const fresh = await app(2).restart();
    const old = await first;
    lines.push(`first ${thrown(() => old.getService(Clock))}`);
    lines.push(`new ${fresh.getRequiredService(Clock).n}`);
    "#);
    assert_eq!(
        shown,
        "done first disposed|new 2 | notStarted,starting,ready,starting,ready"
    );
}

#[test]
fn a_started_builder_is_refused_and_nothing_changes() {
    let shown = run(r#"
    const builder = app(1);
    const root = await builder.start();
    try {
      await builder.restart();
      lines.push("restarted");
    } catch (error) {
      lines.push(`refused ${error}`);
    }
    lines.push(`root ${root.getRequiredService(Clock).n}`);
    "#);
    assert_eq!(
        shown,
        "done refused Error: the RayXAppBuilder has started already: make a new one|root 1 \
         | notStarted,starting,ready"
    );
}

#[test]
fn on_a_surface_where_nothing_started_it_starts() {
    let shown = run(r#"
    const root = await app(5).restart();
    lines.push(`root ${root.getRequiredService(Clock).n}`);
    "#);
    assert_eq!(shown, "done root 5 | notStarted,starting,ready");
}

#[test]
fn the_declarations_type_restart() {
    let set = headless::set(|_| {});
    let ts = headless::declarations(&set);
    assert!(
        ts.contains("  restart(): Promise<ServiceProvider>;"),
        "no restart in\n{ts}"
    );
    assert_eq!(set.undocumented(), Vec::<String>::new());
}
