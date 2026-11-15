//! A RayX app made by `rayx new`, with the dev tools it writes, checked and tested against this
//! checkout (LAYER-2), ignored by default: it installs rayx's default nsh into a temporary
//! `RAYX_HOME` and builds the app, which needs the network. `rayx new` pins `rayx` at this `rayx`'s
//! release tag, which cannot hold this work before its release, so the proof writes a test-only
//! path override of the RayX crates into the generated app, in its temporary directory.
//!
//! ```text
//! cargo test -p rayx-cli --test proof_new_devtools -- --ignored --test-threads=1
//! ```

mod common;

use common::Workspace;
use common::proof::{override_rayx, rayx_ok};

#[test]
#[ignore = "installs nsh and builds a RayX app (network, many minutes)"]
fn a_new_app_with_the_dev_tools_builds_and_passes_its_test() {
    let home = Workspace::empty();
    let work = Workspace::empty();
    rayx_ok(&home.root, &work.root, &["new", "hello"]);
    let app = work.path("hello");
    let lib = std::fs::read_to_string(app.join("src/lib.rs")).expect("lib.rs");
    assert!(lib.contains(".register::<DevLayer>()"), "{lib}");
    override_rayx(&app);
    rayx_ok(&home.root, &app, &["app", "check"]);
    rayx_ok(&home.root, &app, &["app", "test"]);
}
