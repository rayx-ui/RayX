//! RayX's i18n service: the app's languages and messages (the asset catalog's `locales/` and
//! messages added while the app runs), the user's locale (the platform's languages and clock,
//! or the app's choice) and time zone, published as [`I18nState`].

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, TryLockError};

use dxn_i18n::Bundle;
use dxn_intl::{DateTime, Direction, Locale, negotiate, parse_locale};
use futures::FutureExt as _;
use futures::future::{BoxFuture, LocalBoxFuture};
use rayx_app::di::{
    AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupInitializationState, StartupServiceError,
    StartupServiceType,
};
use rayx_app::services::assets::{AssetKind, AssetQuery, AssetService};
use rayx_app::services::i18n::{I18nService, I18nServiceError, I18nState, MessageBundle};
use rayx_app::services::platform::{LocaleSnapshot, PlatformInfoService, TextDirection};
use rayx_app::{ChangeSubscription, StateCell};

use crate::options::I18nOptions;

/// What the state is made from.
struct Inner {
    /// The user's locale.
    locale: Locale,
    /// The app picked the language itself: the platform's languages no longer apply.
    chosen: bool,
    /// The user's time zone (an IANA name), which instants are shown in.
    time_zone: String,
    /// The messages, a bundle per language, in the order the languages came.
    bundles: Vec<MessageBundle>,
    version: u64,
}

/// RayX's i18n service: the asset catalog's translations (and messages added while the app
/// runs), the user's locale and time zone, following the platform's until the app picks a
/// language. Messages are MessageFormat 2; a namespace other than the default one prefixes its
/// ids (`checkout.title`).
pub(crate) struct RayxI18n {
    options: I18nOptions,
    default: Locale,
    fallbacks: Vec<Locale>,
    inner: Mutex<Inner>,
    i18n: StateCell<I18nState>,
    /// Held by the thread publishing the state ([`RayxI18n::publish_latest`]).
    publishing: Mutex<()>,
    /// A change was made since the publishing thread read the state.
    stale: AtomicBool,
    assets: Option<Arc<dyn AssetService>>,
    platform: Option<Arc<dyn PlatformInfoService>>,
    following: Mutex<Option<ChangeSubscription>>,
    initialization: StartupInitializationState,
}

