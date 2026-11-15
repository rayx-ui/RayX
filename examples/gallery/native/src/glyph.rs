//! `Glyph`: a few Lucide icons (https://lucide.dev, ISC License, Copyright (c) Lucide
//! Contributors) for the gallery's examples, stroked in `currentColor`: the text color around them,
//! so in a Button's icon slot they take its label's color, hover and disabled included.

use nsh_core::element::ImageSrc;
use nsh_core::prelude::*;

/// A [`Glyph`]'s icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PropEnum)]
pub enum GlyphName {
    /// A plus: add, new.
    #[default]
    Plus,
    /// A new file.
    FilePlus,
    /// A download arrow into a tray.
    Download,
    /// A chevron pointing right: next, more.
    ChevronRight,
    /// A check mark: done, confirm.
    Check,
    /// An arrow curving back: undo.
    Undo,
    /// An arrow curving forward: redo.
    Redo,
    /// An inbox tray.
    Inbox,
    /// A gear: settings.
    Settings,
    /// A magnifier: search.
    Search,
    /// A paper plane: send.
    Send,
    /// A cross: close, remove.
    X,
}

/// The props of [`Glyph`].
#[derive(Debug, Clone, PartialEq, Props)]
pub struct GlyphProps {
    /// Which icon.
    #[prop(default)]
    pub name: Prop<GlyphName>,
    /// Its edge in points. Left out, it fills the box it is in: a Button's icon box, which is
    /// `lvl5/adornmentSize` square.
    pub size: Prop<Option<f32>>,
}

/// A Lucide icon on its 24 × 24 grid, stroked 2 units wide in the color of the text around it.
/// Decorative: the control it is in names the action.
#[component]
pub fn Glyph(props: GlyphProps) -> impl IntoElement {
    let edge = move || match props.size.get() {
        Some(size) => Length::from(px(size)),
        None => Length::from(relative(1.0)),
    };
    svg(move || ImageSrc::svg_markup(&markup(props.name.get())))
        .w(edge)
        .h(edge)
        .flex_shrink_0()
}

/// The SVG document of `name`.
fn markup(name: GlyphName) -> String {
    format!(
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" "#,
            r#"fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" "#,
            r#"stroke-linejoin="round">{}</svg>"#
        ),
        shapes(name)
    )
}

/// The shapes of `name`, from lucide-static 1.61.0.
fn shapes(name: GlyphName) -> &'static str {
    match name {
        GlyphName::Plus => r#"<path d="M5 12h14"/><path d="M12 5v14"/>"#,
        GlyphName::FilePlus => concat!(
            r#"<path d="M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z"/>"#,
            r#"<path d="M14 2v5a1 1 0 0 0 1 1h5"/><path d="M9 15h6"/><path d="M12 18v-6"/>"#
        ),
        GlyphName::Download => concat!(
            r#"<path d="M12 15V3"/><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>"#,
            r#"<path d="m7 10 5 5 5-5"/>"#
        ),
        GlyphName::ChevronRight => r#"<path d="m9 18 6-6-6-6"/>"#,
        GlyphName::Check => r#"<path d="M20 6 9 17l-5-5"/>"#,
        GlyphName::Undo => concat!(
            r#"<path d="M9 14 4 9l5-5"/>"#,
            r#"<path d="M4 9h10.5a5.5 5.5 0 0 1 5.5 5.5a5.5 5.5 0 0 1-5.5 5.5H11"/>"#
        ),
        GlyphName::Redo => concat!(
            r#"<path d="m15 14 5-5-5-5"/>"#,
            r#"<path d="M20 9H9.5A5.5 5.5 0 0 0 4 14.5A5.5 5.5 0 0 0 9.5 20H13"/>"#
        ),
        GlyphName::Inbox => concat!(
            r#"<polyline points="22 12 16 12 14 15 10 15 8 12 2 12"/>"#,
            r#"<path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z"/>"#
        ),
        GlyphName::Settings => concat!(
            r#"<path d="M9.671 4.136a2.34 2.34 0 0 1 4.659 0 2.34 2.34 0 0 0 3.319 1.915 2.34 2.34 0 0 1 2.33 4.033 2.34 2.34 0 0 0 0 3.831 2.34 2.34 0 0 1-2.33 4.033 2.34 2.34 0 0 0-3.319 1.915 2.34 2.34 0 0 1-4.659 0 2.34 2.34 0 0 0-3.32-1.915 2.34 2.34 0 0 1-2.33-4.033 2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915"/>"#,
            r#"<circle cx="12" cy="12" r="3"/>"#
        ),
        GlyphName::Search => r#"<path d="m21 21-4.34-4.34"/><circle cx="11" cy="11" r="8"/>"#,
        GlyphName::Send => concat!(
            r#"<path d="M14.536 21.686a.5.5 0 0 0 .937-.024l6.5-19a.496.496 0 0 0-.635-.635l-19 6.5a.5.5 0 0 0-.024.937l7.93 3.18a2 2 0 0 1 1.112 1.11z"/>"#,
            r#"<path d="m21.854 2.147-10.94 10.939"/>"#
        ),
        GlyphName::X => r#"<path d="M18 6 6 18"/><path d="m6 6 12 12"/>"#,
    }
}
