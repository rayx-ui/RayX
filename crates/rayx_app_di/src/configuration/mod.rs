//! Options configuration (RayX's `rayx_runtime::configuration`, options plans only).
//!
//! RayX layers configuration sources (files, environment, app settings, through
//! the `config` crate) under the options plans. Sources are platform-specific
//! (file system, bundled app content, browser), so the core keeps only the plans:
//! exact values (`options_value`) and default-based, configured and finalized
//! values (`options`, `configure_options`). The section name is kept for error
//! messages and conflict detection.

mod builder;
mod error;
mod options;

pub use builder::{ConfigurationBuildResult, ConfigurationBuilder};
pub use error::{ConfigurationError, OptionsFinalizeError, OptionsFinalizeStage};
