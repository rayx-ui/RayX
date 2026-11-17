//! Configuration (.NET's `Microsoft.Extensions.Configuration`): sources merged into one tree of
//! values, read by path and bound onto options.
//!
//! The core has the sources every target can load: values in memory, a tree, JSON text, and
//! [`ConfigurationSource`] for the rest (files, the app's bundled settings, a server), which may
//! load asynchronously.

mod builder;
mod error;
mod root;
mod source;
pub(crate) mod tree;

pub use builder::ConfigurationBuilder;
pub use error::ConfigurationError;
pub use root::{Configuration, ConfigurationSection};
pub use serde_json::Value;
pub use source::{ConfigurationSource, SourceLoad};