impl RayxI18n {
    /// A service with `options` and no messages yet, showing the default language, loading
    /// messages from `assets` and following `platform`.
    pub(crate) fn new(
        options: I18nOptions,
        assets: Option<Arc<dyn AssetService>>,
        platform: Option<Arc<dyn PlatformInfoService>>,
    ) -> Result<Arc<Self>, I18nServiceError> {
        let default = language(&options.default_language)?;
        let fallbacks = options
            .fallback_languages
            .iter()
            .map(|tag| language(tag))
            .collect::<Result<Vec<_>, _>>()?;
        let inner = Inner {
            locale: default.clone(),
            chosen: false,
            time_zone: "UTC".to_owned(),
            bundles: Vec::new(),
            version: 0,
        };
        let service = Self {
            i18n: StateCell::new(I18nState::default()),
            options,
            default,
            fallbacks,
            inner: Mutex::new(inner),
            publishing: Mutex::new(()),
            stale: AtomicBool::new(false),
            assets,
            platform,
            following: Mutex::default(),
            initialization: StartupInitializationState::default(),
        };
        let state = service.state(&service.lock());
        service.i18n.publish(state);
        Ok(Arc::new(service))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The published state of `inner`.
    fn state(&self, inner: &Inner) -> I18nState {
        let tags = |locales: &[Locale]| locales.iter().map(ToString::to_string).collect();
        I18nState {
            language: inner.locale.id.to_string(),
            chosen_language: inner.chosen.then(|| inner.locale.id.to_string()),
            locale: inner.locale.to_string(),
            direction: match dxn_intl::direction(&inner.locale) {
                Direction::Rtl => TextDirection::Rtl,
                _ => TextDirection::Ltr,
            },
            default_language: self.default.to_string(),
            fallback_languages: tags(&self.fallbacks),
            available_languages: inner
                .bundles
                .iter()
                .map(|bundle| bundle.language.clone())
                .collect(),
            time_zone: inner.time_zone.clone(),
            messages: inner.bundles.clone(),
            version: inner.version,
        }
    }

    /// Changes the state with `change`, which says whether it changed anything, and publishes
    /// it under the next version if it did.
    fn change(&self, change: impl FnOnce(&mut Inner) -> bool) {
        {
            let mut inner = self.lock();
            if !change(&mut inner) {
                return;
            }
            inner.version += 1;
        }
        self.publish_latest();
    }

    /// Publishes the latest state, outside the state's lock (a subscriber may call the
    /// service). One thread publishes at a time and reads the state when it does, so the
    /// published state is never older than one published before it: a change made while
    /// another thread (or a subscriber of this one) publishes is published right after, by the
    /// publishing thread, once it finds the state stale.
    fn publish_latest(&self) {
        self.stale.store(true, Ordering::SeqCst);
        loop {
            let publishing = match self.publishing.try_lock() {
                Ok(guard) => guard,
                Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
                Err(TryLockError::WouldBlock) => return,
            };
            self.stale.store(false, Ordering::SeqCst);
            let state = self.state(&self.lock());
            if self.i18n.get().version != state.version {
                self.i18n.publish(state);
            }
            drop(publishing);
            if !self.stale.load(Ordering::SeqCst) {
                return;
            }
        }
    }

    /// Takes the user's time zone from the platform, and their preferred languages and clock
    /// unless the app picked a language: the best of the app's languages for them, with the
    /// user's region (for separators and date order) and hour cycle. One change, decided
    /// under the state's lock, so a language the app picks meanwhile is kept.
    fn apply_platform(&self, snapshot: &LocaleSnapshot) {
        self.change(|inner| self.take_platform(inner, snapshot));
    }

    /// [`apply_platform`](Self::apply_platform)'s change of `inner`; whether it changed it.
    fn take_platform(&self, inner: &mut Inner, snapshot: &LocaleSnapshot) -> bool {
        let mut changed = false;
        if !snapshot.time_zone.is_empty() && inner.time_zone != snapshot.time_zone {
            inner.time_zone.clone_from(&snapshot.time_zone);
            changed = true;
        }
        if !inner.chosen {
            let locale = self.platform_locale(snapshot, &inner.bundles);
            if inner.locale != locale {
                inner.locale = locale;
                changed = true;
            }
        }
        changed
    }

    /// The best of the languages of `bundles` for the user's languages in `snapshot`, with the
    /// user's region and hour cycle.
    fn platform_locale(&self, snapshot: &LocaleSnapshot, bundles: &[MessageBundle]) -> Locale {
        let available: Vec<Locale> = bundles
            .iter()
            .filter_map(|bundle| parse_locale(&bundle.language).ok())
            .collect();
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
        let chosen = negotiate(&requested, &available, &self.default);
        // The user's own tag of that language, so its region formats the values.
        requested
            .iter()
            .find(|wanted| wanted.id.language == chosen.id.language)
            .cloned()
            .unwrap_or(chosen)
    }

    /// Follows the platform's locale changes (with [`I18nOptions::follow_platform`]), starting
    /// from its current snapshot.
    fn follow(self: &Arc<Self>) {
        let Some(platform) = self
            .platform
            .clone()
            .filter(|_| self.options.follow_platform)
        else {
            return;
        };
        let mut following = self
            .following
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if following.is_some() {
            return;
        }
        let state = platform.platform();
        let current = state.get().locale.clone();
        self.apply_platform(&current);
        let service = Arc::downgrade(self);
        let last = Mutex::new(current);
        *following = Some(state.subscribe(move |snapshot| {
            let mut seen = last.lock().unwrap_or_else(PoisonError::into_inner);
            if *seen != snapshot.locale {
                *seen = snapshot.locale.clone();
                drop(seen);
                if let Some(service) = service.upgrade() {
                    service.apply_platform(&snapshot.locale);
                }
            }
        }));
    }

    fn add(
        &self,
        language_tag: &str,
        namespace: Option<&str>,
        json: &str,
    ) -> Result<(), I18nServiceError> {
        let locale = language(language_tag)?;
        let mut bundle = Bundle::new(locale.clone());
        let prefix = match namespace {
            Some(namespace) if namespace != self.options.default_namespace => namespace,
            _ => "",
        };
        let result = bundle.add_json_with_prefix(prefix, json);
        // The messages that parse are added even when others do not.
        let messages: Vec<(String, String)> = bundle
            .ids()
            .filter_map(|id| {
                bundle
                    .get(id)
                    .map(|message| (id.to_owned(), message.source().to_owned()))
            })
            .collect();
        let tag = locale.to_string();
        self.change(|inner| {
            let index = match inner.bundles.iter().position(|b| b.language == tag) {
                Some(index) => index,
                None => {
                    inner.bundles.push(MessageBundle {
                        language: tag,
                        messages: Default::default(),
                    });
                    inner.bundles.len() - 1
                }
            };
            inner.bundles[index].messages.extend(messages);
            true
        });
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
    ///
    /// A message several packages have comes from the package listed first in the asset
    /// options (the app's own), so an app overrides the translations of the libraries it uses.
    async fn load_assets(&self) -> Result<(), I18nServiceError> {
        let assets = self
            .assets
            .clone()
            .ok_or(I18nServiceError::MissingAssetService)?;
        let mut resources = assets
            .query(&AssetQuery::kind(AssetKind::Locale))
            .unwrap_or_default();
        let packages = assets
            .loaded_catalog()
            .ok()
            .and_then(|catalog| {
                catalog
                    .as_ref()
                    .as_ref()
                    .map(|catalog| catalog.packages().to_vec())
            })
            .unwrap_or_default();
        let rank = |package: &str| {
            packages
                .iter()
                .position(|listed| listed == package)
                .unwrap_or(packages.len())
        };
        // A bundle added later replaces the messages it shares with earlier ones: the package
        // listed last goes first and the app's own, listed first, last.
        resources.sort_by_key(|resource| std::cmp::Reverse(rank(&resource.package)));
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
            let loaded = match assets.load_text(logical_path.clone()).await {
                Ok(json) => self.add(language_tag, namespace, &json),
                Err(error) => Err(I18nServiceError::Resource {
                    path: logical_path,
                    reason: error.to_string(),
                }),
            };
            if let Err(error) = loaded {
                first_error.get_or_insert(error);
            }
        }
        // The platform's languages may fit a language that just came.
        let following = self
            .following
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some();
        if let Some(platform) = self.platform.as_ref().filter(|_| following) {
            self.apply_platform(&platform.platform().get().locale);
        }
        first_error.map_or(Ok(()), Err)
    }
}

impl I18nService for RayxI18n {
    fn i18n(&self) -> &StateCell<I18nState> {
        &self.i18n
    }

