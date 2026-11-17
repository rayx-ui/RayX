//! The options objects TS builder methods hand their callbacks
//! (`builder.addI18n(options => { options.defaultLanguage("en"); })`), and the configuration
//! builder `configureAppConfiguration` hands its callback.
//!
//! Each records what its methods set, in order, as plain data. The builder method replays the
//! record in the Rust `configure` closure it passes to the Rust extension method of its name, so
//! what TS sets applies after the options bound from configuration, exactly as a Rust closure's
//! changes do. An options object is valid during its callback only: it is disposed after it, and
//! using it later throws.

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx_app::configuration::ConfigurationBuilder as Configuration;
use rayx_runtime::services::application_configuration::ApplicationConfiguration;
use rayx_runtime::services::assets::AssetOptions as Assets;
use rayx_runtime::services::fonts::{FontAlias, FontOptions as Fonts};
use rayx_runtime::services::i18n::{I18nOptions as I18n, MissingMessagePolicy};
use rayx_runtime::services::input_map::InputMapOptions as InputMap;
use rayx_runtime::services::navigation::{NavigationOptions as Navigation, RouteDefinition};

/// Calls `configure` (when there is one) with a new recording options object and gives what it
/// recorded; the object is disposed after the call. A callback that throws fails with its
/// message.
pub(crate) fn record<C, S>(
    options: C,
    configure: Option<Callback<Instance<C>>>,
    steps: impl FnOnce(&mut C) -> Vec<S>,
) -> Result<Vec<S>, String>
where
    C: nsh_core::element::Class,
{
    let Some(configure) = configure else {
        return Ok(Vec::new());
    };
    let options = Instance::new(options);
    let called = configure.call(options.clone());
    let recorded = if options.is_disposed() {
        Vec::new()
    } else {
        let recorded = options.with_mut(steps);
        options.dispose();
        recorded
    };
    called.map(|()| recorded)
}

/// The i18n service's options (`addI18n`), as Rust's `I18nOptions` sets them: each method sets
/// one and returns the object, so calls chain.
#[class]
pub struct I18nOptions {
    steps: Vec<I18nStep>,
}

/// What one `I18nOptions` method set.
#[derive(Clone, Debug)]
pub(crate) enum I18nStep {
    DefaultLanguage(String),
    FallbackLanguage(String),
    ResourcesPath(String),
    AutoLoadAssets(bool),
    FollowPlatform(bool),
    MissingMessage(MissingMessagePolicy),
    BidiIsolation(bool),
}

impl I18nStep {
    pub(crate) fn apply(&self, options: &mut I18n) {
        match self {
            Self::DefaultLanguage(language) => options.default_language(language.clone()),
            Self::FallbackLanguage(language) => options.fallback_language(language.clone()),
            Self::ResourcesPath(path) => options.resources_path(path.clone()),
            Self::AutoLoadAssets(enabled) => options.auto_load_assets(*enabled),
            Self::FollowPlatform(enabled) => options.follow_platform(*enabled),
            Self::MissingMessage(policy) => options.missing_message(*policy),
            Self::BidiIsolation(enabled) => options.bidi_isolation(*enabled),
        };
    }
}

impl I18nOptions {
    pub(crate) fn new() -> Self {
        Self { steps: Vec::new() }
    }

    pub(crate) fn take(&mut self) -> Vec<I18nStep> {
        std::mem::take(&mut self.steps)
    }
}

/// Records `step` on `this` and gives it back, for chaining.
fn push<C: nsh_core::element::Class>(this: Instance<C>, step: impl FnOnce(&mut C)) -> Instance<C> {
    this.with_mut(step);
    this
}

#[class]
impl I18nOptions {
    /// The language messages fall back to last, and the one shown before the user's is known
    /// (`"en"` by default).
    pub fn default_language(this: Instance<Self>, language: String) -> Instance<Self> {
        push(this, |o| o.steps.push(I18nStep::DefaultLanguage(language)))
    }

