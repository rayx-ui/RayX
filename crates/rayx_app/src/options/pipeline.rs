//! The steps that make options, and the factory that runs them (.NET's `IConfigureOptions`,
//! `IPostConfigureOptions`, `IValidateOptions` and `OptionsFactory`).

use super::OptionsError;
use crate::configuration::{Configuration, ConfigurationError};
use crate::di::{ServiceProvider, ServiceResolutionError};
use std::any::{Any, TypeId, type_name};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Stage {
    Configure,
    PostConfigure,
    Validate,
}

type Configure<T> = Box<dyn Fn(&mut T) -> Result<(), ConfigurationError> + Send + Sync>;

/// What a step does once its dependencies are resolved.
pub(crate) enum Action<T> {
    Configure(Configure<T>),
    Validate(Box<dyn Fn(&T) -> bool + Send + Sync>, String),
}

type MakeAction<T> =
    Arc<dyn Fn(&ServiceProvider) -> Result<Action<T>, ServiceResolutionError> + Send + Sync>;

/// One registered step: the names it applies to, its stage and how to make its action with a
/// provider (resolving its dependency, if any).
pub(crate) struct Step<T> {
    /// `None`: every name (`configure_all`, `post_configure_all`).
    name: Option<String>,
    stage: Stage,
    /// Binds a configuration section: an options monitor refreshes the name on a change.
    binds: bool,
    make: MakeAction<T>,
}

impl<T> Clone for Step<T> {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            stage: self.stage,
            binds: self.binds,
            make: Arc::clone(&self.make),
        }
    }
}

impl<T> Step<T>
where
    T: 'static,
{
    pub(crate) fn new(
        name: Option<String>,
        stage: Stage,
        make: impl Fn(&ServiceProvider) -> Result<Action<T>, ServiceResolutionError>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self {
            name,
            stage,
            binds: false,
            make: Arc::new(make),
        }
    }

    /// A configure or post-configure step without dependencies.
    pub(crate) fn configure(
        name: Option<String>,
        stage: Stage,
        configure: impl Fn(&mut T) + Send + Sync + 'static,
    ) -> Self {
        let configure = Arc::new(configure);
        Self::new(name, stage, move |_| {
            let configure = Arc::clone(&configure);
            Ok(Action::Configure(Box::new(move |options| {
                configure(options);
                Ok(())
            })))
        })
    }

    /// A configure or post-configure step that takes the service `D`.
    pub(crate) fn configure_with<D>(
        name: Option<String>,
        stage: Stage,
        configure: impl Fn(&mut T, &D) + Send + Sync + 'static,
    ) -> Self
    where
        D: ?Sized + Send + Sync + 'static,
    {
        let configure = Arc::new(configure);
        Self::new(name, stage, move |services| {
            let dependency = services.required_service::<D>()?;
            let configure = Arc::clone(&configure);
            Ok(Action::Configure(Box::new(move |options| {
                configure(options, &dependency);
                Ok(())
            })))
        })
    }

    /// A configure step binding the configuration's section at `path`.
    pub(crate) fn bind(name: String, path: String) -> Self
    where
        T: serde::Serialize + serde::de::DeserializeOwned,
    {
        let mut step = Self::new(Some(name), Stage::Configure, move |services| {
            let configuration = services.required_service::<Configuration>()?;
            let path = path.clone();
            Ok(Action::Configure(Box::new(move |options| {
                configuration.bind(&path, options)
            })))
        });
        step.binds = true;
        step
    }

    /// A validation without dependencies.
    pub(crate) fn validate(
        name: Option<String>,
        validate: impl Fn(&T) -> bool + Send + Sync + 'static,
        failure_message: String,
    ) -> Self {
        let validate = Arc::new(validate);
        Self::new(name, Stage::Validate, move |_| {
            let validate = Arc::clone(&validate);
            Ok(Action::Validate(
                Box::new(move |options| validate(options)),
                failure_message.clone(),
            ))
        })
    }

    /// A validation that takes the service `D`.
    pub(crate) fn validate_with<D>(
        name: Option<String>,
        validate: impl Fn(&T, &D) -> bool + Send + Sync + 'static,
        failure_message: String,
    ) -> Self
    where
        D: ?Sized + Send + Sync + 'static,
    {
        let validate = Arc::new(validate);
        Self::new(name, Stage::Validate, move |services| {
            let dependency = services.required_service::<D>()?;
            let validate = Arc::clone(&validate);
            Ok(Action::Validate(
                Box::new(move |options| validate(options, &dependency)),
                failure_message.clone(),
            ))
        })
    }
}

/// The steps registered for `T`, in registration order.
pub(crate) struct Pipeline<T> {
    steps: Vec<Step<T>>,
}

impl<T> Clone for Pipeline<T> {
    fn clone(&self) -> Self {
        Self {
            steps: self.steps.clone(),
        }
    }
}

impl<T> Default for Pipeline<T> {
    fn default() -> Self {
        Self { steps: Vec::new() }
    }
}

type StartValidation = Arc<dyn Fn(&ServiceProvider) -> Result<(), OptionsError> + Send + Sync>;

