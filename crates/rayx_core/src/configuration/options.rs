use super::{ConfigurationError, OptionsFinalizeError};
use crate::options::OptionsRegistry;
use std::any::{Any, TypeId, type_name};
use std::collections::HashMap;

type BaseFactory<T> = Box<dyn FnOnce() -> T>;
type OptionsConfigurator<T> = Box<dyn FnOnce(&mut T)>;
type OptionsFinalizer<T> = Box<dyn FnOnce(&mut T) -> Result<(), OptionsFinalizeError>>;

#[derive(Default)]
pub(crate) struct OptionsPlans {
    plans: Vec<Box<dyn ErasedOptionsPlan>>,
    indices: HashMap<TypeId, usize>,
}

impl OptionsPlans {
    pub(crate) fn len(&self) -> usize {
        self.plans.len()
    }

    pub(crate) fn configure<T>(
        &mut self,
        section: String,
        base_factory: impl FnOnce() -> T + 'static,
        configure: impl FnOnce(&mut T) + 'static,
        finalize: impl FnOnce(&mut T) -> Result<(), OptionsFinalizeError> + 'static,
    ) where
        T: Send + Sync + 'static,
    {
        let type_id = TypeId::of::<T>();
        if let Some(index) = self.indices.get(&type_id).copied() {
            let existing = &mut self.plans[index];
            if let Some(plan) = existing.as_any_mut().downcast_mut::<OptionsPlan<T>>() {
                plan.append(section, configure);
                return;
            }

            if existing
                .as_any_mut()
                .downcast_mut::<ExactOptionsPlan<T>>()
                .is_some()
            {
                return;
            }

            self.plans[index] = Box::new(OptionsPlan::layered(
                section,
                base_factory,
                configure,
                finalize,
            ));
            return;
        }

        let index = self.plans.len();
        self.plans.push(Box::new(OptionsPlan::layered(
            section,
            base_factory,
            configure,
            finalize,
        )));
        self.indices.insert(type_id, index);
    }

    pub(crate) fn exact<T>(&mut self, value: T)
    where
        T: Send + Sync + 'static,
    {
        let type_id = TypeId::of::<T>();
        let plan: Box<dyn ErasedOptionsPlan> = Box::new(ExactOptionsPlan { value });
        if let Some(index) = self.indices.get(&type_id).copied() {
            self.plans[index] = plan;
        } else {
            let index = self.plans.len();
            self.plans.push(plan);
            self.indices.insert(type_id, index);
        }
    }

    /// Materializes every plan; on the first failure nothing is published.
    pub(crate) fn materialize(self) -> Result<OptionsRegistry, ConfigurationError> {
        let mut registry = OptionsRegistry::default();
        for plan in self.plans {
            plan.materialize(&mut registry)?;
        }
        Ok(registry)
    }
}

trait ErasedOptionsPlan {
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn materialize(
        self: Box<Self>,
        registry: &mut OptionsRegistry,
    ) -> Result<(), ConfigurationError>;
}

struct OptionsPlan<T> {
    section: String,
    conflict: Option<String>,
    base_factory: BaseFactory<T>,
    configurators: Vec<OptionsConfigurator<T>>,
    finalizer: OptionsFinalizer<T>,
}

impl<T> OptionsPlan<T> {
    fn layered(
        section: String,
        base_factory: impl FnOnce() -> T + 'static,
        configure: impl FnOnce(&mut T) + 'static,
        finalize: impl FnOnce(&mut T) -> Result<(), OptionsFinalizeError> + 'static,
    ) -> Self {
        Self {
            section,
            conflict: None,
            base_factory: Box::new(base_factory),
            configurators: vec![Box::new(configure)],
            finalizer: Box::new(finalize),
        }
    }

    fn append(&mut self, section: String, configure: impl FnOnce(&mut T) + 'static) {
        if !self.section.eq_ignore_ascii_case(&section) && self.conflict.is_none() {
            self.conflict = Some(section);
        }
        self.configurators.push(Box::new(configure));
    }
}

impl<T> ErasedOptionsPlan for OptionsPlan<T>
where
    T: Send + Sync + 'static,
{
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn materialize(
        self: Box<Self>,
        registry: &mut OptionsRegistry,
    ) -> Result<(), ConfigurationError> {
        let Self {
            section,
            conflict,
            base_factory,
            configurators,
            finalizer,
        } = *self;
        if let Some(requested_section) = conflict {
            return Err(ConfigurationError::OptionsPlanConflict {
                type_name: type_name::<T>(),
                existing_section: section,
                requested_section,
            });
        }

        let mut options = base_factory();
        for configure in configurators {
            configure(&mut options);
        }
        finalizer(&mut options).map_err(|source| ConfigurationError::OptionsFinalize {
            section: section.clone(),
            type_name: type_name::<T>(),
            stage: source.stage(),
            source: Box::new(source),
        })?;
        registry.insert(options);
        Ok(())
    }
}

struct ExactOptionsPlan<T> {
    value: T,
}

impl<T> ErasedOptionsPlan for ExactOptionsPlan<T>
where
    T: Send + Sync + 'static,
{
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn materialize(
        self: Box<Self>,
        registry: &mut OptionsRegistry,
    ) -> Result<(), ConfigurationError> {
        registry.insert(self.value);
        Ok(())
    }
}
