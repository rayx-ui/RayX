use std::sync::{Arc, Mutex, PoisonError, RwLock};

use dxn_i18n::{
    Args, BidiIsolation, Bundle, Formatted, MissingMessage, Part, TranslateError, Translator,
};
use dxn_intl::{
    DateTime, DateTimeFormatter, DateTimeOptions, Direction, Locale, Number, NumberFormatter,
    NumberOptions, PluralCategory, PluralKind, PluralRules, negotiate, parse_locale,
};
use futures::FutureExt as _;
use futures::future::LocalBoxFuture;
use rayx_core::ChangeSubscription;
use rayx_core::change::Listeners;
use rayx_core::di::{
    AppService, AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupInitializationState, StartupServiceError,
    StartupServiceType,
};

use super::{I18nOptions, MissingMessagePolicy};
use crate::services::assets::{AssetKind, AssetQuery, AssetService};
use crate::services::platform::{LocaleSnapshot, PlatformInfoService};

/// The user's locale changed, or messages were added: what the app shows needs formatting
/// again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct I18nChangedEvent {
    /// The locale messages and values are formatted for now.
    pub locale: Locale,
    /// Counts the changes, from 0.
    pub version: u64,
}

/// Why an i18n call failed.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum I18nServiceError {
    #[error("`{0}` is not a valid language tag")]
    InvalidLanguage(String),
    #[error("the translations of `{path}` did not load: {reason}")]
    Resource { path: String, reason: String },
    #[error("the i18n service needs the asset service to load translations")]
    MissingAssetService,
    #[error("the value could not be formatted: {0}")]
    Format(String),
}

type I18nListener = dyn Fn(&I18nChangedEvent) + Send + Sync;

struct State {
    translator: Translator,
    /// The app picked the language itself: the platform's languages no longer apply.
    chosen: bool,
    version: u64,
}

/// The app's translations: a [`Translator`] over the asset catalog's `locales/` (and messages
/// added while the app runs), the user's locale (the platform's languages and clock, or the
/// app's choice), and the locale's number, date and plural formatting.
///
/// Messages are [MessageFormat 2](https://www.unicode.org/reports/tr35/tr35-messageFormat.html);
/// a namespace other than the default one prefixes its ids (`checkout.title`).
#[derive(Clone, AppService)]
#[app_service(startup)]
pub struct I18nService {
    options: I18nOptions,
    state: Arc<RwLock<State>>,
    listeners: Listeners<I18nListener>,
    platform: Arc<Mutex<Option<ChangeSubscription>>>,
    initialization: StartupInitializationState,
}

impl I18nService {
    /// A service with `options` and no messages yet, showing the default language.
    pub fn new(options: I18nOptions) -> Result<Self, I18nServiceError> {
        let default = language(&options.default_language)?;
        let mut translator = Translator::new(default);
        translator.set_fallbacks(
            options
                .fallback_languages
                .iter()
                .map(|tag| language(tag))
                .collect::<Result<_, _>>()?,
        );
        translator.set_missing_message(match options.missing_message {
            MissingMessagePolicy::Id => MissingMessage::Id,
            MissingMessagePolicy::Empty => MissingMessage::Empty,
        });
        if options.bidi_isolation {
            translator.set_bidi_isolation(BidiIsolation::Default);
        }
        Ok(Self {
            options,
            state: Arc::new(RwLock::new(State {
                translator,
                chosen: false,
                version: 0,
            })),
            listeners: Listeners::default(),
            platform: Arc::default(),
            initialization: StartupInitializationState::default(),
        })
    }

    pub fn options(&self) -> &I18nOptions {
        &self.options
    }

    /// The locale messages and values are formatted for.
    pub fn locale(&self) -> Locale {
        self.read().translator.locale().clone()
    }

    /// The locale's language tag (`de-AT`), without its Unicode extensions.
    pub fn language(&self) -> String {
        self.locale().id.to_string()
    }

    /// Counts the changes listeners heard.
    pub fn version(&self) -> u64 {
        self.read().version
    }

