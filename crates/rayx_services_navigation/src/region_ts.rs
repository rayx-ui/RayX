//! A navigation region for TSX ([`NavigationRegion`]), made with `navigationService.region(name)`.

use std::cell::RefCell;
use std::sync::Arc;

use futures::FutureExt as _;
use futures::future::{LocalBoxFuture, Shared};

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
    /// Whether the region is made: a TS implementation answers later, so every command waits
    /// for the one call making it, and rejects with why it could not.
    made: RefCell<Made>,
}

/// The service's call making a region, shared by the commands waiting for it.
type Making = Shared<LocalBoxFuture<'static, Result<(), String>>>;

/// Whether a [`NavigationRegion`]'s region is made.
enum Made {
    Yes,
    /// The call making it runs.
    Making(Making),
    /// Its last call failed: the next command makes it again.
    No,
}

/// `navigation`'s call making the region `name`, started.
fn making(navigation: &Arc<dyn NavigationService>, name: &str) -> Making {
    let (navigation, name) = (Arc::clone(navigation), name.to_owned());
    async move {
        navigation
            .ensure_region(name)
            .await
            .map_err(|error| error.to_string())
    }
    .boxed_local()
    .shared()
}

/// What a command of `this` runs on: the service and the region's name, the region made (every
/// command waits for the one call making it; after a failed one, the next command calls again).
/// A region disposed while its command waits rejects that command.
async fn region(
    this: &Instance<NavigationRegion>,
) -> Result<(Arc<dyn NavigationService>, String), String> {
    let (navigation, name, waiting) = this.with(|this| {
        let mut made = this.made.borrow_mut();
        let waiting = match &*made {
            Made::Yes => None,
            Made::Making(call) => Some(call.clone()),
            Made::No => {
                let call = making(&this.navigation, &this.name);
                *made = Made::Making(call.clone());
                Some(call)
            }
        };
        (Arc::clone(&this.navigation), this.name.clone(), waiting)
    });
    if let Some(call) = waiting {
        let answer = call.clone().await;
        if this.is_disposed() {
            return Err("the region was disposed while the command waited".to_owned());
        }
        this.with(|this| {
            let mut made = this.made.borrow_mut();
            // Only the answer of the current call counts (one failed, the next may run).
            if matches!(&*made, Made::Making(current) if current.ptr_eq(&call)) {
                *made = if answer.is_ok() { Made::Yes } else { Made::No };
            }
        });
        answer?;
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
    /// (its error is this call's); a TS one's call starts, kept, and the region's commands wait
    /// for it.
    pub(crate) fn of(navigation: Arc<dyn NavigationService>, name: String) -> Result<Self, String> {
        let call = making(&navigation, &name);
        let made = match call.clone().now_or_never() {
            Some(made) => {
                made?;
                Made::Yes
            }
            None => Made::Making(call),
        };
        Ok(NavigationRegion {
            navigation,
            name,
            listeners: Listeners::default(),
            made: RefCell::new(made),
        })
    }
}
