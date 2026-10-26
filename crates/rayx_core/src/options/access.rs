//! The ways to read options: once per app ([`Options`] through
//! [`ServiceProvider::options`]), once per scope ([`OptionsSnapshot`]) and as they change
//! ([`OptionsMonitor`]).

use super::pipeline::OptionsFactory;
use super::{DEFAULT_NAME, Options, OptionsError};
use crate::change::{ChangeSubscription, Listeners};
use crate::di::ServiceProvider;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

/// Options made by a factory, cached by name.
struct Cache<T> {
    factory: OptionsFactory<T>,
    values: Mutex<HashMap<String, Result<Options<T>, OptionsError>>>,
}

impl<T> Cache<T>
where
    T: Default + 'static,
{
    fn new(factory: OptionsFactory<T>) -> Self {
        Self {
            factory,
            values: Mutex::new(HashMap::new()),
        }
    }

    fn get(&self, name: &str) -> Result<Options<T>, OptionsError> {
        if let Some(made) = self.values().get(name) {
            return made.clone();
        }
        // Made outside the lock: a step may read other options.
        let made = self.factory.create(name).map(Options::new);
        self.values().entry(name.to_owned()).or_insert(made).clone()
    }

    fn forget(&self, name: &str) {
        self.values().remove(name);
    }

    fn values(
        &self,
    ) -> std::sync::MutexGuard<'_, HashMap<String, Result<Options<T>, OptionsError>>> {
        self.values.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Options made once per scope (.NET's `IOptionsSnapshot<T>`): a scope sees the configuration as
/// it was when it first read them, by name. Resolve it with
/// [`ServiceProvider::options_snapshot`]; a clone shares the snapshot.
pub struct OptionsSnapshot<T> {
    cache: Arc<Cache<T>>,
}

impl<T> OptionsSnapshot<T>
where
    T: Default + Send + Sync + 'static,
{
    pub(crate) fn new(factory: OptionsFactory<T>) -> Self {
        Self {
            cache: Arc::new(Cache::new(factory)),
        }
    }

    /// The unnamed options.
    pub fn value(&self) -> Result<Options<T>, OptionsError> {
        self.get(DEFAULT_NAME)
    }

    /// The options named `name`.
    pub fn get(&self, name: &str) -> Result<Options<T>, OptionsError> {
        self.cache.get(name)
    }
}

impl<T> Clone for OptionsSnapshot<T> {
    fn clone(&self) -> Self {
        Self {
            cache: Arc::clone(&self.cache),
        }
    }
}

impl<T> std::fmt::Debug for OptionsSnapshot<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OptionsSnapshot")
            .field("type", &std::any::type_name::<T>())
            .finish()
    }
}

/// The app-wide options cache behind [`ServiceProvider::options`] (.NET's `IOptions<T>`): made
/// once, never refreshed. A singleton of its own type.
pub(crate) struct AppOptions<T>(pub(crate) OptionsSnapshot<T>);

type ChangeListener<T> = dyn Fn(&Options<T>, &str) + Send + Sync;

/// Options as they change (.NET's `IOptionsMonitor<T>`): a singleton that makes options by name
/// and keeps them until the configuration a name is bound to changes ([`Configuration::set`],
/// [`Configuration::reload`]); then it makes them again and tells [`Self::on_change`]'s
/// listeners. Resolve it with [`ServiceProvider::options_monitor`]; a clone is the same monitor.
///
/// [`Configuration::set`]: crate::configuration::Configuration::set
/// [`Configuration::reload`]: crate::configuration::Configuration::reload
pub struct OptionsMonitor<T> {
    inner: Arc<Monitor<T>>,
}

struct Monitor<T> {
    cache: Cache<T>,
    bound_names: Vec<String>,
    listeners: Listeners<ChangeListener<T>>,
    configuration_change: Mutex<Option<ChangeSubscription>>,
}

