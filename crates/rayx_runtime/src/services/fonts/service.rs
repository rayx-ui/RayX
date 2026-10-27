use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, PoisonError};

use futures::FutureExt as _;
use futures::future::LocalBoxFuture;
use rayx_core::di::{
    AppService, AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupInitializationState, StartupServiceError,
    StartupServiceType,
};
use serde::{Deserialize, Serialize};

use super::{
    FontAlias, FontDiagnostic, FontDiagnosticCode, FontLoadFallbackKind, FontOptions,
    FontOptionsError, FontRegistrar,
};
use crate::services::assets::{AssetKind, AssetQuery, AssetService};

/// A font the service registered.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RegisteredFontFace {
    /// The asset it came from (`@<package>/<path>`), or `None` for one registered from bytes.
    pub logical_path: Option<String>,
    /// Its family name, as the platform read it.
    pub family: String,
}

/// An alias resolved against the fonts that loaded.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolvedFontAlias {
    pub alias: String,
    pub requested_family: String,
    /// The family, or its load fallback's when it did not load.
    pub resolved_family: String,
    /// The glyph fallbacks' families, in order.
    pub glyph_fallback_families: Vec<String>,
}

impl ResolvedFontAlias {
    /// The family and its glyph fallbacks as one font family list (`Inter, Noto Color Emoji`),
    /// for NSH's `font_family` / `fontFamily`, which take fallback lists.
    pub fn family_list(&self) -> String {
        std::iter::once(&self.resolved_family)
            .chain(&self.glyph_fallback_families)
            .map(|family| {
                if family.contains([',', '"']) || family.trim() != family {
                    format!("\"{}\"", family.replace('"', "\\\""))
                } else {
                    family.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Why a font call failed.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum FontServiceError {
    #[error(transparent)]
    InvalidOptions(#[from] FontOptionsError),
    #[error("no FontRegistrar is registered (rayx_app registers NSH's)")]
    NoRegistrar,
    #[error("the font service needs the asset service to register font assets")]
    MissingAssetService,
    #[error("the font could not be registered: {0}")]
    Registration(String),
}

#[derive(Default)]
struct FontState {
    faces: Vec<RegisteredFontFace>,
    aliases: Vec<ResolvedFontAlias>,
    diagnostics: Vec<FontDiagnostic>,
}

/// The app's fonts: registers the asset catalog's fonts with the platform when the app starts
/// (and others while it runs), and resolves the aliases the app names fonts by into family
/// lists, a missing family's load fallback standing in.
#[derive(Clone, AppService)]
#[app_service(startup)]
pub struct FontService {
    options: FontOptions,
    registrar: Option<Arc<dyn FontRegistrar>>,
    state: Arc<Mutex<FontState>>,
    initialization: StartupInitializationState,
}

impl FontService {
    /// A service with `options` (normalized and checked) registering through `registrar`.
    pub fn new(
        options: FontOptions,
        registrar: Option<Arc<dyn FontRegistrar>>,
    ) -> Result<Self, FontServiceError> {
        let options = options.normalized()?;
        let service = Self {
            options,
            registrar,
            state: Arc::default(),
            initialization: StartupInitializationState::default(),
        };
        service.resolve();
        Ok(service)
    }

    pub fn options(&self) -> &FontOptions {
        &self.options
    }

    /// The fonts registered so far.
    pub fn registered_fonts(&self) -> Vec<RegisteredFontFace> {
        self.lock().faces.clone()
    }

    /// Every alias, resolved against the fonts registered so far.
    pub fn resolved_aliases(&self) -> Vec<ResolvedFontAlias> {
        self.lock().aliases.clone()
    }

    /// The alias `alias`, resolved.
    pub fn resolve_alias(&self, alias: &str) -> Option<ResolvedFontAlias> {
        self.lock()
            .aliases
            .iter()
            .find(|resolved| resolved.alias == alias)
            .cloned()
    }

    /// The default alias, resolved.
    pub fn default_font(&self) -> Option<ResolvedFontAlias> {
        self.options
            .default_alias
            .as_deref()
            .and_then(|alias| self.resolve_alias(alias))
    }

    /// The font family list for `alias_or_family`: an alias's resolved list, or the name itself.
    pub fn font_family(&self, alias_or_family: &str) -> String {
        self.resolve_alias(alias_or_family)
            .map_or_else(|| alias_or_family.to_owned(), |alias| alias.family_list())
    }

    /// What registering fonts and resolving aliases found wrong.
    pub fn diagnostics(&self) -> Vec<FontDiagnostic> {
        self.lock().diagnostics.clone()
    }

    /// Registers a font from its data while the app runs (one downloaded, picked or made),
    /// and resolves the aliases again; resolves to its family name.
    pub async fn register_font(&self, data: Vec<u8>) -> Result<String, FontServiceError> {
        let registrar = self
            .registrar
            .clone()
            .ok_or(FontServiceError::NoRegistrar)?;
        let family = registrar
            .register(data)
            .await
            .map_err(FontServiceError::Registration)?;
        self.lock().faces.push(RegisteredFontFace {
            logical_path: None,
            family: family.clone(),
        });
        self.resolve();
        Ok(family)
    }

    /// Registers the font assets of `assets`' catalog, then resolves the aliases (the startup
    /// does when `auto_register_assets` is on). A font that does not load or register is a
    /// diagnostic; a missing registrar, with fonts to register, is an error.
    pub async fn register_font_assets(
        &self,
        assets: &AssetService,
    ) -> Result<(), FontServiceError> {
        let fonts = assets
            .query(&AssetQuery::kind(AssetKind::Font))
            .unwrap_or_default();
        if fonts.is_empty() {
            self.resolve();
            return Ok(());
        }
        let registrar = self
            .registrar
            .clone()
            .ok_or(FontServiceError::NoRegistrar)?;
        for font in fonts {
            let logical_path = font.logical_path();
            let registered = match assets.load_bytes(&logical_path).await {
                Ok(bytes) => registrar.register(bytes.to_vec()).await.map_err(|reason| {
                    FontDiagnostic::error(
                        FontDiagnosticCode::RegistrationFailed,
                        format!("font asset `{logical_path}` did not register: {reason}"),
                    )
                }),
                Err(error) => Err(FontDiagnostic::error(
                    FontDiagnosticCode::MissingAsset,
                    format!("font asset `{logical_path}` did not load: {error}"),
                )),
            };
            let mut state = self.lock();
            match registered {
                Ok(family) => state.faces.push(RegisteredFontFace {
                    logical_path: Some(logical_path),
                    family,
                }),
                Err(diagnostic) => state
                    .diagnostics
                    .push(diagnostic.with_logical_path(logical_path)),
            }
        }
        self.resolve();
        Ok(())
    }

    /// Resolves the aliases against the registered families.
    fn resolve(&self) {
        let mut state = self.lock();
        let loaded: BTreeSet<String> = state.faces.iter().map(|face| face.family.clone()).collect();
        let (aliases, substitutions) = resolve_aliases(&self.options, &loaded);
        state.aliases = aliases;
        state
            .diagnostics
            .retain(|diagnostic| diagnostic.code != FontDiagnosticCode::LoadFallbackSubstitution);
        state.diagnostics.extend(substitutions);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, FontState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl AsyncInitializable for FontService {
    fn is_initialized(&self) -> bool {
        self.initialization.is_initialized()
    }

    fn initialize(
        &self,
        context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let service = self.clone();
        async move {
            if service.options.auto_register_assets {
                let assets = context
                    .services()
                    .service::<AssetService>()
                    .ok_or(FontServiceError::MissingAssetService)
                    .map_err(|error| StartupServiceError::new(error.to_string()))?;
                service
                    .register_font_assets(&assets)
                    .await
                    .map_err(|error| StartupServiceError::new(error.to_string()))?;
            }
            service.initialization.mark_initialized();
            Ok(())
        }
        .boxed_local()
    }
}

impl StartupInitializable for FontService {
    fn startup_service_type(&self) -> StartupServiceType {
        StartupServiceType::of::<Self>()
    }

    fn initialization_depends_on(&self) -> Vec<StartupServiceType> {
        if self.options.auto_register_assets {
            vec![StartupServiceType::of::<AssetService>()]
        } else {
            Vec::new()
        }
    }
}

/// Every alias resolved against `loaded`, and the substitutions load fallbacks made.
fn resolve_aliases(
    options: &FontOptions,
    loaded: &BTreeSet<String>,
) -> (Vec<ResolvedFontAlias>, Vec<FontDiagnostic>) {
    let aliases: BTreeMap<&str, &FontAlias> = options
        .aliases
        .iter()
        .map(|alias| (alias.alias.as_str(), alias))
        .collect();
    let mut cache = BTreeMap::new();
    let mut diagnostics = Vec::new();
    let resolved = options
        .aliases
        .iter()
        .map(|alias| {
            let resolved_family =
                resolve_family(&alias.alias, &aliases, loaded, &mut cache, &mut Vec::new());
            if resolved_family != alias.family {
                diagnostics.push(
                    FontDiagnostic::warning(
                        FontDiagnosticCode::LoadFallbackSubstitution,
                        format!(
                            "font alias `{}` stands in `{}` for `{}`, which did not load",
                            alias.alias, resolved_family, alias.family
                        ),
                    )
                    .with_alias(alias.alias.clone())
                    .with_fallback(resolved_family.clone()),
                );
            }
            ResolvedFontAlias {
                alias: alias.alias.clone(),
                requested_family: alias.family.clone(),
                glyph_fallback_families: alias
                    .glyph_fallbacks
                    .iter()
                    .map(|fallback| {
                        resolve_family(fallback, &aliases, loaded, &mut cache, &mut Vec::new())
                    })
                    .collect(),
                resolved_family,
            }
        })
        .collect();
    (resolved, diagnostics)
}

/// An alias's family: its own when it loaded or it has no load fallback, else its fallback's
/// (another alias's, resolved the same way, or a family).
fn resolve_family(
    name: &str,
    aliases: &BTreeMap<&str, &FontAlias>,
    loaded: &BTreeSet<String>,
    cache: &mut BTreeMap<String, String>,
    stack: &mut Vec<String>,
) -> String {
    if let Some(cached) = cache.get(name) {
        return cached.clone();
    }
    let Some(alias) = aliases.get(name) else {
        return name.to_owned();
    };
    let Some(fallback) = alias
        .load_fallback
        .as_ref()
        .filter(|_| !loaded.contains(&alias.family))
    else {
        cache.insert(name.to_owned(), alias.family.clone());
        return alias.family.clone();
    };
    if stack.iter().any(|entry| entry == name) {
        return alias.family.clone();
    }
    stack.push(name.to_owned());
    let resolved = match fallback.kind {
        FontLoadFallbackKind::Alias => {
            resolve_family(&fallback.target, aliases, loaded, cache, stack)
        }
        FontLoadFallbackKind::Family => fallback.target.clone(),
    };
    stack.pop();
    cache.insert(name.to_owned(), resolved.clone());
    resolved
}