/// Every options type's pipeline, and the options validated on start.
#[derive(Clone, Default)]
pub(crate) struct OptionsRegistrations {
    pipelines: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
    on_start: Vec<(TypeId, String, StartValidation)>,
}

impl std::fmt::Debug for OptionsRegistrations {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OptionsRegistrations")
            .field("types", &self.pipelines.len())
            .field("validated_on_start", &self.on_start.len())
            .finish()
    }
}

impl OptionsRegistrations {
    pub(crate) fn pipeline<T>(&self) -> Option<Arc<Pipeline<T>>>
    where
        T: 'static,
    {
        self.pipelines
            .get(&TypeId::of::<T>())
            .cloned()
            .and_then(|pipeline| pipeline.downcast::<Pipeline<T>>().ok())
    }

    /// Adds `step` to `T`'s pipeline, making the pipeline if `T` has none; returns whether it
    /// was made.
    pub(crate) fn add<T>(&mut self, step: Option<Step<T>>) -> bool
    where
        T: 'static,
    {
        let existing = self.pipeline::<T>();
        let made = existing.is_none();
        let mut pipeline = existing
            .map(|pipeline| (*pipeline).clone())
            .unwrap_or_default();
        pipeline.steps.extend(step);
        self.pipelines.insert(TypeId::of::<T>(), Arc::new(pipeline));
        made
    }

    pub(crate) fn validate_on_start<T>(&mut self, name: String, validate: StartValidation)
    where
        T: 'static,
    {
        let type_id = TypeId::of::<T>();
        if !self
            .on_start
            .iter()
            .any(|(other, other_name, _)| *other == type_id && *other_name == name)
        {
            self.on_start.push((type_id, name, validate));
        }
    }

    pub(crate) fn validations_on_start(&self) -> impl Iterator<Item = &StartValidation> {
        self.on_start.iter().map(|(_, _, validate)| validate)
    }
}

/// A step's names, stage and action (or its dependency's failure to resolve).
type MadeStep<T> = (
    Option<String>,
    Stage,
    Result<Action<T>, ServiceResolutionError>,
);

/// `T`'s steps with their actions made for one provider: makes options by name.
pub(crate) struct OptionsFactory<T> {
    steps: Vec<MadeStep<T>>,
    /// The names a configuration section is bound to.
    pub(crate) bound_names: Vec<String>,
    /// The configuration the bound sections come from.
    pub(crate) configuration: Option<Arc<Configuration>>,
}

impl<T> OptionsFactory<T>
where
    T: Default + 'static,
{
    pub(crate) fn empty() -> Self {
        Self {
            steps: Vec::new(),
            bound_names: Vec::new(),
            configuration: None,
        }
    }

    pub(crate) fn new(pipeline: Option<&Pipeline<T>>, services: &ServiceProvider) -> Self {
        let Some(pipeline) = pipeline else {
            return Self::empty();
        };
        let mut bound_names = Vec::new();
        for step in pipeline.steps.iter().filter(|step| step.binds) {
            if let Some(name) = &step.name
                && !bound_names.contains(name)
            {
                bound_names.push(name.clone());
            }
        }
        Self {
            steps: pipeline
                .steps
                .iter()
                .map(|step| (step.name.clone(), step.stage, (step.make)(services)))
                .collect(),
            configuration: if bound_names.is_empty() {
                None
            } else {
                services.service::<Configuration>()
            },
            bound_names,
        }
    }

    /// Makes the options named `name`: `T::default()`, every configure step for the name (a
    /// bind is one) in registration order, then every post-configure step, then every
    /// validation, failing with all of their failures.
    pub(crate) fn create(&self, name: &str) -> Result<T, OptionsError> {
        let mut options = T::default();
        let steps = |stage: Stage| {
            self.steps.iter().filter(move |(own, step_stage, _)| {
                *step_stage == stage && own.as_deref().is_none_or(|own| own == name)
            })
        };
        let action = |made| made_action(made, name);
        for stage in [Stage::Configure, Stage::PostConfigure] {
            for (_, _, made) in steps(stage) {
                if let Action::Configure(configure) = action(made)? {
                    configure(&mut options).map_err(|error| OptionsError::Bind {
                        type_name: type_name::<T>(),
                        name: name.to_owned(),
                        error,
                    })?;
                }
            }
        }
        let mut failures = Vec::new();
        for (_, _, made) in steps(Stage::Validate) {
            if let Action::Validate(validate, message) = action(made)?
                && !validate(&options)
            {
                failures.push(message.clone());
            }
        }
        if failures.is_empty() {
            Ok(options)
        } else {
            Err(OptionsError::Validation {
                type_name: type_name::<T>(),
                name: name.to_owned(),
                failures,
            })
        }
    }
}

/// A step's action, or the failure to resolve its dependency.
fn made_action<'a, T>(
    made: &'a Result<Action<T>, ServiceResolutionError>,
    name: &str,
) -> Result<&'a Action<T>, OptionsError> {
    made.as_ref().map_err(|error| OptionsError::Dependency {
        type_name: type_name::<T>(),
        name: name.to_owned(),
        error: error.clone(),
    })
}