    /// Adds a language tried, in order, after the user's and before the default one.
    pub fn fallback_language(this: Instance<Self>, language: String) -> Instance<Self> {
        push(this, |o| o.steps.push(I18nStep::FallbackLanguage(language)))
    }

    /// The directory of the asset catalog's translations (`"locales"` by default).
    pub fn resources_path(this: Instance<Self>, path: String) -> Instance<Self> {
        push(this, |o| o.steps.push(I18nStep::ResourcesPath(path)))
    }

    /// Whether every translation of the asset catalog loads when the app starts (yes by
    /// default).
    pub fn auto_load_assets(this: Instance<Self>, enabled: bool) -> Instance<Self> {
        push(this, |o| o.steps.push(I18nStep::AutoLoadAssets(enabled)))
    }

    /// Whether the app follows the user's preferred languages and clock as the platform reports
    /// them, until it picks a language itself (yes by default).
    pub fn follow_platform(this: Instance<Self>, enabled: bool) -> Instance<Self> {
        push(this, |o| o.steps.push(I18nStep::FollowPlatform(enabled)))
    }

    /// What a missing message shows: `"id"` (its id, the default) or `"empty"` (nothing).
    pub fn missing_message(this: Instance<Self>, policy: String) -> Result<Instance<Self>, String> {
        let policy = match policy.as_str() {
            "id" => MissingMessagePolicy::Id,
            "empty" => MissingMessagePolicy::Empty,
            other => {
                return Err(format!(
                    "missingMessage takes \"id\" or \"empty\", not {other:?}"
                ));
            }
        };
        Ok(push(this, |o| {
            o.steps.push(I18nStep::MissingMessage(policy))
        }))
    }

    /// Whether placeholders whose direction is not the message's are wrapped in Unicode
    /// isolates (no by default).
    pub fn bidi_isolation(this: Instance<Self>, enabled: bool) -> Instance<Self> {
        push(this, |o| o.steps.push(I18nStep::BidiIsolation(enabled)))
    }
}

/// How a route `NavigationOptions.addRoute` adds is shown.
#[derive(Clone, Debug, Default, PartialEq, JsObject)]
pub struct RouteOptions {
    /// The component the route shows, by name.
    pub component: Option<String>,
    /// Whether its screen is made before it is first shown.
    pub prewarm: Option<bool>,
}

/// The navigation service's options (`addNavigation`), as Rust's `NavigationOptions` sets
/// them: `addRoute` adds a route and returns the object, so calls chain.
#[class]
pub struct NavigationOptions {
    routes: Vec<RouteDefinition>,
}

impl NavigationOptions {
    pub(crate) fn new() -> Self {
        Self { routes: Vec::new() }
    }

    pub(crate) fn take(&mut self) -> Vec<RouteDefinition> {
        std::mem::take(&mut self.routes)
    }

    pub(crate) fn apply(route: &RouteDefinition, options: &mut Navigation) {
        options.add_route(route.clone());
    }
}

#[class]
impl NavigationOptions {
    /// Adds the route `pattern` (`"/projects/{id}"`), with the component it shows and whether
    /// its screen is made early.
    pub fn add_route(
        this: Instance<Self>,
        pattern: String,
        options: Option<RouteOptions>,
    ) -> Instance<Self> {
        let options = options.unwrap_or_default();
        let mut route = RouteDefinition::new(pattern);
        route.component = options.component;
        route.prewarm = options.prewarm.unwrap_or(false);
        push(this, |o| o.routes.push(route))
    }
}

/// The asset service's options (`addAssets`), as Rust's `AssetOptions` sets them: `package`
/// adds a package and returns the object, so calls chain.
#[class]
pub struct AssetOptions {
    packages: Vec<String>,
}

impl AssetOptions {
    pub(crate) fn new() -> Self {
        Self {
            packages: Vec::new(),
        }
    }

    pub(crate) fn take(&mut self) -> Vec<String> {
        std::mem::take(&mut self.packages)
    }

    pub(crate) fn apply(package: &str, options: &mut Assets) {
        options.package(package);
    }
}