    fn set_language(&self, language_tag: String) -> BoxFuture<'_, Result<(), I18nServiceError>> {
        let result = language(&language_tag).map(|locale| {
            self.change(|inner| {
                let changed = !inner.chosen || inner.locale != locale;
                inner.chosen = true;
                inner.locale = locale;
                changed
            });
        });
        futures::future::ready(result).boxed()
    }

    fn follow_platform(&self) -> BoxFuture<'_, Result<(), I18nServiceError>> {
        match &self.platform {
            Some(platform) => {
                let snapshot = platform.platform().get();
                self.change(|inner| {
                    let was_chosen = std::mem::replace(&mut inner.chosen, false);
                    self.take_platform(inner, &snapshot.locale) || was_chosen
                });
            }
            None => self.change(|inner| {
                let changed = inner.chosen || inner.locale != self.default;
                inner.chosen = false;
                inner.locale = self.default.clone();
                changed
            }),
        }
        futures::future::ready(Ok(())).boxed()
    }

    fn load_messages(&self) -> BoxFuture<'_, Result<(), I18nServiceError>> {
        self.load_assets().boxed()
    }

    fn add_resource(
        &self,
        language: String,
        namespace: Option<String>,
        json: String,
    ) -> BoxFuture<'_, Result<(), I18nServiceError>> {
        futures::future::ready(self.add(&language, namespace.as_deref(), &json)).boxed()
    }

    fn set_time_zone(&self, time_zone: String) -> BoxFuture<'_, Result<(), I18nServiceError>> {
        let result = DateTime::from_unix_millis_in(0, &time_zone)
            .map(|_| {
                self.change(|inner| {
                    let changed = inner.time_zone != time_zone;
                    inner.time_zone = time_zone;
                    changed
                });
            })
            .map_err(|error| I18nServiceError::Format(error.to_string()));
        futures::future::ready(result).boxed()
    }
}

fn language(tag: &str) -> Result<Locale, I18nServiceError> {
    parse_locale(tag).map_err(|_| I18nServiceError::InvalidLanguage(tag.to_owned()))
}

/// RayX's i18n service as a startup service under `dyn I18nService`'s key: it loads the asset
/// catalog's translations when the app starts (after the asset service) unless its options turn
/// that off, then follows the platform's languages and clock.
pub(crate) struct Loading(pub(crate) Arc<RayxI18n>);

impl AsyncInitializable for Loading {
    fn is_initialized(&self) -> bool {
        self.0.initialization.is_initialized()
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let service = Arc::clone(&self.0);
        async move {
            if service.options.auto_load_assets {
                service
                    .load_assets()
                    .await
                    .map_err(|error| StartupServiceError::new(error.to_string()))?;
            }
            service.follow();
            service.initialization.mark_initialized();
            Ok(())
        }
        .boxed_local()
    }
}

impl StartupInitializable for Loading {
    fn startup_service_type(&self) -> Option<StartupServiceType> {
        Some(StartupServiceType::of::<dyn I18nService>())
    }

    fn initialization_depends_on(&self) -> Vec<StartupServiceType> {
        if self.0.options.auto_load_assets {
            vec![StartupServiceType::of::<dyn AssetService>()]
        } else {
            Vec::new()
        }
    }
}
