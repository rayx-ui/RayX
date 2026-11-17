//! RayX's message formatter: the [`I18nService`]'s messages compiled per language into a
//! [`Translator`], formatted for its locale with [`dxn_i18n`] and [`dxn_intl`], compiled again
//! when the service publishes a new version.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use dxn_i18n::{
    Args, BidiIsolation, Bundle, Formatted, MissingMessage, Part, TranslateError, Translator,
};
use dxn_intl::{
    DateTime, DateTimeFormatter, DateTimeOptions, Locale, Number, NumberFormatter, NumberOptions,
    PluralCategory, PluralKind, PluralRules, parse_locale,
};
use futures::FutureExt as _;
use futures::future::LocalBoxFuture;
use rayx_app::di::{
    AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupServiceType,
};
use rayx_app::services::i18n::{I18nService, I18nServiceError, I18nState, MessageFormatter};

use crate::options::{I18nOptions, MissingMessagePolicy};

/// The messages of one version of the service's state, compiled.
struct Compiled {
    version: u64,
    translator: Arc<Translator>,
    time_zone: String,
}

/// RayX's [`MessageFormatter`] over whichever `dyn I18nService` is registered: it compiles the
/// service's message bundles on the first call after each new version and formats with them.
pub(crate) struct RayxFormatter {
    i18n: Arc<dyn I18nService>,
    missing: MissingMessage,
    bidi: BidiIsolation,
    compiled: Mutex<Option<Compiled>>,
    /// Whether its startup began following the service on NSH.
    following: AtomicBool,
}

impl RayxFormatter {
    /// A formatter over `i18n`, with `options`' missing-message policy and bidi isolation.
    pub(crate) fn new(i18n: Arc<dyn I18nService>, options: &I18nOptions) -> Arc<Self> {
        Arc::new(Self {
            i18n,
            missing: match options.missing_message {
                MissingMessagePolicy::Id => MissingMessage::Id,
                MissingMessagePolicy::Empty => MissingMessage::Empty,
            },
            bidi: if options.bidi_isolation {
                BidiIsolation::Default
            } else {
                BidiIsolation::None
            },
            compiled: Mutex::default(),
            following: AtomicBool::new(false),
        })
    }

    /// The service the formatter follows.
    fn i18n(&self) -> &Arc<dyn I18nService> {
        &self.i18n
    }

    /// The translator of the service's version now, and its time zone: the cached one, or one
    /// compiled now when the service published a new version (the old one is dropped).
    fn current(&self) -> (Arc<Translator>, String) {
        let state = self.i18n.i18n().get();
        let mut compiled = self.compiled.lock().unwrap_or_else(PoisonError::into_inner);
        match &*compiled {
            Some(cached) if cached.version == state.version => {
                (Arc::clone(&cached.translator), cached.time_zone.clone())
            }
            _ => {
                let translator = Arc::new(self.compile(&state));
                *compiled = Some(Compiled {
                    version: state.version,
                    translator: Arc::clone(&translator),
                    time_zone: state.time_zone.clone(),
                });
                (translator, state.time_zone.clone())
            }
        }
    }

    /// `state`'s bundles in a translator at its locale, with its fallbacks.
    fn compile(&self, state: &I18nState) -> Translator {
        let default = parse_locale(&state.default_language)
            .or_else(|_| parse_locale("en"))
            .unwrap_or(Locale::UNKNOWN);
        let mut translator = Translator::new(default.clone());
        translator.set_fallbacks(
            state
                .fallback_languages
                .iter()
                .filter_map(|tag| parse_locale(tag).ok())
                .collect(),
        );
        translator.set_missing_message(self.missing);
        translator.set_bidi_isolation(self.bidi);
        for messages in &state.messages {
            let Ok(locale) = parse_locale(&messages.language) else {
                continue;
            };
            let mut bundle = Bundle::new(locale);
            for (id, source) in &messages.messages {
                // A message that does not parse was refused when it was added.
                let _ = bundle.add(id.clone(), source);
            }
            translator.add_bundle(bundle);
        }
        translator.set_locale(parse_locale(&state.locale).unwrap_or(default));
        translator
    }
}

impl MessageFormatter for RayxFormatter {
    fn locale(&self) -> Locale {
        self.current().0.locale().clone()
    }

    fn contains(&self, id: &str) -> bool {
        self.current().0.contains(id)
    }

    fn format(&self, id: &str, args: &Args) -> String {
        self.current().0.format(id, args)
    }

    fn try_format(&self, id: &str, args: &Args) -> Result<String, TranslateError> {
        self.current().0.try_format(id, args)
    }

    fn format_to_parts(&self, id: &str, args: &Args) -> Option<Formatted<Vec<Part>>> {
        self.current().0.format_to_parts(id, args)
    }

    fn format_number(
        &self,
        number: Number,
        options: NumberOptions,
    ) -> Result<String, I18nServiceError> {
        NumberFormatter::new(&self.locale(), options)
            .and_then(|formatter| formatter.format(number))
            .map_err(|error| I18nServiceError::Format(error.to_string()))
    }

    fn format_date_time(
        &self,
        value: &DateTime,
        options: DateTimeOptions,
    ) -> Result<String, I18nServiceError> {
        DateTimeFormatter::new(&self.locale(), options)
            .and_then(|formatter| formatter.format(value))
            .map_err(|error| I18nServiceError::Format(error.to_string()))
    }

    fn plural_category(&self, number: Number) -> PluralCategory {
        PluralRules::new(&self.locale(), PluralKind::Cardinal)
            .map_or(PluralCategory::Other, |rules| rules.category(number))
    }

    fn local_time(&self, instant: &str) -> Result<DateTime, I18nServiceError> {
        DateTime::from_instant(instant, &self.current().1)
            .map_err(|error| I18nServiceError::Format(error.to_string()))
    }

    fn local_time_from_millis(&self, millis: i64) -> Result<DateTime, I18nServiceError> {
        DateTime::from_unix_millis_in(millis, &self.current().1)
            .map_err(|error| I18nServiceError::Format(error.to_string()))
    }
}

/// RayX's formatter as a startup service under `dyn MessageFormatter`'s key, after the app's
/// `dyn I18nService`: on NSH, from then on, `tr!` follows the service's changes and NSH's locale
/// follows the language the app picked.
pub(crate) struct Following(pub(crate) Arc<RayxFormatter>);

impl AsyncInitializable for Following {
    fn is_initialized(&self) -> bool {
        self.0.following.load(Ordering::SeqCst)
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        if !self.0.following.swap(true, Ordering::SeqCst) {
            crate::translate::follow_i18n_changes(self.0.i18n());
            crate::locale::keep_nsh_locale(self.0.i18n());
        }
        futures::future::ready(Ok(())).boxed_local()
    }
}

impl StartupInitializable for Following {
    fn startup_service_type(&self) -> Option<StartupServiceType> {
        Some(StartupServiceType::of::<dyn MessageFormatter>())
    }

    fn initialization_depends_on(&self) -> Vec<StartupServiceType> {
        vec![StartupServiceType::of::<dyn I18nService>()]
    }
}
