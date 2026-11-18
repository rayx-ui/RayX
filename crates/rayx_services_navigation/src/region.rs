//! Navigation regions on NSH: a region of the app's `dyn NavigationService` as an NSH
//! navigation stack ([`navigation_region`]), whose pages are the region's journal up to its
//! current entry, or as the current route alone ([`navigation_view`]); TSX has the same as
//! `NavigationRegion` (`navigationService.region(name)`, [`crate::NavigationRegion`]).
//!
//! Pushing, replacing, resetting, going back and forward are the service's (its journal); the
//! stack follows it. A page the user pops natively (a swipe back, the system back button) moves
//! the journal back to the page below.

use std::cell::Cell;
use std::sync::Arc;

use futures::FutureExt as _;
use futures::StreamExt as _;
use futures::channel::mpsc::unbounded;
use nsh_core::element::{Screen, Stack, stack};
use nsh_core::prelude::*;
use rayx_app::ChangeSubscription;
use rayx_app::services::navigation::{NavigationEvent, NavigationService, Route};
use rayx_app_nsh::{AppError, services};

/// The pages of `region`: its journal up to the current entry, with their indexes.
pub(crate) fn pages(navigation: &dyn NavigationService, region: &str) -> Vec<(usize, Route)> {
    navigation.navigation().get().pages(region)
}

/// Whether `event` changes `region`'s pages: a route change in it, or its closing.
pub(crate) fn changes_pages(event: &NavigationEvent, region: &str) -> bool {
    match event {
        NavigationEvent::RouteChanged(changed) => changed.region == region,
        NavigationEvent::RegionClosed { region: closed } => closed == region,
        _ => false,
    }
}

/// Makes `region` when it does not exist. Polled once: RayX's implementation answers at once;
/// a TS one's call starts, and the region is made in its turn.
pub(crate) fn ensure_region(
    navigation: &dyn NavigationService,
    region: &str,
) -> Result<(), String> {
    match navigation.ensure_region(region.to_owned()).now_or_never() {
        Some(made) => made.map_err(|error| error.to_string()),
        None => Ok(()),
    }
}

/// `region`'s pages as an NSH signal, following the service until the component's cleanup.
fn use_pages(
    navigation: &Arc<dyn NavigationService>,
    region: &str,
) -> Result<ReadSignal<Vec<(usize, Route)>>, AppError> {
    ensure_region(navigation.as_ref(), region).map_err(AppError::Failed)?;
    let target = signal(pages(navigation.as_ref(), region));
    let (send, mut changed) = unbounded::<()>();
    let name = region.to_owned();
    let keep: ChangeSubscription = navigation.events().subscribe(move |event| {
        if changes_pages(event, &name) {
            let _ = send.unbounded_send(());
        }
    });
    let kept = Cell::new(Some(keep));
    cleanup(move || drop(kept.take()));
    let (source, name) = (Arc::clone(navigation), region.to_owned());
    drop(spawn(async move {
        while changed.next().await.is_some() {
            target.set(pages(source.as_ref(), &name));
        }
    }));
    Ok(target.read_only())
}

/// The navigation region `name` (made when missing) as an NSH navigation stack: one page per
/// journal entry up to the current one, each `render`ed from its route (`nsh_base`'s
/// `nav_page()` with what the route shows). Navigating the region pushes, replaces or pops pages;
/// a page the user pops natively takes the journal back to the page below. Call it in a
/// component's body, under the app's services (its `dyn NavigationService`).
pub fn navigation_region(
    name: &str,
    render: impl Fn(&Route) -> Screen + 'static,
) -> Result<Stack, AppError> {
    let navigation = services()?.required_service::<dyn NavigationService>()?;
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
                    // Polled once, as `ensure_region` is.
                    let _ = navigation.go_to_entry(region.clone(), below).now_or_never();
                }
            })
        },
    )))
}

/// The navigation region `name` (made when missing) as its current route alone, `render`ed
/// again for every navigation: a region with no stack (tabs, a side panel). Call it in a
/// component's body, under the app's services (its `dyn NavigationService`).
pub fn navigation_view<E: IntoElement + 'static>(
    name: &str,
    render: impl Fn(Option<&Route>) -> E + 'static,
) -> Result<AnyElement, AppError> {
    let navigation = services()?.required_service::<dyn NavigationService>()?;
    let pages = use_pages(&navigation, name)?;
    Ok(dynamic(move || {
        let current = pages.with(|pages| pages.last().map(|(_, route)| route.clone()));
        render(current.as_ref()).into_element()
    }))
}
