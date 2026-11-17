//! A navigation region for TSX ([`NavigationRegion`]), made with `navigationService.region(name)`.

use std::sync::Arc;

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx_app::services::navigation::{
    NavigationArgs, NavigationHistoryMode, NavigationService, Route,
};
use rayx_app_nsh::ts::{Listeners, json};

use crate::navigation_ts::now;
use crate::region::{changes_pages, ensure_region, pages};

/// A navigation region for TSX: `navigationService.region("panel")`, then `navigate`, `back`,
/// `forward`, `goTo(index)` and `close`; `pages()` is its journal up to the current entry (for
/// a `NavStack` of `NavPage`s, each `onDismiss` going to the page below), and `onChange` hears
/// every change of them.
#[class]
pub struct NavigationRegion {
    navigation: Arc<dyn NavigationService>,
    name: String,
    listeners: Listeners,
}

/// The routes of `region`'s pages, bottom to top, for TS.
fn page_routes(navigation: &dyn NavigationService, region: &str) -> JsonValue {
    let routes: Vec<Route> = pages(navigation, region)
        .into_iter()
        .map(|(_, route)| route)
        .collect();
    json(&routes)
}

#[class]
impl NavigationRegion {
    /// Navigates the region to `uri`, as a new entry (`replace`: in place of the current one).
    pub fn navigate(&self, uri: String, replace: Option<bool>) -> Result<JsonValue, String> {
        let mut args = NavigationArgs::new(uri);
        if replace == Some(true) {
            args.history_mode = NavigationHistoryMode::Replace;
        }
        now(self.navigation.navigate_in_region(self.name.clone(), args))
    }

    /// Goes back in the region's journal.
    pub fn back(&self) -> Result<JsonValue, String> {
        now(self.navigation.back(self.name.clone()))
    }

    /// Goes forward in the region's journal.
    pub fn forward(&self) -> Result<JsonValue, String> {
        now(self.navigation.forward(self.name.clone()))
    }

    /// Goes to the journal's entry `index` (a page's `onDismiss`: the page below it).
    pub fn go_to(&self, index: u32) -> Result<JsonValue, String> {
        now(self
            .navigation
            .go_to_entry(self.name.clone(), index as usize))
    }

    /// Closes the region (not the URL region); whether it was open.
    pub fn close(&self) -> Result<bool, String> {
        now(self.navigation.close_region(self.name.clone()))
            .map(|closed| closed.as_bool() == Some(true))
    }

    /// The region's journal up to the current entry, bottom to top: its routes.
    pub fn pages(&self) -> JsonValue {
        page_routes(self.navigation.as_ref(), &self.name)
    }

    /// Calls `listener` with the pages after every change of them, until the function it
    /// returns is called (or the reactive owner of the call goes), or this object is disposed.
    pub fn on_change(
        &mut self,
        #[args(pages)] listener: Callback<JsonValue>,
    ) -> Result<Stop, String> {
        let (navigation, name) = (Arc::clone(&self.navigation), self.name.clone());
        self.listeners.listen(listener, move |post| {
            let source = Arc::clone(&navigation);
            Ok::<_, String>(navigation.events().subscribe(move |event| {
                if changes_pages(event, &name) {
                    post(page_routes(source.as_ref(), &name));
                }
            }))
        })
    }
}

impl NavigationRegion {
    /// The region `name` of `navigation`, made when missing.
    pub(crate) fn of(navigation: Arc<dyn NavigationService>, name: String) -> Result<Self, String> {
        ensure_region(navigation.as_ref(), &name)?;
        Ok(NavigationRegion {
            navigation,
            name,
            listeners: Listeners::default(),
        })
    }
}