#[class]
impl AssetOptions {
    /// Adds `package` (a crate) to the packages whose `assets/` directories hold the app's
    /// assets, the app's own first.
    pub fn package(this: Instance<Self>, package: String) -> Instance<Self> {
        push(this, |o| o.packages.push(package))
    }
}

/// The font service's options (`addFonts`), as Rust's `FontOptions` sets them: each method sets
/// one and returns the object, so calls chain.
#[class]
pub struct FontOptions {
    steps: Vec<FontStep>,
}

/// What one `FontOptions` method set.
#[derive(Clone, Debug)]
pub(crate) enum FontStep {
    AutoRegisterAssets(bool),
    DefaultAlias(String),
    Alias(FontAlias),
}

impl FontStep {
    pub(crate) fn apply(&self, options: &mut Fonts) {
        match self {
            Self::AutoRegisterAssets(enabled) => {
                options.auto_register_assets(*enabled);
            }
            Self::DefaultAlias(alias) => {
                options.default_alias(alias.clone());
            }
            Self::Alias(alias) => {
                let alias = alias.clone();
                options.alias(alias.alias.clone(), move |configured| *configured = alias);
            }
        }
    }
}

impl FontOptions {
    pub(crate) fn new() -> Self {
        Self { steps: Vec::new() }
    }

    pub(crate) fn take(&mut self) -> Vec<FontStep> {
        std::mem::take(&mut self.steps)
    }
}

#[class]
impl FontOptions {
    /// Whether every font asset of the asset catalog is registered when the app starts (yes by
    /// default).
    pub fn auto_register_assets(this: Instance<Self>, enabled: bool) -> Instance<Self> {
        push(this, |o| {
            o.steps.push(FontStep::AutoRegisterAssets(enabled))
        })
    }

    /// The alias the app's text uses by default.
    pub fn default_alias(this: Instance<Self>, alias: String) -> Instance<Self> {
        push(this, |o| o.steps.push(FontStep::DefaultAlias(alias)))
    }

    /// Adds the alias `alias` (its family is the alias itself unless `configure` names one),
    /// with `configure` setting its family and fallbacks; a `configure` that throws makes this
    /// call throw.
    pub fn alias(
        this: Instance<Self>,
        alias: String,
        #[args(alias)] configure: Option<Callback<Instance<FontAliasOptions>>>,
    ) -> Result<Instance<Self>, String> {
        let mut font = FontAlias::new(alias);
        for step in record(FontAliasOptions::new(), configure, FontAliasOptions::take)? {
            step.apply(&mut font);
        }
        Ok(push(this, |o| o.steps.push(FontStep::Alias(font))))
    }
}

/// A font alias's settings (`FontOptions.alias`'s callback), as Rust's `FontAlias` sets them:
/// each method sets one and returns the object, so calls chain.
#[class]
pub struct FontAliasOptions {
    steps: Vec<FontAliasStep>,
}

/// What one `FontAliasOptions` method set.
#[derive(Clone, Debug)]
pub(crate) enum FontAliasStep {
    Family(String),
    LoadFallbackAlias(String),
    LoadFallbackFamily(String),
    GlyphFallback(String),
}

impl FontAliasStep {
    fn apply(&self, alias: &mut FontAlias) {
        match self {
            Self::Family(family) => alias.family(family.clone()),
            Self::LoadFallbackAlias(other) => alias.load_fallback_alias(other.clone()),
            Self::LoadFallbackFamily(family) => alias.load_fallback_family(family.clone()),
            Self::GlyphFallback(other) => alias.glyph_fallback(other.clone()),
        };
    }
}

impl FontAliasOptions {
    fn new() -> Self {
        Self { steps: Vec::new() }
    }

    fn take(&mut self) -> Vec<FontAliasStep> {
        std::mem::take(&mut self.steps)
    }
}

#[class]
impl FontAliasOptions {
    /// The family the alias names.
    pub fn family(this: Instance<Self>, family: String) -> Instance<Self> {
        push(this, |o| o.steps.push(FontAliasStep::Family(family)))
    }

