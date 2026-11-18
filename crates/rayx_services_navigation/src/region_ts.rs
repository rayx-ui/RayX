//! A navigation region for TSX ([`NavigationRegion`]), made with `navigationService.region(name)`.

use std::cell::Cell;
use std::sync::Arc;

use futures::FutureExt as _;

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx_app::services::navigation::{
    NavigationArgs, NavigationHistoryMode, NavigationService, Route,
};
use rayx_app_nsh::ts::{Listeners, json};

use crate::navigation_ts::answer;
use crate::region::{changes_pages, pages};

/// A navigation region for TSX: `navigationService.region("panel")`, then `navigate`, `back`,
/// `forward`, `refresh`, `goTo(index)` and `close`, each a promise; `pages()` is its journal up
/// to the current entry (for a `NavStack` of `NavPage`s, each `onDismiss` going to the page
/// below), and `onChange` hears every change of them.
#[class]
pub struct NavigationRegion {
    navigation: Arc<dyn NavigationService>,
    name: String,
    listeners: Listeners,
    /// The region was not made yet when this object was (a TS implementation answers later):
    /// the first command makes it first, and rejects with why it could not.
    unmade: Cell<bool>,
}

/// What a command of `this` runs on: the service and the region's name, the region made.
async fn region(
    this: &Instance<NavigationRegion>,
) -> Result<(Arc<dyn NavigationService>, String), String> {
    let (navigation, name, unmade) = this.with(|this| {
        (
            Arc::clone(&this.navigation),
            this.name.clone(),
            this.unmade.replace(false),
        )
    });
    if unmade {
        navigation
            .ensure_region(name.clone())
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok((navigation, name))
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
    /// Navigates the region to `uri`, as a new entry (`replace`: in place of the current one);
    /// resolves with the `NavigationResult`, or rejects with the service's error.
    #[ts("(uri: string, replace?: boolean | null): NavigationResult")]
    pub async fn navigate(
        this: Instance<Self>,
        uri: String,
        replace: Option<bool>,
    ) -> Result<JsonValue, String> {
        let (navigation, name) = region(&this).await?;
        let mut args = NavigationArgs::new(uri);
        if replace == Some(true) {
            args.history_mode = NavigationHistoryMode::Replace;
        }
        answer(navigation.navigate_in_region(name, args)).await
    }

    /// Goes back in the region's journal.
    #[ts("(): NavigationResult")]
    pub async fn back(this: Instance<Self>) -> Result<JsonValue, String> {
        let (navigation, name) = region(&this).await?;
        answer(navigation.back(name)).await
    }

    /// Goes forward in the region's journal.
    #[ts("(): NavigationResult")]
    pub async fn forward(this: Instance<Self>) -> Result<JsonValue, String> {
        let (navigation, name) = region(&this).await?;
        answer(navigation.forward(name)).await
    }

    /// Navigates the region to its current route again.
    #[ts("(): NavigationResult")]
    pub async fn refresh(this: Instance<Self>) -> Result<JsonValue, String> {
        let (navigation, name) = region(&this).await?;
        answer(navigation.refresh(name)).await
    }

    /// Goes to the journal's entry `index` (a page's `onDismiss`: the page below it).
    #[ts("(index: number): NavigationResult")]
    pub async fn go_to(this: Instance<Self>, index: u32) -> Result<JsonValue, String> {
        let (navigation, name) = region(&this).await?;
        answer(navigation.go_to_entry(name, index as usize)).await
    }

    /// Closes the region (not the URL region); resolves with whether it was open.
    pub async fn close(this: Instance<Self>) -> Result<bool, String> {
        let (navigation, name) = region(&this).await?;
        navigation
            .close_region(name)
            .await
            .map_err(|error| error.to_string())
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
    /// The region `name` of `navigation`, made when missing: at once by RayX's implementation
    /// (its error is this call's); a TS one's call starts, and the region's first command waits
    /// for it.
    pub(crate) fn of(navigation: Arc<dyn NavigationService>, name: String) -> Result<Self, String> {
        let unmade = match navigation.ensure_region(name.clone()).now_or_never() {
            Some(made) => {
                made.map_err(|error| error.to_string())?;
                false
            }
            None => true,
        };
        Ok(NavigationRegion {
            navigation,
            name,
            listeners: Listeners::default(),
            unmade: Cell::new(unmade),
        })
    }
}