    /// The languages the app has messages for.
    pub fn available_languages(&self) -> Vec<String> {
        self.read()
            .translator
            .available_locales()
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    /// Shows the app in `language` from now on (`de`, `pt-BR`, `en-u-hc-h23`), whatever the
    /// platform reports later.
    pub fn set_language(&self, language_tag: &str) -> Result<(), I18nServiceError> {
        let locale = language(language_tag)?;
        self.change(|state| {
            state.chosen = true;
            state.translator.set_locale(locale);
        });
        Ok(())
    }

    /// Follows the platform's languages again after [`I18nService::set_language`].
    pub fn follow_platform_again(&self, snapshot: &LocaleSnapshot) {
        self.write().chosen = false;
        self.apply_platform(snapshot);
    }

    /// Takes the user's preferred languages and clock from the platform, unless the app picked
    /// a language: the best of the app's languages for them, with the user's region (for
    /// separators and date order) and hour cycle.
    pub fn apply_platform(&self, snapshot: &LocaleSnapshot) {
        if self.read().chosen {
            return;
        }
        let hour_cycle = match snapshot.hour_cycle.as_str() {
            "h11" | "h12" | "h23" => Some(snapshot.hour_cycle.as_str()),
            "h24" => Some("h23"),
            _ => None,
        };
        let mut requested: Vec<Locale> = snapshot
            .languages
            .iter()
            .chain(std::iter::once(&snapshot.locale))
            .filter_map(|tag| parse_locale(tag).ok())
            .collect();
        if let Some(cycle) = hour_cycle {
            for locale in &mut requested {
                if let Ok(with_cycle) = format!("{}-u-hc-{cycle}", locale.id).parse::<Locale>() {
                    *locale = with_cycle;
                }
            }
        }
        let locale = {
            let state = self.read();
            let available = state.translator.available_locales();
            let default = state.translator.default_locale().clone();
            let chosen = negotiate(&requested, &available, &default);
            // The user's own tag of that language, so its region formats the values.
            requested
                .iter()
                .find(|wanted| wanted.id.language == chosen.id.language)
                .cloned()
                .unwrap_or(chosen)
        };
        if *self.read().translator.locale() != locale {
            self.change(|state| state.translator.set_locale(locale));
        }
    }

    /// Adds the messages of a JSON resource for `language` (in `namespace`, or the default one
    /// when `None`), replacing messages with the same ids.
    pub fn add_resource(
        &self,
        language_tag: &str,
        namespace: Option<&str>,
        json: &str,
    ) -> Result<(), I18nServiceError> {
        let mut bundle = Bundle::new(language(language_tag)?);
        let prefix = match namespace {
            Some(namespace) if namespace != self.options.default_namespace => namespace,
            _ => "",
        };
        let result = bundle.add_json_with_prefix(prefix, json);
        // The messages that parse are added even when others do not.
        self.change(|state| state.translator.add_bundle(bundle));
        result.map_err(|error| I18nServiceError::Resource {
            path: format!(
                "{language_tag}/{}",
                namespace.unwrap_or(&self.options.default_namespace)
            ),
            reason: error.to_string(),
        })
    }

    /// Loads every translation under [`I18nOptions::resources_path`] of the asset catalog:
    /// `<path>/<language>.json` and `<path>/<language>/<namespace>.json`. The ones that load
    /// are added; the error names the first that did not.
    pub async fn load_assets(&self, assets: &AssetService) -> Result<(), I18nServiceError> {
        let resources = assets
            .query(&AssetQuery::kind(AssetKind::Locale))
            .unwrap_or_default();
        let root = format!("{}/", self.options.resources_path.trim_matches('/'));
        let mut first_error = None;
        for resource in resources {
            let Some(relative) = resource.path.strip_prefix(&root) else {
                continue;
            };
            let Some(relative) = relative.strip_suffix(".json") else {
                continue;
            };
            let (language_tag, namespace) = match relative.split_once('/') {
                Some((language, namespace)) => (language, Some(namespace)),
                None => (relative, None),
            };
            let language_tag = resource.locale.as_deref().unwrap_or(language_tag);
            let logical_path = resource.logical_path();
            let loaded = match assets.load_text(&logical_path).await {
                Ok(json) => self.add_resource(language_tag, namespace, &json),
                Err(error) => Err(I18nServiceError::Resource {
                    path: logical_path,
                    reason: error.to_string(),
                }),
            };
            if let Err(error) = loaded {
                first_error.get_or_insert(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    /// Whether a language in the lookup chain has the message `id`.
    pub fn contains(&self, id: &str) -> bool {
        self.read().translator.contains(id)
    }

    /// The message `id` without arguments.
    pub fn t(&self, id: &str) -> String {
        self.format(id, &Args::new())
    }

    /// The message `id` formatted with `args`; what fails shows as its fallback (`{$name}`), a
    /// missing message as [`I18nOptions::missing_message`] says.
    pub fn format(&self, id: &str, args: &Args) -> String {
        self.read().translator.format(id, args)
    }

    /// The message `id` formatted with `args`, or why it could not be formatted whole.
    pub fn try_format(&self, id: &str, args: &Args) -> Result<String, TranslateError> {
        self.read().translator.try_format(id, args)
    }

    /// The message `id` as text, values and markup, for rich text; `None` when it is missing.
    pub fn format_to_parts(&self, id: &str, args: &Args) -> Option<Formatted<Vec<Part>>> {
        self.read().translator.format_to_parts(id, args)
    }

    /// `number` formatted for the locale.
    pub fn format_number(
        &self,
        number: impl Into<Number>,
        options: NumberOptions,
    ) -> Result<String, I18nServiceError> {
        NumberFormatter::new(&self.locale(), options)
            .and_then(|formatter| formatter.format(number.into()))
            .map_err(|error| I18nServiceError::Format(error.to_string()))
    }

    /// `value` formatted for the locale, in its calendar and clock.
    pub fn format_date_time(
        &self,
        value: &DateTime,
        options: DateTimeOptions,
    ) -> Result<String, I18nServiceError> {
        DateTimeFormatter::new(&self.locale(), options)
            .and_then(|formatter| formatter.format(value))
            .map_err(|error| I18nServiceError::Format(error.to_string()))
    }

    /// `number`'s cardinal plural category in the locale.
    pub fn plural_category(&self, number: impl Into<Number>) -> PluralCategory {
        PluralRules::new(&self.locale(), PluralKind::Cardinal)
            .map_or(PluralCategory::Other, |rules| rules.category(number))
    }

    /// The direction the locale's script is written in.
    pub fn direction(&self) -> Direction {
        dxn_intl::direction(&self.locale())
    }

    /// Calls `listener` with every change of the locale or the messages until the subscription
    /// is dropped.
    pub fn subscribe(
        &self,
        listener: impl Fn(&I18nChangedEvent) + Send + Sync + 'static,
    ) -> ChangeSubscription {
        self.listeners.add(Arc::new(listener))
    }

    /// Follows `platform`'s locale changes (with [`I18nOptions::follow_platform`]), starting
    /// from its current snapshot.
    pub fn follow(&self, platform: &PlatformInfoService) {
        if !self.options.follow_platform {
            return;
        }
        self.apply_platform(&platform.current().locale);
        let service = self.clone();
        let subscription = platform.subscribe(move |event| {
            if event.previous.locale != event.current.locale {
                service.apply_platform(&event.current.locale);
            }
        });
        *self.platform.lock().unwrap_or_else(PoisonError::into_inner) = Some(subscription);
    }

    fn change(&self, change: impl FnOnce(&mut State)) {
        let event = {
            let mut state = self.write();
            change(&mut state);
            state.version += 1;
            I18nChangedEvent {
                locale: state.translator.locale().clone(),
                version: state.version,
            }
        };
        self.listeners.notify(|listener| listener(&event));
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, State> {
        self.state.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, State> {
        self.state.write().unwrap_or_else(PoisonError::into_inner)
    }
}

fn language(tag: &str) -> Result<Locale, I18nServiceError> {
    parse_locale(tag).map_err(|_| I18nServiceError::InvalidLanguage(tag.to_owned()))
}

impl AsyncInitializable for I18nService {
    fn is_initialized(&self) -> bool {
        self.initialization.is_initialized()
    }

    fn initialize(
        &self,
        context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let service = self.clone();
        async move {
            if service.options.auto_load_assets {
                let assets = context
                    .services()
                    .service::<AssetService>()
                    .ok_or(I18nServiceError::MissingAssetService)
                    .map_err(|error| StartupServiceError::new(error.to_string()))?;
                service
                    .load_assets(&assets)
                    .await
                    .map_err(|error| StartupServiceError::new(error.to_string()))?;
            }
            if let Some(platform) = context.services().service::<PlatformInfoService>() {
                service.follow(&platform);
            }
            service.initialization.mark_initialized();
            Ok(())
        }
        .boxed_local()
    }
}

impl StartupInitializable for I18nService {
    fn startup_service_type(&self) -> StartupServiceType {
        StartupServiceType::of::<Self>()
    }

    fn initialization_depends_on(&self) -> Vec<StartupServiceType> {
        if self.options.auto_load_assets {
            vec![StartupServiceType::of::<AssetService>()]
        } else {
            Vec::new()
        }
    }
}
