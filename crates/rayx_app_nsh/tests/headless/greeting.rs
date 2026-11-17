//! A service of the tests' own, as a service crate's would be: `builder.addGreeting()` (a class
//! extension) registers a [`Greeting`], which the class [`GreetingService`] (registered with
//! `service_class`, so it is its own token) reaches. The host's TS tests resolve it where they
//! need a RayX service's class ([`register`] adds both to a set).

use std::sync::Arc;

use ::nsh_core::element::Registry;
use ::nsh_core::prelude::*;
use ::rayx_app_nsh::ts::{Listeners, RayXAppBuilder, ServiceClass, ServiceClassRegistryExt as _};
use ::rayx_app_nsh::{AppBuilder as _, ChangeSubscription, StateCell};

/// The greeting: a text, published as it changes.
pub struct Greeting {
    text: StateCell<String>,
}

/// The greeting's class: `services.getRequiredService(GreetingService)`.
#[class]
pub struct GreetingService {
    greeting: Arc<Greeting>,
    listeners: Listeners,
}

impl ServiceClass for GreetingService {
    type Service = Greeting;

    fn wrap(greeting: Arc<Greeting>) -> Self {
        Self {
            greeting,
            listeners: Listeners::default(),
        }
    }
}

#[class]
impl GreetingService {
    /// The greeting now.
    pub fn current(&self) -> String {
        (*self.greeting.text.get()).clone()
    }

    /// Changes the greeting to `text`.
    pub fn set(&self, text: String) {
        self.greeting.text.publish(text);
    }

    /// Calls `listener` with the greeting after every change, until the function it returns is
    /// called (or the reactive owner of the call goes), or this object is disposed.
    pub fn on_change(
        &mut self,
        #[args(greeting)] listener: Callback<String>,
    ) -> Result<Stop, String> {
        let greeting = Arc::clone(&self.greeting);
        self.listeners.listen(listener, move |post| {
            Ok::<ChangeSubscription, String>(
                greeting.text.subscribe(move |text| post((**text).clone())),
            )
        })
    }
}

/// `builder.addGreeting()`.
pub struct GreetingBuilder;

#[class_extension(RayXAppBuilder)]
impl GreetingBuilder {
    /// Registers the tests' greeting service, saying "hello".
    pub fn add_greeting(
        this: Instance<RayXAppBuilder>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        RayXAppBuilder::extend_with(this, |mut builder| {
            builder
                .services_mut()
                .add_singleton_as::<Greeting>(Arc::new(Greeting {
                    text: StateCell::new("hello".to_owned()),
                }));
            builder
        })
    }
}

/// Adds the greeting's class and `builder.addGreeting()` to `set`.
pub fn register(set: &mut Registry) {
    set.service_class::<GreetingService>()
        .class_extension::<GreetingBuilder>();
}