    /// The alias used while this one's font loads, or when it does not.
    pub fn load_fallback_alias(this: Instance<Self>, alias: String) -> Instance<Self> {
        push(this, |o| {
            o.steps.push(FontAliasStep::LoadFallbackAlias(alias))
        })
    }

    /// The family used while this alias's font loads, or when it does not.
    pub fn load_fallback_family(this: Instance<Self>, family: String) -> Instance<Self> {
        push(this, |o| {
            o.steps.push(FontAliasStep::LoadFallbackFamily(family))
        })
    }

    /// Adds an alias whose font draws the glyphs this one's lacks, in order.
    pub fn glyph_fallback(this: Instance<Self>, alias: String) -> Instance<Self> {
        push(this, |o| o.steps.push(FontAliasStep::GlyphFallback(alias)))
    }
}

/// The input map's options (`addInputMap`), as Rust's `InputMapOptions` sets them: `bindKey`
/// adds a binding and returns the object, so calls chain.
#[class]
pub struct InputMapOptions {
    bindings: Vec<(String, String, String)>,
}

impl InputMapOptions {
    pub(crate) fn new() -> Self {
        Self {
            bindings: Vec::new(),
        }
    }

    pub(crate) fn take(&mut self) -> Vec<(String, String, String)> {
        std::mem::take(&mut self.bindings)
    }

    pub(crate) fn apply(binding: &(String, String, String), options: &mut InputMap) {
        let (context, sequence, action) = binding.clone();
        options.bind_key(context, sequence, action);
    }
}

#[class]
impl InputMapOptions {
    /// Adds an application binding of the key sequence `sequence` (`"ctrl-k ctrl-c"`) to
    /// `action` in `context`.
    pub fn bind_key(
        this: Instance<Self>,
        context: String,
        sequence: String,
        action: String,
    ) -> Instance<Self> {
        push(this, |o| o.bindings.push((context, sequence, action)))
    }
}

/// The app's configuration settings (`addApplicationConfiguration`), as Rust's configure
/// closure sets `ApplicationConfiguration`: `name` and `description`, and `merge` for the rest
/// (motion, scrolling, tooltips, hover cards, notifications, layout). Each returns the object, so
/// calls chain.
#[class]
pub struct ApplicationConfigurationOptions {
    steps: Vec<ApplicationConfigurationStep>,
}

/// What one `ApplicationConfigurationOptions` method set.
#[derive(Clone, Debug)]
pub(crate) enum ApplicationConfigurationStep {
    Name(String),
    Description(String),
    Merge(JsonValue),
}

impl ApplicationConfigurationStep {
    pub(crate) fn apply(&self, settings: &mut ApplicationConfiguration) {
        match self {
            Self::Name(name) => settings.name = name.clone(),
            Self::Description(description) => settings.description = description.clone(),
            Self::Merge(patch) => {
                // Checked when it was recorded: it reads over any settings.
                if let Ok(merged) = merged(settings, patch) {
                    *settings = merged;
                }
            }
        }
    }
}

/// `settings` with `patch` merged over them: its objects into theirs, its other values in place
/// of theirs, keys matched as configuration matches them (ignoring ASCII case, `_` and `-`).
fn merged(
    settings: &ApplicationConfiguration,
    patch: &JsonValue,
) -> Result<ApplicationConfiguration, String> {
    fn key(text: &str) -> String {
        text.chars()
            .filter(|c| *c != '_' && *c != '-')
            .map(|c| c.to_ascii_lowercase())
            .collect()
    }
    fn merge(target: &mut JsonValue, patch: &JsonValue) {
        match (target, patch) {
            (JsonValue::Object(target), JsonValue::Object(patch)) => {
                for (name, value) in patch {
                    let existing = target.keys().find(|k| key(k) == key(name)).cloned();
                    match existing.and_then(|existing| target.get_mut(&existing)) {
                        Some(slot) => merge(slot, value),
                        None => {
                            target.insert(name.clone(), value.clone());
                        }
                    }
                }
            }
            (target, patch) => *target = patch.clone(),
        }
    }
    let mut value = serde_json::to_value(settings).map_err(|error| error.to_string())?;
    merge(&mut value, patch);
    serde_json::from_value(value).map_err(|error| error.to_string())
}

