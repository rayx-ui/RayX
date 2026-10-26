//! Options (.NET's `Microsoft.Extensions.Options`): typed settings made by a pipeline of steps
//! registered on the [`ServiceProvider`](crate::di::ServiceProvider), read by type and name.
//!
//! ```
//! use rayx_core::configuration::ConfigurationBuilder;
//! use rayx_core::{RayXAppBuilder, ServiceAppBuilderExt};
//! use serde_json::json;
//!
//! #[derive(Default, serde::Serialize, serde::Deserialize)]
//! struct FeedOptions {
//!     label: String,
//!     retry_limit: u32,
//! }
//!
//! let mut app = RayXAppBuilder::new();
//! app.configuration.add_value(json!({ "Feed": { "label": "live", "retryLimit": 3 } }));
//! app.services
//!     .add_options::<FeedOptions>()
//!     .bind("Feed")
//!     .post_configure(|options| options.retry_limit = options.retry_limit.min(5))
//!     .validate(|options| !options.label.is_empty(), "the feed needs a label")
//!     .validate_on_start();
//! let services = app.build()?;
//!
//! let options = services.options::<FeedOptions>()?;
//! assert_eq!((options.label.as_str(), options.retry_limit), ("live", 3));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! - [`ServiceProvider::options`](crate::di::ServiceProvider::options) (.NET's `IOptions<T>`):
//!   the unnamed options, made once.
//! - [`OptionsSnapshot`] (`IOptionsSnapshot<T>`): made once per scope, by name.
//! - [`OptionsMonitor`] (`IOptionsMonitor<T>`): by name, made again when the configuration a
//!   name is bound to changes, with change listeners.
//!
//! Options with no registration are `T::default()`.

mod access;
mod builder;
mod error;
mod pipeline;
mod value;

pub use access::{OptionsMonitor, OptionsSnapshot};
pub use builder::OptionsBuilder;
pub use error::OptionsError;
pub(crate) use pipeline::OptionsRegistrations;
pub use value::Options;

pub use crate::change::ChangeSubscription;

/// The name of the unnamed options (.NET's `Options.DefaultName`).
pub const DEFAULT_NAME: &str = "";
