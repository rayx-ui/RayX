//! Navigation regions on NSH: a region of the navigation service as an NSH navigation stack
//! ([`navigation_region`]), whose pages are the region's journal up to its current entry, or as
//! the current route alone ([`navigation_view`]); and [`NavigationRegion`] for TSX.
//!
//! Pushing, replacing, resetting, going back and forward are the service's (its journal); the
//! stack follows it. A page the user pops natively (a swipe back, the system back button) moves
//! the journal back to the page below.

use std::cell::Cell;

use futures::StreamExt as _;
use futures::channel::mpsc::unbounded;
use nsh_core::JsonValue;
use nsh_core::element::{Screen, Stack, stack};
use nsh_core::prelude::*;
use rayx_runtime::services::navigation::{
    NavigationArgs, NavigationService, NavigationSubscription, Route,
};
use std::sync::Arc;

use crate::{AppError, AppServices, services};

/// The pages of `region`: its journal up to the current entry, with their indexes.
fn pages(navigation: &NavigationService, region: &str) -> Vec<(usize, Route)> {
    let Ok(Some(journal)) = navigation.journal(region) else {
        return Vec::new();
    };
    let Some(current) = journal.current_index() else {
        return Vec::new();
    };
    journal
        .iter()
        .take(current + 1)
        .cloned()
        .enumerate()
        .collect()
}

/// `region`'s pages as an NSH signal, following the service until the component's cleanup.
fn use_pages(
    navigation: &Arc<NavigationService>,
    region: &str,
) -> Result<ReadSignal<Vec<(usize, Route)>>, AppError> {
    navigation.get_or_create_region(region).map_err(failed)?;
    let target = signal(pages(navigation, region));
    let (send, mut changed) = unbounded::<()>();
    let keep: Vec<NavigationSubscription> = {
        let (on_route, on_close) = (send.clone(), send);
        let name = region.to_owned();
        vec![
            navigation
                .subscribe_route_changed(move |event| {
                    if event.region == name {
                        let _ = on_route.unbounded_send(());
                    }
                })
                .map_err(failed)?,
            navigation
                .subscribe_region_closed(move |_| {
                    let _ = on_close.unbounded_send(());
                })
                .map_err(failed)?,
        ]
    };
    let kept = Cell::new(Some(keep));
    cleanup(move || drop(kept.take()));
    let (source, name) = (Arc::clone(navigation), region.to_owned());
    drop(spawn(async move {
        while changed.next().await.is_some() {
            target.set(pages(&source, &name));
        }
    }));
    Ok(target.read_only())
}

fn failed(error: impl std::fmt::Display) -> AppError {
    AppError::Failed(error.to_string())
}

/// The navigation region `name` (made when missing) as an NSH navigation stack: one page per
/// journal entry up to the current one, each `render`ed from its route (`nsh_base`'s
/// `nav_page()` with what the route shows). Navigating the region pushes, replaces or pops pages;
/// a page the user pops natively takes the journal back to the page below. Call it in a
/// component's body, under the app's services.
pub fn navigation_region(
    name: &str,
    render: impl Fn(&Route) -> Screen + 'static,
) -> Result<Stack, AppError> {
    let navigation = services()?.required_service::<NavigationService>()?;
    let pages = use_pages(&navigation, name)?;
    let region = name.to_owned();
    let render = std::rc::Rc::new(render);
    Ok(stack().child(each(
        move || pages.get(),
        |(index, route): &(usize, Route)| format!("{index} {}", route.location),
        move |page, _depth| {
            let (index, route) = page.get_untracked();
            let (navigation, region) = (Arc::clone(&navigation), region.clone());
            render(&route).on_dismiss(move || {
                if let Some(below) = index.checked_sub(1) {
                    let _ = navigation.go_to_entry(&region, below);
                }
            })
        },
    )))
}

/// The navigation region `name` (made when missing) as its current route alone, `render`ed
/// again for every navigation: a region with no stack (tabs, a side panel). Call it in a
/// component's body, under the app's services.
pub fn navigation_view<E: IntoElement + 'static>(
    name: &str,
    render: impl Fn(Option<&Route>) -> E + 'static,
) -> Result<AnyElement, AppError> {
    let navigation = services()?.required_service::<NavigationService>()?;
    let pages = use_pages(&navigation, name)?;
    Ok(dynamic(move || {
        let current = pages.with(|pages| pages.last().map(|(_, route)| route.clone()));
        render(current.as_ref()).into_element()
    }))
}

/// A navigation region for TSX: `NavigationRegion.of(app, "panel")`, then `navigate`, `back`,
/// `forward`, `goTo(index)` and `close`; `pages()` is its journal up to the current entry (for
/// a `NavStack` of `NavPage`s, each `onDismiss` going to the page below), and `onChange` hears
/// every change of them.
#[class]
pub struct NavigationRegion {
    navigation: Arc<NavigationService>,
    name: String,
    listeners: Vec<(u64, NavigationSubscription, NavigationSubscription)>,
}

thread_local! {
    static LISTENERS: std::cell::RefCell<std::collections::HashMap<u64, Callback<JsonValue>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
    static NEXT_ID: Cell<u64> = const { Cell::new(0) };
}

fn json(value: &impl serde::Serialize) -> JsonValue {
    serde_json::to_value(value).unwrap_or(JsonValue::Null)
}

#[class]
impl NavigationRegion {
    /// The region `name` of the app's navigation, made when missing.
    pub fn of(
        app: Instance<AppServices>,
        name: String,
    ) -> Result<Instance<NavigationRegion>, AppError> {
        let navigation = app.with(|app| app.services().required_service::<NavigationService>())?;
        navigation
            .get_or_create_region(name.as_str())
            .map_err(failed)?;
        Ok(Instance::new(NavigationRegion {
            navigation,
            name,
            listeners: Vec::new(),
        }))
    }

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

    /// Calls `listener` with the pages after every change of them, until this object is
    /// disposed.
    pub fn on_change(
        &mut self,
        #[args(pages)] listener: Callback<JsonValue>,
    ) -> Result<(), String> {
        let id = NEXT_ID.with(|next| {
            let id = next.get();
            next.set(id + 1);
            id
        });
        LISTENERS.with_borrow_mut(|listeners| listeners.insert(id, listener));
        let post = move |navigation: &NavigationService, name: &str| {
            let routes: Vec<Route> = pages(navigation, name)
                .into_iter()
                .map(|(_, route)| route)
                .collect();
            let value = json(&routes);
            LISTENERS.with_borrow(|listeners| {
                if let Some(listener) = listeners.get(&id) {
                    listener.post(value);
                }
            });
        };
        let post = Arc::new(post);
        let (navigation, name) = (Arc::clone(&self.navigation), self.name.clone());
        let on_route = {
            let (navigation, name, post) =
                (Arc::clone(&navigation), name.clone(), Arc::clone(&post));
            self.navigation
                .subscribe_route_changed(move |event| {
                    if event.region == name {
                        post(&navigation, &name);
                    }
                })
                .map_err(|error| error.to_string())?
        };
        let on_close = {
            self.navigation
                .subscribe_region_closed(move |closed| {
                    if closed == name {
                        post(&navigation, &name);
                    }
                })
                .map_err(|error| error.to_string())?
        };
        self.listeners.push((id, on_route, on_close));
        Ok(())
    }
}

impl Drop for NavigationRegion {
    fn drop(&mut self) {
        LISTENERS.with_borrow_mut(|listeners| {
            for (id, _, _) in &self.listeners {
                listeners.remove(id);
            }
        });
    }
}
