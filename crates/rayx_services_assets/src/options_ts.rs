//! The asset service's options for TSX (`addAssets`).

use nsh_core::prelude::*;
use rayx_app_nsh::ts::push;

use crate::AssetOptions as Assets;

/// The asset service's options (`addAssets`), as Rust's `AssetOptions` sets them: `package`
/// adds a package and returns the object, so calls chain.
#[class]
pub struct AssetOptions {
    packages: Vec<String>,
}

impl AssetOptions {
    pub(crate) fn new() -> Self {
        Self {
            packages: Vec::new(),
        }
    }

    pub(crate) fn take(&mut self) -> Vec<String> {
        std::mem::take(&mut self.packages)
    }

    pub(crate) fn apply(package: &str, options: &mut Assets) {
        options.package(package);
    }
}

#[class]
impl AssetOptions {
    /// Adds `package` (a crate) to the packages whose `assets/` directories hold the app's
    /// assets, the app's own first.
    pub fn package(this: Instance<Self>, package: String) -> Instance<Self> {
        push(this, |o| o.packages.push(package))
    }
}
