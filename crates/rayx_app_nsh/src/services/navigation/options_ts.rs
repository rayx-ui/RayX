//! The navigation service's options for TSX (`addNavigation`).

use nsh_core::prelude::*;
use rayx_runtime::services::navigation::{NavigationOptions as Navigation, RouteDefinition};

use crate::configuration::options_ts::push;

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
