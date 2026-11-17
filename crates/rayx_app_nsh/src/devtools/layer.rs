//! The in-app layer of the dev tools: one element the app mounts at its root ([`DevLayer`],
//! `<DevLayer />` in TSX), which in a dev build shows a small toolbar and, in pick mode, takes the
//! pointer before the app and outlines what it points at. In a build without the dev tools it
//! renders nothing.
//!
//! - **Toolbar.** Three buttons, reachable by touch: pick mode on or off (asked of the hub,
//!   `layer.pickModeRequested`; also `mod+shift+d`), the note box, and one step outward from
//!   the selection.
//! - **Pick mode** (the hub's `layer.pickMode`): a full-surface view over the app takes the
//!   pointer. Hovering outlines the innermost node under it; a press reports it to the hub
//!   (`layer.picked`), a press with a modifier (shift, alt, control or command) the next node
//!   outward from the selection under the pointer.
//! - **Outlines.** The hovered node and the selected one (the hub's `layer.select`), each with its
//!   margin and padding bands.
//! - **No state of its own.** Pick mode and the selection are the hub's; the layer only shows
//!   them and reports what the person does.
//!
//! The component's host is the layer's root: the agent leaves its whole subtree, the overlay
//! included, out of every answer ([`super::tree`]). The host itself takes no part in its parent's
//! layout: [`dev_layer`] makes it absolute, and so does TSX's `style={{ position: "absolute" }}`
//! (a component cannot style its own host). **Stacking:** the layer is an overlay; a modal opened
//! after it sits above it on some platforms, where it cannot outline the modal's content.

use nsh_core::prelude::*;

pub use component::DevLayer;

mod component {
    use nsh_core::prelude::*;

    /// The dev tools' layer: mount it once at the app's root, out of its parent's layout
    /// (`<DevLayer style={{ position: "absolute" }} />`). In a build without the dev tools it
    /// renders nothing.
    #[component]
    pub fn DevLayer() -> impl IntoElement {
        #[cfg(rayx_devtools)]
        {
            super::shown::layer().into_element()
        }
        #[cfg(not(rayx_devtools))]
        {
            when(|| false, view).into_element()
        }
    }
}

/// The dev tools' layer for a Rust root (`.child(dev_layer())`): [`DevLayer`], absolute, so it
/// takes no place in its parent's layout. In a build without the dev tools it renders nothing.
pub fn dev_layer() -> impl IntoElement {
    component::dev_layer().absolute()
}

#[cfg(rayx_devtools)]
mod shown {
    use std::rc::Rc;

    use nsh_core::element::{AdaptiveColor, input};
    use nsh_core::prelude::*;
    use rayx_devtools_protocol::{self as proto, methods};

    use super::super::agent::{self, Agent};
    use super::super::tree::{self, LAYER_TEST_ID, Outline};

    /// The test id of the pick mode's surface.
    pub(super) const PICK_TEST_ID: &str = "rayx-devtools-pick";

    const OUTLINE: u32 = 0x3b82f6;
    const SELECTED: u32 = 0xf97316;
    const MARGIN: u32 = 0xf59e0b;
    const PADDING: u32 = 0x22c55e;

    pub(super) fn layer() -> impl IntoElement {
        dynamic(move || match agent::tracked() {
            Some(agent) => with_agent(agent).into_element(),
            None => when(|| false, view).into_element(),
        })
    }

