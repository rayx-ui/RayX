//! Typed, immutable options resolved by type (RayX's `rayx_runtime::options`).

mod error;
mod registry;
mod value;

pub use error::OptionsError;
pub use registry::OptionsRegistry;
pub use value::Options;
