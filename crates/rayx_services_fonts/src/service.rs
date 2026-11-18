//! RayX's font service: the fonts registered through the platform's [`FontRegistrar`], the
//! aliases resolved against them, published as [`FontState`].

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, PoisonError};

use futures::FutureExt as _;
use futures::future::{BoxFuture, LocalBoxFuture};
use rayx_app::StateCell;
use rayx_app::di::{
    AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupInitializationState, StartupServiceError,
    StartupServiceType,
};
use rayx_app::services::assets::{AssetKind, AssetQuery, AssetService};
use rayx_app::services::fonts::{
    FontDiagnostic, FontDiagnosticCode, FontService, FontServiceError, FontState,
    RegisteredFontFace, ResolvedFontAlias,
};

use crate::options::{FontAlias, FontLoadFallbackKind, FontOptions, FontOptionsError};
use crate::registrar::FontRegistrar;

/// RayX's font service: registers the asset catalog's fonts with the platform when the app
/// starts (and others while it runs), and resolves the aliases the app names fonts by into
/// family lists, a missing family's load fallback standing in.
pub(crate) struct RayxFonts {
    options: FontOptions,
    registrar: Option<Arc<dyn FontRegistrar>>,
    assets: Option<Arc<dyn AssetService>>,
    fonts: StateCell<FontState>,
    /// Serializes changes, so two registrations never publish over each other.
    changing: Mutex<()>,
    initialization: StartupInitializationState,
}

impl RayxFonts {
    /// A service with `options` (normalized and checked) registering through `registrar` and
    /// reading font files through `assets`.
    pub(crate) fn new(
        options: FontOptions,
        registrar: Option<Arc<dyn FontRegistrar>>,
        assets: Option<Arc<dyn AssetService>>,
    ) -> Result<Arc<Self>, FontOptionsError> {
        let options = options.normalized()?;
        let service = Arc::new(Self {
            fonts: StateCell::new(FontState {
                default_alias: options.default_alias.clone(),
                ..FontState::default()
            }),
            options,
            registrar,
            assets,
            changing: Mutex::new(()),
            initialization: StartupInitializationState::default(),
        });
        service.change(|_| {});
        Ok(service)
    }

    /// Changes the state with `change`, resolves the aliases again and publishes it.
    fn change(&self, change: impl FnOnce(&mut FontState)) {
        let _changing = self.changing.lock().unwrap_or_else(PoisonError::into_inner);
        let mut state = (*self.fonts.get()).clone();
        change(&mut state);
        let loaded: BTreeSet<String> = state.faces.iter().map(|face| face.family.clone()).collect();
        let (aliases, substitutions) = resolve_aliases(&self.options, &loaded);
        state.aliases = aliases;
        state
            .diagnostics
            .retain(|diagnostic| diagnostic.code != FontDiagnosticCode::LoadFallbackSubstitution);
        state.diagnostics.extend(substitutions);
        self.fonts.publish(state);
    }

    async fn register_assets(&self) -> Result<(), FontServiceError> {
        let assets = self
            .assets
            .clone()
            .ok_or(FontServiceError::MissingAssetService)?;
        let fonts = assets
            .query(&AssetQuery::kind(AssetKind::Font))
            .unwrap_or_default();
        if fonts.is_empty() {
            self.change(|_| {});
            return Ok(());
        }
        let registrar = self
            .registrar
            .clone()
            .ok_or(FontServiceError::NoRegistrar)?;
        let mut faces = Vec::new();
        let mut diagnostics = Vec::new();
        for font in fonts {
            let logical_path = font.logical_path();
            let registered = match assets.load_bytes(logical_path.clone()).await {
                Ok(bytes) => registrar
                    .register_asset(&font.uri(), bytes)
                    .await
                    .map_err(|reason| {
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
            match registered {
                Ok(family) => faces.push(RegisteredFontFace {
                    logical_path: Some(logical_path),
                    family,
                }),
                Err(diagnostic) => diagnostics.push(diagnostic.with_logical_path(logical_path)),
            }
        }
        self.change(|state| {
            state.faces.extend(faces);
            state.diagnostics.extend(diagnostics);
        });
        Ok(())
    }
}

impl FontService for RayxFonts {
    fn fonts(&self) -> &StateCell<FontState> {
        &self.fonts
    }

    fn register_font(&self, data: Vec<u8>) -> BoxFuture<'_, Result<String, FontServiceError>> {
        async move {
            let registrar = self
                .registrar
                .clone()
                .ok_or(FontServiceError::NoRegistrar)?;
            let family = registrar
                .register(data)
                .await
                .map_err(FontServiceError::Registration)?;
            self.change(|state| {
                state.faces.push(RegisteredFontFace {
                    logical_path: None,
                    family: family.clone(),
                });
            });
            Ok(family)
        }
        .boxed()
    }

    fn register_font_assets(&self) -> BoxFuture<'_, Result<(), FontServiceError>> {
        self.register_assets().boxed()
    }
}

/// RayX's font service as a startup service under `dyn FontService`'s key: it registers the
/// catalog's fonts when the app starts (after the asset service) unless its options turn that
/// off.
pub(crate) struct Registering(pub(crate) Arc<RayxFonts>);

impl AsyncInitializable for Registering {
    fn is_initialized(&self) -> bool {
        self.0.initialization.is_initialized()
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let service = Arc::clone(&self.0);
        async move {
            if service.options.auto_register_assets {
                service
                    .register_assets()
                    .await
                    .map_err(|error| StartupServiceError::new(error.to_string()))?;
            }
            service.initialization.mark_initialized();
            Ok(())
        }
        .boxed_local()
    }
}

impl StartupInitializable for Registering {
    fn startup_service_type(&self) -> Option<StartupServiceType> {
        Some(StartupServiceType::of::<dyn FontService>())
    }

    fn initialization_depends_on(&self) -> Vec<StartupServiceType> {
        if self.0.options.auto_register_assets {
            vec![StartupServiceType::of::<dyn AssetService>()]
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
