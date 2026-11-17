//! The application configuration's options for TSX (`addApplicationConfiguration`).

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx_runtime::services::application_configuration::ApplicationConfiguration;

use crate::configuration::options_ts::push;

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
