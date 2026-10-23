use super::{Options, OptionsError};
use std::any::{Any, TypeId, type_name};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Default)]
pub struct OptionsRegistry {
    values: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

impl OptionsRegistry {
    pub fn insert<T>(&mut self, value: T)
    where
        T: Send + Sync + 'static,
    {
        self.insert_options(Options::new(value));
    }

    pub fn insert_options<T>(&mut self, options: Options<T>)
    where
        T: Send + Sync + 'static,
    {
        self.values.insert(TypeId::of::<T>(), Arc::new(options));
    }

    pub fn options<T>(&self) -> Option<Options<T>>
    where
        T: Send + Sync + 'static,
    {
        self.values
            .get(&TypeId::of::<T>())
            .and_then(|value| value.downcast_ref::<Options<T>>())
            .cloned()
    }

    pub fn required<T>(&self) -> Result<Options<T>, OptionsError>
    where
        T: Send + Sync + 'static,
    {
        self.options::<T>().ok_or(OptionsError::Missing {
            type_name: type_name::<T>(),
        })
    }

    pub fn value<T>(&self) -> Option<&T>
    where
        T: Send + Sync + 'static,
    {
        self.values
            .get(&TypeId::of::<T>())
            .and_then(|value| value.downcast_ref::<Options<T>>())
            .map(Options::value)
    }

    pub fn contains<T>(&self) -> bool
    where
        T: Send + Sync + 'static,
    {
        self.values.contains_key(&TypeId::of::<T>())
    }

    pub fn extend(&mut self, other: Self) {
        self.values.extend(other.values);
    }
}

impl std::fmt::Debug for OptionsRegistry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OptionsRegistry")
            .field("len", &self.values.len())
            .finish()
    }
}