impl ApplicationConfigurationOptions {
    pub(crate) fn new() -> Self {
        Self { steps: Vec::new() }
    }

    pub(crate) fn take(&mut self) -> Vec<ApplicationConfigurationStep> {
        std::mem::take(&mut self.steps)
    }
}

#[class]
impl ApplicationConfigurationOptions {
    /// The app's name.
    pub fn name(this: Instance<Self>, name: String) -> Instance<Self> {
        push(this, |o| {
            o.steps.push(ApplicationConfigurationStep::Name(name))
        })
    }

    /// What the app is.
    pub fn description(this: Instance<Self>, description: String) -> Instance<Self> {
        push(this, |o| {
            o.steps
                .push(ApplicationConfigurationStep::Description(description))
        })
    }

    /// Merges `settings` (an object, as `RayX:Application` in `appsettings.json` has them:
    /// `{ notifications: { maxItems: 3 } }`) over the settings: its objects into theirs, its
    /// other values in their place. Throws for a value of the wrong type.
    pub fn merge(this: Instance<Self>, settings: JsonValue) -> Result<Instance<Self>, String> {
        if !settings.is_object() {
            return Err("merge takes an object of settings".into());
        }
        merged(&ApplicationConfiguration::default(), &settings)
            .map_err(|error| format!("merge: {error}"))?;
        Ok(push(this, |o| {
            o.steps.push(ApplicationConfigurationStep::Merge(settings))
        }))
    }
}

/// The app's configuration sources (`configureAppConfiguration`'s callback), as Rust's
/// `ConfigurationBuilder` adds them: each method adds one, after those before it, and returns
/// the object, so calls chain.
#[class]
pub struct ConfigurationBuilder {
    steps: Vec<ConfigurationStep>,
}

/// What one `ConfigurationBuilder` method added.
#[derive(Clone, Debug)]
pub(crate) enum ConfigurationStep {
    Json(String),
    InMemory(Vec<(String, JsonValue)>),
    Value(JsonValue),
}

impl ConfigurationStep {
    pub(crate) fn apply(self, configuration: &mut Configuration) {
        match self {
            Self::Json(json) => configuration.add_json(json),
            Self::InMemory(values) => configuration.add_in_memory(values),
            Self::Value(value) => configuration.add_value(value),
        };
    }
}

impl ConfigurationBuilder {
    pub(crate) fn new() -> Self {
        Self { steps: Vec::new() }
    }

    pub(crate) fn take(&mut self) -> Vec<ConfigurationStep> {
        std::mem::take(&mut self.steps)
    }
}

#[class]
impl ConfigurationBuilder {
    /// Adds JSON text (an object) as a source.
    pub fn add_json(this: Instance<Self>, json: String) -> Instance<Self> {
        push(this, |o| o.steps.push(ConfigurationStep::Json(json)))
    }

    /// Adds values by path (.NET's `AddInMemoryCollection`):
    /// `{ "Feed:Label": "live", "Feed:Retry:Limit": 3 }`.
    pub fn add_in_memory(
        this: Instance<Self>,
        values: JsonValue,
    ) -> Result<Instance<Self>, String> {
        let JsonValue::Object(values) = values else {
            return Err("addInMemory takes an object of values by path".into());
        };
        let values = values.into_iter().collect();
        Ok(push(this, |o| {
            o.steps.push(ConfigurationStep::InMemory(values))
        }))
    }

    /// Adds a tree of values (an object) as a source: `{ Feed: { Label: "live" } }`.
    pub fn add_value(this: Instance<Self>, value: JsonValue) -> Instance<Self> {
        push(this, |o| o.steps.push(ConfigurationStep::Value(value)))
    }
}
