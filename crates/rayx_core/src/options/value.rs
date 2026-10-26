use std::ops::Deref;
use std::sync::Arc;

/// Options made by the options pipeline (.NET's `IOptions<T>.Value`): shared and immutable.
pub struct Options<T> {
    value: Arc<T>,
}

impl<T> Options<T> {
    pub fn new(value: T) -> Self {
        Self {
            value: Arc::new(value),
        }
    }

    pub fn value(&self) -> &T {
        &self.value
    }

    pub fn cloned(&self) -> T
    where
        T: Clone,
    {
        self.value.as_ref().clone()
    }

    /// Whether two options are the same instance (made once, cached).
    pub fn ptr_eq(left: &Self, right: &Self) -> bool {
        Arc::ptr_eq(&left.value, &right.value)
    }
}

impl<T> Deref for Options<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> Clone for Options<T> {
    fn clone(&self) -> Self {
        Self {
            value: Arc::clone(&self.value),
        }
    }
}

impl<T> std::fmt::Debug for Options<T>
where
    T: std::fmt::Debug,
{
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Options")
            .field("value", self.value())
            .finish()
    }
}
