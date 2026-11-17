//! Translations: the app's languages and messages, published by an [`I18nService`]
//! implementation as [`I18nState`] (the user's language and locale, the languages the app has,
//! the time zone and the messages as resource bundles), and formatted by a [`MessageFormatter`]:
//! [MessageFormat 2](https://www.unicode.org/reports/tr35/tr35-messageFormat.html) messages,
//! numbers, dates and plurals for that locale, through [`dxn_i18n`] and [`dxn_intl`].
//!
//! The two are separate so that any implementation formats alike: `I18nService` is a contract
//! (its state and commands cross to TS, so a TS implementation can load messages from
//! anywhere), and `MessageFormatter` is Rust only and synchronous, for `t()` in render. RayX's
//! implementations of both are `rayx_services_i18n`'s (`add_i18n(..)`, or
//! `add_message_formatter()` over an app's own `dyn I18nService`).

use std::collections::BTreeMap;
use std::fmt;

use dxn_i18n::{Args, Formatted, Part, TranslateError};
use dxn_intl::{DateTime, DateTimeOptions, Locale, Number, NumberOptions, PluralCategory};
use serde::{Deserialize, Serialize};

use super::ContractFailure;
use super::platform::TextDirection;
use crate::published::StateCell;

/// The messages of one language: MessageFormat 2 sources by id (a namespace other than the
/// default one prefixes its ids: `checkout.title`).
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MessageBundle {
    /// The language's tag (`de`, `pt-BR`).
    pub language: String,
    /// The messages' sources, by id.
    pub messages: BTreeMap<String, String>,
}

/// The app's translations now: the user's language and locale, the languages the app has, the
/// time zone and the messages, with a version that counts the changes.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct I18nState {
    /// The locale's language tag (`de-AT`), without its Unicode extensions.
    pub language: String,
    /// The language the app picked ([`I18nService::set_language`]), or `None` while it follows
    /// the platform's languages.
    pub chosen_language: Option<String>,
    /// The locale messages and values are formatted for (`de-AT-u-hc-h23`).
    pub locale: String,
    /// The direction the locale's script is written in.
    pub direction: TextDirection,
    /// The language messages fall back to last.
    pub default_language: String,
    /// The languages tried, in order, after the user's and before the default one.
    pub fallback_languages: Vec<String>,
    /// The languages the app has messages for.
    pub available_languages: Vec<String>,
    /// The user's time zone (an IANA name such as `Europe/Zagreb`), which instants are shown in.
    pub time_zone: String,
    /// The messages, a bundle per language, in the order the languages came.
    pub messages: Vec<MessageBundle>,
    /// Counts the changes: a formatter compiles the messages again when it moves.
    pub version: u64,
}

/// Why an i18n call failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum I18nServiceError {
    /// The language tag does not read.
    InvalidLanguage(String),
    /// A resource of messages did not load whole.
    Resource { path: String, reason: String },
    /// Loading the asset catalog's messages needs an asset service, and none is registered.
    MissingAssetService,
    /// A value (or a time zone) could not be formatted.
    Format(String),
    /// The call failed (a TS implementation threw, the engine is gone).
    Failed(ContractFailure),
}

impl fmt::Display for I18nServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLanguage(tag) => write!(f, "`{tag}` is not a valid language tag"),
            Self::Resource { path, reason } => {
                write!(f, "the translations of `{path}` did not load: {reason}")
            }
            Self::MissingAssetService => {
                f.write_str("the i18n service needs the asset service to load translations")
            }
            Self::Format(reason) => write!(f, "the value could not be formatted: {reason}"),
            Self::Failed(failure) => failure.fmt(f),
        }
    }
}

impl std::error::Error for I18nServiceError {}

impl From<ContractFailure> for I18nServiceError {
    fn from(failure: ContractFailure) -> Self {
        Self::Failed(failure)
    }
}

/// The i18n service: the app's languages and messages, published as [`I18nState`], and the
/// commands that change them. Formatting is a [`MessageFormatter`]'s.
#[rayx_app_macros::contract(ts = "I18nServiceContract")]
pub trait I18nService {
    /// The languages, locale, time zone and messages now, and each change of them.
    #[state(ts = "I18nState")]
    fn i18n(&self) -> &StateCell<I18nState>;

    /// Shows the app in `language` from now on (`de`, `pt-BR`, `en-u-hc-h23`), whatever the
    /// platform reports later.
    async fn set_language(&self, language: String) -> Result<(), I18nServiceError>;

    /// Follows the platform's languages again after [`set_language`](Self::set_language).
    async fn follow_platform(&self) -> Result<(), I18nServiceError>;

