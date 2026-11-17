//! A navigation region for TSX ([`NavigationRegion`]), made with `navigationService.region(name)`.

use std::sync::Arc;

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx_runtime::services::navigation::{NavigationArgs, NavigationService, Route};

use super::listeners::Listeners;

/// Posts a region's pages to a listener.
type PostPages = dyn Fn(&NavigationService, &str) + Send + Sync;
use crate::region::pages;

/// A navigation region for TSX: `navigationService.region("panel")`, then `navigate`, `back`,
/// `forward`, `goTo(index)` and `close`; `pages()` is its journal up to the current entry (for
/// a `NavStack` of `NavPage`s, each `onDismiss` going to the page below), and `onChange` hears
/// every change of them.
#[class]
pub struct NavigationRegion {
    navigation: Arc<NavigationService>,
    name: String,
    listeners: Listeners,
}

fn json(value: &impl serde::Serialize) -> JsonValue {
    serde_json::to_value(value).unwrap_or(JsonValue::Null)
}

#[class]
impl NavigationRegion {
    /// Navigates the region to `uri`, as a new entry (`replace`: in place of the current one).
    pub fn navigate(&self, uri: String, replace: Option<bool>) -> Result<JsonValue, String> {
        let mut args = NavigationArgs::new(uri);
        if replace == Some(true) {
            args.history_mode = rayx_runtime::services::navigation::NavigationHistoryMode::Replace;
        }
        self.navigation
            .navigate_in_region(self.name.clone(), args)
            .map(|result| json(&result))
            .map_err(|error| error.to_string())
    }

    /// Goes back in the region's journal.
    pub fn back(&self) -> Result<JsonValue, String> {
        self.navigation
            .back_in_region(&self.name)
            .map(|result| json(&result))
            .map_err(|error| error.to_string())
    }

    /// Goes forward in the region's journal.
    pub fn forward(&self) -> Result<JsonValue, String> {
        self.navigation
            .forward_in_region(&self.name)
            .map(|result| json(&result))
            .map_err(|error| error.to_string())
    }

    /// Goes to the journal's entry `index` (a page's `onDismiss`: the page below it).
    pub fn go_to(&self, index: u32) -> Result<JsonValue, String> {
        self.navigation
            .go_to_entry(&self.name, index as usize)
            .map(|result| json(&result))
            .map_err(|error| error.to_string())
    }

    /// Closes the region (not the URL region); whether it was open.
    pub fn close(&self) -> Result<bool, String> {
        self.navigation
            .close_region(&self.name)
            .map_err(|error| error.to_string())
    }

    /// The region's journal up to the current entry, bottom to top: its routes.
    pub fn pages(&self) -> JsonValue {
        let routes: Vec<Route> = pages(&self.navigation, &self.name)
            .into_iter()
            .map(|(_, route)| route)
            .collect();
        json(&routes)
    }

    /// Calls `listener` with the pages after every change of them, until the function it
    /// returns is called (or the reactive owner of the call goes), or this object is disposed.
    pub fn on_change(
        &mut self,
        #[args(pages)] listener: Callback<JsonValue>,
    ) -> Result<Stop, String> {
        let (navigation, name) = (Arc::clone(&self.navigation), self.name.clone());
        self.listeners.listen(listener, move |post| {
            let post: Arc<PostPages> =
                Arc::new(move |navigation: &NavigationService, name: &str| {
                    let routes: Vec<Route> = pages(navigation, name)
                        .into_iter()
                        .map(|(_, route)| route)
                        .collect();
                    post(json(&routes));
                });
            let on_route = {
                let (source, name, post) =
                    (Arc::clone(&navigation), name.clone(), Arc::clone(&post));
                navigation
                    .subscribe_route_changed(move |event| {
                        if event.region == name {
                            post(&source, &name);
                        }
                    })
                    .map_err(|error| error.to_string())?
            };
            let on_close = {
                let source = Arc::clone(&navigation);
                navigation
                    .subscribe_region_closed(move |closed| {
                        if closed == name {
                            post(&source, &name);
                        }
                    })
                    .map_err(|error| error.to_string())?
            };
            Ok::<_, String>((on_route, on_close))
        })
    }
}

impl NavigationRegion {
    /// The region `name` of `navigation`, made when missing.
    pub(crate) fn of(navigation: Arc<NavigationService>, name: String) -> Result<Self, String> {
        navigation
            .get_or_create_region(name.as_str())
            .map_err(|error| error.to_string())?;
        Ok(NavigationRegion {
            navigation,
            name,
            listeners: Listeners::default(),
        })
    }
}