impl<T> OptionsMonitor<T>
where
    T: Default + Send + Sync + 'static,
{
    pub(crate) fn new(factory: OptionsFactory<T>) -> Self {
        let configuration = factory.configuration.clone();
        let inner = Arc::new(Monitor {
            bound_names: factory.bound_names.clone(),
            cache: Cache::new(factory),
            listeners: Listeners::default(),
            configuration_change: Mutex::new(None),
        });
        if let Some(configuration) = configuration {
            let monitor = Arc::downgrade(&inner);
            let subscription = configuration.on_change(move || {
                if let Some(monitor) = monitor.upgrade() {
                    monitor.refresh();
                }
            });
            *inner
                .configuration_change
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = Some(subscription);
        }
        Self { inner }
    }

    /// The unnamed options now.
    pub fn current_value(&self) -> Result<Options<T>, OptionsError> {
        self.get(DEFAULT_NAME)
    }

    /// The options named `name` now.
    pub fn get(&self, name: &str) -> Result<Options<T>, OptionsError> {
        self.inner.cache.get(name)
    }

    /// Calls `listener` with the new options and their name each time options bound to the
    /// configuration are made again after it changed, until the returned subscription is
    /// dropped. Options that fail to make again call no listener; [`Self::get`] returns the
    /// error.
    pub fn on_change(
        &self,
        listener: impl Fn(&Options<T>, &str) + Send + Sync + 'static,
    ) -> ChangeSubscription {
        self.inner.listeners.add(Arc::new(listener))
    }
}

impl<T> Monitor<T>
where
    T: Default + Send + Sync + 'static,
{
    fn refresh(&self) {
        for name in &self.bound_names {
            self.cache.forget(name);
            if let Ok(options) = self.cache.get(name) {
                for listener in self.listeners.current() {
                    listener(&options, name);
                }
            }
        }
    }
}

impl<T> Clone for OptionsMonitor<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T> std::fmt::Debug for OptionsMonitor<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OptionsMonitor")
            .field("type", &std::any::type_name::<T>())
            .field("bound_names", &self.inner.bound_names)
            .finish()
    }
}

impl ServiceProvider {
    /// The unnamed options `T` (.NET's `IOptions<T>.Value`): made with the first call and the
    /// same from then on, in every scope. Options without registrations are `T::default()`.
    pub fn options<T>(&self) -> Result<Options<T>, OptionsError>
    where
        T: Default + Send + Sync + 'static,
    {
        match self.service::<AppOptions<T>>() {
            Some(options) => options.0.value(),
            None => Ok(Options::new(T::default())),
        }
    }

    /// This scope's snapshot of the options `T` (.NET's `IOptionsSnapshot<T>`).
    pub fn options_snapshot<T>(&self) -> OptionsSnapshot<T>
    where
        T: Default + Send + Sync + 'static,
    {
        self.service::<OptionsSnapshot<T>>()
            .map(|snapshot| (*snapshot).clone())
            .unwrap_or_else(|| OptionsSnapshot::new(OptionsFactory::empty()))
    }

    /// The monitor of the options `T` (.NET's `IOptionsMonitor<T>`).
    pub fn options_monitor<T>(&self) -> OptionsMonitor<T>
    where
        T: Default + Send + Sync + 'static,
    {
        self.service::<OptionsMonitor<T>>()
            .map(|monitor| (*monitor).clone())
            .unwrap_or_else(|| OptionsMonitor::new(OptionsFactory::empty()))
    }

    /// Makes every option registered with `validate_on_start` (.NET's `ValidateOnStart`), so
    /// that invalid options fail startup rather than their first use; every failure is
    /// returned. Startup ([`StartupRegistry::initialize`]) does this before it initializes the
    /// startup services.
    ///
    /// [`StartupRegistry::initialize`]: crate::di::StartupRegistry::initialize
    pub fn validate_options_on_start(&self) -> Result<(), Vec<OptionsError>> {
        let failures = self
            .options_registrations()
            .validations_on_start()
            .filter_map(|validate| validate(self).err())
            .collect::<Vec<_>>();
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures)
        }
    }
}