    fn with_agent(agent: Rc<Agent>) -> impl IntoElement {
        let pick_mode = agent.pick_mode;
        let selection = agent.selection;
        let hovered: Signal<Option<u64>> = signal(None);
        let hovered_box: Signal<Option<Outline>> = signal(None);
        let selected_box: Signal<Option<Outline>> = signal(None);
        let note_open = signal(false);
        let note = signal(String::new());

        // The selected node's box, again whenever the hub selects another.
        {
            let agent = Rc::clone(&agent);
            effect(move || {
                let id = selection.get();
                let agent = Rc::clone(&agent);
                drop(spawn(async move {
                    let outline = match id {
                        Some(id) => tree::outline(&agent, id).await,
                        None => None,
                    };
                    selected_box.set(outline);
                }));
            });
        }
        // Leaving pick mode forgets the hovered node.
        effect(move || {
            if !pick_mode.get() {
                hovered.set(None);
                hovered_box.set(None);
            }
        });
        // The dev shortcut: ask for pick mode on or off.
        {
            let agent = Rc::clone(&agent);
            on_key("mod+shift+d", move |_| toggle(&agent));
        }

        let hover = {
            let agent = Rc::clone(&agent);
            move |x: f32, y: f32| {
                let agent = Rc::clone(&agent);
                drop(spawn(async move {
                    let innermost = tree::pick_at(&agent, x, y).await.first().copied();
                    if hovered.get_untracked() != innermost {
                        hovered.set(innermost);
                        let outline = match innermost {
                            Some(id) => tree::outline(&agent, id).await,
                            None => None,
                        };
                        hovered_box.set(outline);
                    }
                }));
            }
        };
        let press = {
            let agent = Rc::clone(&agent);
            move |x: f32, y: f32, outward: bool| {
                let agent = Rc::clone(&agent);
                drop(spawn(async move {
                    let picked = tree::pick_at(&agent, x, y).await;
                    let current = selection.get_untracked();
                    let node = if outward {
                        // The next node outward from the selection, when it is under the pointer.
                        current
                            .and_then(|current| picked.iter().position(|id| *id == current))
                            .and_then(|at| picked.get(at + 1))
                            .or(picked.first())
                    } else {
                        picked.first()
                    };
                    if let Some(&node) = node {
                        report(&agent, node);
                    }
                }));
            }
        };

        let surface = move || {
            let hover = hover.clone();
            let press = press.clone();
            view()
                .absolute()
                .inset_0()
                .test_id(PICK_TEST_ID)
                .on_mouse_move(move |event: &MouseEvent| {
                    hover(
                        f32::from(event.page_position.x),
                        f32::from(event.page_position.y),
                    );
                })
                .on_press(move |event: &PressEvent| {
                    let held = event.modifiers;
                    let outward = held.shift || held.alt || held.ctrl || held.cmd;
                    press(
                        f32::from(event.page_position.x),
                        f32::from(event.page_position.y),
                        outward,
                    );
                })
        };

        let toolbar = {
            let (pick, out, send) = (Rc::clone(&agent), Rc::clone(&agent), Rc::clone(&agent));
            view()
                .absolute()
                .right(px(12.0))
                .bottom(px(12.0))
                .flex_row()
                .gap_2()
                .p_1()
                .rounded_md()
                .bg(AdaptiveColor::rgb(0x111827, 0x111827))
                .child(button("Pick", move || toggle(&pick)))
                .child(button("Note", move || {
                    note_open.set(!note_open.get_untracked())
                }))
                .child(button("Out", move || step_outward(&out)))
                .child(when(note_open, move || {
                    let send = Rc::clone(&send);
                    view()
                        .flex_row()
                        .gap_2()
                        .child(
                            input()
                                .value(move || note.get())
                                .on_change(move |text: &str| note.set(text.to_owned()))
                                .placeholder("A note"),
                        )
                        .child(button("Send", move || {
                            send.notify(
                                methods::LAYER_NOTE,
                                &proto::Note {
                                    node: selection.get_untracked(),
                                    text: note.get_untracked(),
                                },
                            );
                            note.set(String::new());
                            note_open.set(false);
                        }))
                }))
        };

        overlay().absolute().inset_0().child(
            view()
                .absolute()
                .inset_0()
                .test_id(LAYER_TEST_ID)
                .pointer_events(PointerEvents::BoxNone)
                .child(when(pick_mode, surface))
                .child(outlines(hovered_box, OUTLINE))
                .child(outlines(selected_box, SELECTED))
                .child(toolbar),
        )
    }

    /// Asks the hub for pick mode on or off.
    fn toggle(agent: &Agent) {
        agent.notify(
            methods::LAYER_PICK_MODE_REQUESTED,
            &proto::PickMode {
                on: !agent.pick_mode.get_untracked(),
            },
        );
    }

    /// Reports the selection's parent as picked: one step outward.
    fn step_outward(agent: &Agent) {
        if let Some(parent) = agent
            .selection
            .get_untracked()
            .and_then(|selected| agent.parent_of(selected))
        {
            report(agent, parent);
        }
    }

    fn report(agent: &Agent, node: u64) {
        agent.notify(
            methods::LAYER_PICKED,
            &proto::Picked {
                node,
                revision: agent.revision(),
            },
        );
    }

    fn button(label: &'static str, press: impl Fn() + 'static) -> impl IntoElement {
        view()
            .px_2()
            .py_1()
            .rounded_md()
            .on_press(move |_: &PressEvent| press())
            .child(
                text(label)
                    .text_sm()
                    .text_color(AdaptiveColor::rgb(0xffffff, 0xffffff)),
            )
    }

    /// A node's outline in `color`, with its margin and padding bands.
    fn outlines(outline: Signal<Option<Outline>>, color: u32) -> impl IntoElement {
        dynamic(move || match outline.get() {
            None => when(|| false, view).into_element(),
            Some(outline) => {
                let frame = outline.frame;
                let band = |rect: proto::Rect, color: u32, opacity: f32| {
                    view()
                        .absolute()
                        .left(px(rect.x))
                        .top(px(rect.y))
                        .w(px(rect.width.max(0.0)))
                        .h(px(rect.height.max(0.0)))
                        .bg(AdaptiveColor::rgb(color, color))
                        .opacity(opacity)
                        .pointer_events(PointerEvents::None)
                };
                let margin = proto::Rect {
                    x: frame.x - outline.margin.left,
                    y: frame.y - outline.margin.top,
                    width: frame.width + outline.margin.left + outline.margin.right,
                    height: frame.height + outline.margin.top + outline.margin.bottom,
                };
                let content = proto::Rect {
                    x: frame.x + outline.padding.left,
                    y: frame.y + outline.padding.top,
                    width: frame.width - outline.padding.left - outline.padding.right,
                    height: frame.height - outline.padding.top - outline.padding.bottom,
                };
                view()
                    .absolute()
                    .inset_0()
                    .pointer_events(PointerEvents::None)
                    .child(band(margin, MARGIN, 0.25))
                    .child(band(frame, PADDING, 0.25))
                    .child(band(content, color, 0.25))
                    .child(
                        view()
                            .absolute()
                            .left(px(frame.x))
                            .top(px(frame.y))
                            .w(px(frame.width.max(0.0)))
                            .h(px(frame.height.max(0.0)))
                            .border_2()
                            .border_color(AdaptiveColor::rgb(color, color))
                            .pointer_events(PointerEvents::None),
                    )
                    .into_element()
            }
        })
    }
}
