//! Translations: the app's MessageFormat 2 messages in the asset catalog's `locales/`, looked up
//! for the user's languages with fallbacks, through [`dxn_i18n`]; numbers, dates and plurals
//! formatted for the same locale through [`dxn_intl`].

mod app_ext;
mod options;
mod service;

pub use app_ext::I18nAppBuilderExt;
pub use options::{I18N_CONFIG_PATH, I18nOptions, MissingMessagePolicy};
pub use service::{I18nChangedEvent, I18nService, I18nServiceError};

pub use dxn_i18n::{Args, Value};
