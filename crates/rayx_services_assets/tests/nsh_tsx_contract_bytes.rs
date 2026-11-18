//! Bytes from an asset service written in TS: `loadBytes` may answer a `Uint8Array`, an
//! `ArrayBuffer` or a `number[]`, and Rust's `load_bytes` reads the same bytes from each; what
//! the set declares for it says so.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use nsh_core::prelude::*;
use rayx_app::services::assets::AssetService;
use rayx_app_nsh::testing;
use rayx_app_nsh::ts::ServiceProvider;

/// What the test does with the TS implementation from Rust.
#[module]
struct BytesProbe;

#[module]
impl BytesProbe {
    /// The bytes of `path`, as `Rust` read them, or the error.
    pub(crate) async fn load(services: Instance<ServiceProvider>, path: String) -> String {
        let assets: Arc<dyn AssetService> = services
            .with(|services| services.services().required_service::<dyn AssetService>())
            .unwrap();
        match assets.load_bytes(path).await {
            Ok(bytes) => format!("{bytes:?}"),
            Err(error) => format!("error {error}"),
        }
    }
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { AssetService, BytesProbe, ContractState, RayXAppBuilder } from "test/app";

class MyAssets {
  catalog = new ContractState({ packages: [], entries: {}, diagnostics: [] });
  async loadCatalog() {}
  async loadBytes(path) {
    if (path === "typed") return new Uint8Array([1, 2, 255]);
    if (path === "buffer") return new Uint8Array([4, 5]).buffer;
    if (path === "view") return new Uint16Array([258]);
    if (path === "empty") return new Uint8Array(0);
    if (path === "array") return [6, 7];
    throw new Error(`no asset ${path}`);
  }
  async loadText(path) { throw new Error("unused"); }
  clearCache() {}
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder()
      .addSingletonAs(AssetService, () => new MyAssets())
      .addHostFakes()
      .start();
    for (const path of ["typed", "buffer", "view", "empty", "array"]) {
      lines.push(await BytesProbe.load(services, path));
    }
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn rust_reads_the_bytes_ts_answers_as_a_typed_array_a_buffer_or_an_array() {
    let set = testing::set(|set| {
        rayx_services_assets::register(set);
        set.module::<BytesProbe>();
    });
    let shown = testing::run(set, TSX);
    // A `Uint16Array` is its bytes, in the platform's (little-endian) order.
    assert_eq!(shown, "done [1, 2, 255]|[4, 5]|[2, 1]|[]|[6, 7]", "{shown}");
}

#[test]
fn the_set_declares_both_for_load_bytes() {
    let set = testing::set(rayx_services_assets::register);
    let declarations = testing::declarations(&set);
    assert!(
        declarations.contains("loadBytes(path: string): Promise<number[] | Uint8Array>;"),
        "{declarations}"
    );
}
