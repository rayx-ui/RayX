//! Navigation regions on NSH: a region of the navigation service as an NSH navigation stack
//! ([`navigation_region`]), whose pages are the region's journal up to its current entry, or as
//! the current route alone ([`navigation_view`]); TSX has the same as `NavigationRegion`
//! (`navigationService.region(name)`, [`crate::ts::NavigationRegion`]).
//!
//! Pushing, replacing, resetting, going back and forward are the service's (its journal); the
//! stack follows it. A page the user pops natively (a swipe back, the system back button) moves
//! the journal back to the page below.

use std::cell::Cell;

use futures::StreamExt as _;
use futures::channel::mpsc::unbounded;
use nsh_core::element::{Screen, Stack, stack};
use nsh_core::prelude::*;
use rayx_runtime::services::navigation::{NavigationService, NavigationSubscription, Route};
use std::sync::Arc;

use crate::{AppError, services};

/// The pages of `region`: its journal up to the current entry, with their indexes.
pub(crate) fn pages(navigation: &NavigationService, region: &str) -> Vec<(usize, Route)> {
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