    /// Loads the app's messages (RayX's implementation: every translation of the asset
    /// catalog, which it does at startup unless its options turn that off). The ones that load
    /// are added; the error names the first that did not.
    async fn load_messages(&self) -> Result<(), I18nServiceError>;

    /// Adds the messages of a JSON resource (an object of message sources, nested objects
    /// making dotted ids) for `language`, in `namespace` (the default one when `null`),
    /// replacing messages with the same ids. The messages that parse are added even when
    /// others do not.
    async fn add_resource(
        &self,
        language: String,
        namespace: Option<String>,
        json: String,
    ) -> Result<(), I18nServiceError>;

    /// Shows instants in `time_zone` (an IANA name) from now on, until the platform reports
    /// another.
    async fn set_time_zone(&self, time_zone: String) -> Result<(), I18nServiceError>;
}

impl dyn I18nService {
    /// The locale's language tag (`de-AT`), without its Unicode extensions.
    pub fn language(&self) -> String {
        self.i18n().get().language.clone()
    }

    /// The language the app picked, or `None` while it follows the platform's languages.
    pub fn chosen_language(&self) -> Option<String> {
        self.i18n().get().chosen_language.clone()
    }

    /// The locale messages and values are formatted for (`de-AT-u-hc-h23`).
    pub fn locale(&self) -> String {
        self.i18n().get().locale.clone()
    }

    /// The direction the locale's script is written in.
    pub fn direction(&self) -> TextDirection {
        self.i18n().get().direction
    }

    /// The languages the app has messages for.
    pub fn available_languages(&self) -> Vec<String> {
        self.i18n().get().available_languages.clone()
    }

    /// The user's time zone (an IANA name), which instants are shown in.
    pub fn time_zone(&self) -> String {
        self.i18n().get().time_zone.clone()
    }

    /// Counts the changes.
    pub fn version(&self) -> u64 {
        self.i18n().get().version
    }
}

/// Formats the app's messages and values for the [`I18nService`]'s locale, synchronously, in
/// Rust (render's `t()`, `tr!`): RayX's MessageFormat 2 and ICU formatting over whichever
/// `dyn I18nService` is registered, following its changes on the next call.
pub trait MessageFormatter: Send + Sync {
    /// The locale messages and values are formatted for now.
    fn locale(&self) -> Locale;

    /// Whether a language in the lookup chain has the message `id`.
    fn contains(&self, id: &str) -> bool;

    /// The message `id` without arguments.
    fn t(&self, id: &str) -> String {
        self.format(id, &Args::new())
    }

    /// The message `id` formatted with `args`; what fails shows as its fallback (`{$name}`), a
    /// missing message as the formatter's policy says (its id, by default).
    fn format(&self, id: &str, args: &Args) -> String;

    /// The message `id` formatted with `args`, or why it could not be formatted whole.
    fn try_format(&self, id: &str, args: &Args) -> Result<String, TranslateError>;

    /// The message `id` as text, values and markup, for rich text; `None` when it is missing.
    fn format_to_parts(&self, id: &str, args: &Args) -> Option<Formatted<Vec<Part>>>;

    /// `number` formatted for the locale.
    fn format_number(
        &self,
        number: Number,
        options: NumberOptions,
    ) -> Result<String, I18nServiceError>;

    /// `value` formatted for the locale, in its calendar and clock.
    fn format_date_time(
        &self,
        value: &DateTime,
        options: DateTimeOptions,
    ) -> Result<String, I18nServiceError>;

    /// `number`'s cardinal plural category in the locale.
    fn plural_category(&self, number: Number) -> PluralCategory;

    /// The wall time in the user's time zone at `instant`, an RFC 3339 timestamp with its
    /// offset (`2026-11-02T09:30:00Z`, as JavaScript's `toISOString()` writes one), to format
    /// with [`format_date_time`](Self::format_date_time) or pass as a message's date.
    fn local_time(&self, instant: &str) -> Result<DateTime, I18nServiceError>;

    /// The wall time in the user's time zone `millis` milliseconds after the Unix epoch.
    fn local_time_from_millis(&self, millis: i64) -> Result<DateTime, I18nServiceError>;
}

/// `dxn_i18n::t!` formats with a formatter, as with a `Translator`: `t!(formatter,
/// "cart.items", count)`, from a reference or an `Arc` as well. It is
/// [`MessageFormatter::format`].
impl dxn_i18n::Translate for dyn MessageFormatter {
    fn translate(&self, id: &str, args: &Args) -> String {
        self.format(id, args)
    }
}
