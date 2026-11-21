//! `AutoLayout`: RayX's container for several things, laid out as Figma's auto layout lays out a
//! frame.
//!
//! A thin wrapper: one view whose flex properties its props set, holding the app's children as
//! they are. Its props are Figma's auto layout properties: the direction, the gap between the
//! children (or `spaceBetween`, Figma's "Auto" spacing), the padding on every side or on each, the
//! alignment (Figma's nine positions), wrapping, and its own sizing along each axis (hug, fill or
//! fixed). The gap and the padding are spacing roles (`spacing-gap<n>`, `spacing-inset<n>`), never
//! numbers, so any theme that maps the roles spaces it.
//!
//! **A child's sizing.** In Figma, hug, fill and fixed are each child's own, read against the auto
//! layout around it. JSX has no prop of a parent per child, so here they are each child's too:
//!
//! - An `AutoLayout` child sizes itself with its own `width` and `height` (`hug`, `fill`,
//!   `fixed` with `fixedWidth` / `fixedHeight`), read against the `AutoLayout` around it: `fill`
//!   along that one's direction grows to take the space left (`flex: 1`), across it stretches
//!   (`align-self: stretch`). It learns the direction from the context the `AutoLayout` around it
//!   provides, so nesting auto layouts is Figma's.
//! - Any other child (a `Button`, an app's component) hugs, at its natural size, as a component
//!   does in Figma. It fills or is fixed by the layout classes its `class` (Rust: its styles)
//!   gives it, which place a component without restyling it: `flex-1` fills along the
//!   `AutoLayout`'s direction, `self-stretch` fills across it, `w-[120px]` / `h-[40px]` fix it.
//!   A design tool writes them from the direction it reads in the `AutoLayout`'s props.

use nsh_core::element::{NumberRole, StyleRefinement};
use nsh_core::prelude::*;

use crate::theme::roles::{
    SPACING_GAP1, SPACING_GAP2, SPACING_GAP3, SPACING_GAP4, SPACING_GAP5, SPACING_GAP6,
    SPACING_GAP7, SPACING_GAP8, SPACING_INSET1, SPACING_INSET2, SPACING_INSET3, SPACING_INSET4,
    SPACING_INSET5, SPACING_INSET6, SPACING_INSET7, SPACING_INSET8,
};
use crate::theme::value;

/// The direction an [`AutoLayout`] puts its children in (Figma's direction).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PropEnum)]
pub enum AutoLayoutDirection {
    /// Top to bottom, a column: the default.
    #[default]
    Vertical,
    /// Start to end (left to right, or right to left in a right-to-left layout), a row.
    Horizontal,
}

/// Where an [`AutoLayout`] puts its children (Figma's nine alignment positions): the first word
/// is the vertical position, the second the horizontal one. With `spaceBetween`, the position
/// along the direction is the spread's, and only the one across it applies. Left and right are
/// the start and the end in a right-to-left layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PropEnum)]
pub enum AutoLayoutAlign {
    /// Top left: the default.
    #[default]
    TopLeft,
    /// Top, centered horizontally.
    Top,
    /// Top right.
    TopRight,
    /// Left, centered vertically.
    Left,
    /// Centered on both axes.
    Center,
    /// Right, centered vertically.
    Right,
    /// Bottom left.
    BottomLeft,
    /// Bottom, centered horizontally.
    Bottom,
    /// Bottom right.
    BottomRight,
}

impl AutoLayoutAlign {
    /// Every position, row by row from the top left.
    pub const ALL: [AutoLayoutAlign; 9] = [
        AutoLayoutAlign::TopLeft,
        AutoLayoutAlign::Top,
        AutoLayoutAlign::TopRight,
        AutoLayoutAlign::Left,
        AutoLayoutAlign::Center,
        AutoLayoutAlign::Right,
        AutoLayoutAlign::BottomLeft,
        AutoLayoutAlign::Bottom,
        AutoLayoutAlign::BottomRight,
    ];

    /// Its horizontal and vertical positions.
    pub const fn axes(self) -> (Place, Place) {
        use AutoLayoutAlign::*;
        let horizontal = match self {
            TopLeft | Left | BottomLeft => Place::Start,
            Top | Center | Bottom => Place::Center,
            TopRight | Right | BottomRight => Place::End,
        };
        let vertical = match self {
            TopLeft | Top | TopRight => Place::Start,
            Left | Center | Right => Place::Center,
            BottomLeft | Bottom | BottomRight => Place::End,
        };
        (horizontal, vertical)
    }
}

/// A position along one axis: an [`AutoLayoutAlign`]'s horizontal or vertical part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Place {
    /// The top, or the left (the start).
    Start,
    /// The middle.
    Center,
    /// The bottom, or the right (the end).
    End,
}

/// How an [`AutoLayout`] sizes itself along one axis (Figma's resizing), read against the
/// `AutoLayout` around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PropEnum)]
pub enum AutoLayoutSizing {
    /// As large as its content and padding: the default.
    #[default]
    Hug,
    /// Fills the `AutoLayout` around it: along its direction, the space its other children
    /// leave (`flex: 1`); across it, its whole breadth (`align-self: stretch`). Outside an
    /// `AutoLayout`, the parent is taken for a column, as a view is.
    Fill,
    /// The size `fixedWidth` or `fixedHeight` gives, in points (none given: its content's).
    Fixed,
}

/// The space between an [`AutoLayout`]'s children: a gap role (`spacing-gap<n>`, the layout
/// gap scale), or none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PropEnum)]
pub enum SpacingGap {
    /// No space: the default.
    #[default]
    None,
    /// `spacing-gap1`: tightly related components.
    Gap1,
    /// `spacing-gap2`: related components.
    Gap2,
    /// `spacing-gap3`: form rows.
    Gap3,
    /// `spacing-gap4`: form rows and small groups.
    Gap4,
    /// `spacing-gap5`: groups.
    Gap5,
    /// `spacing-gap6`: sections.
    Gap6,
    /// `spacing-gap7`: large sections.
    Gap7,
    /// `spacing-gap8`: page regions.
    Gap8,
}

impl SpacingGap {
    /// Every gap, none first.
    pub const ALL: [SpacingGap; 9] = [
        SpacingGap::None,
        SpacingGap::Gap1,
        SpacingGap::Gap2,
        SpacingGap::Gap3,
        SpacingGap::Gap4,
        SpacingGap::Gap5,
        SpacingGap::Gap6,
        SpacingGap::Gap7,
        SpacingGap::Gap8,
    ];

    /// The role it reads; none for [`SpacingGap::None`].
    pub const fn role(self) -> Option<NumberRole> {
        Some(match self {
            SpacingGap::None => return None,
            SpacingGap::Gap1 => SPACING_GAP1,
            SpacingGap::Gap2 => SPACING_GAP2,
            SpacingGap::Gap3 => SPACING_GAP3,
            SpacingGap::Gap4 => SPACING_GAP4,
            SpacingGap::Gap5 => SPACING_GAP5,
            SpacingGap::Gap6 => SPACING_GAP6,
            SpacingGap::Gap7 => SPACING_GAP7,
            SpacingGap::Gap8 => SPACING_GAP8,
        })
    }
}

/// The padding inside an [`AutoLayout`], on one side or every side: an inset role
/// (`spacing-inset<n>`, the container inset scale), or none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PropEnum)]
pub enum SpacingInset {
    /// No padding: the default.
    #[default]
    None,
    /// `spacing-inset1`.
    Inset1,
    /// `spacing-inset2`.
    Inset2,
    /// `spacing-inset3`.
    Inset3,
    /// `spacing-inset4`.
    Inset4,
    /// `spacing-inset5`.
    Inset5,
    /// `spacing-inset6`.
    Inset6,
    /// `spacing-inset7`.
    Inset7,
    /// `spacing-inset8`.
    Inset8,
}

impl SpacingInset {
    /// Every inset, none first.
    pub const ALL: [SpacingInset; 9] = [
        SpacingInset::None,
        SpacingInset::Inset1,
        SpacingInset::Inset2,
        SpacingInset::Inset3,
        SpacingInset::Inset4,
        SpacingInset::Inset5,
        SpacingInset::Inset6,
        SpacingInset::Inset7,
        SpacingInset::Inset8,
    ];

    /// The role it reads; none for [`SpacingInset::None`].
    pub const fn role(self) -> Option<NumberRole> {
        Some(match self {
            SpacingInset::None => return None,
            SpacingInset::Inset1 => SPACING_INSET1,
            SpacingInset::Inset2 => SPACING_INSET2,
            SpacingInset::Inset3 => SPACING_INSET3,
            SpacingInset::Inset4 => SPACING_INSET4,
            SpacingInset::Inset5 => SPACING_INSET5,
            SpacingInset::Inset6 => SPACING_INSET6,
            SpacingInset::Inset7 => SPACING_INSET7,
            SpacingInset::Inset8 => SPACING_INSET8,
        })
    }
}

/// The props of [`AutoLayout`]: Figma's auto layout properties.
#[derive(Debug, Clone, PartialEq, Props)]
pub struct AutoLayoutProps {
    /// The direction: `vertical` (the default, a column) or `horizontal` (a row).
    #[prop(default)]
    pub direction: Prop<AutoLayoutDirection>,
    /// The space between the children: a gap role (`gap1` … `gap8`), or `none` (the default).
    #[prop(default)]
    pub gap: Prop<SpacingGap>,
    /// Figma's "Auto" spacing: the children spread along the direction, the first and the last
    /// at the edges, and `gap` is the least space between them; `align` then places them across
    /// the direction only.
    #[prop(default)]
    pub space_between: Prop<bool>,
    /// The padding on every side: an inset role (`inset1` … `inset8`), or `none` (the default).
    #[prop(default)]
    pub padding: Prop<SpacingInset>,
    /// The padding at the top, instead of `padding`'s.
    pub padding_top: Prop<Option<SpacingInset>>,
    /// The padding on the right, instead of `padding`'s.
    pub padding_right: Prop<Option<SpacingInset>>,
    /// The padding at the bottom, instead of `padding`'s.
    pub padding_bottom: Prop<Option<SpacingInset>>,
    /// The padding on the left, instead of `padding`'s.
    pub padding_left: Prop<Option<SpacingInset>>,
    /// Where the children go: one of Figma's nine positions, `topLeft` by default.
    #[prop(default)]
    pub align: Prop<AutoLayoutAlign>,
    /// Wraps the children onto further lines when the next does not fit (Figma's wrap), the
    /// lines placed by `align` across the direction.
    #[prop(default)]
    pub wrap: Prop<bool>,
    /// The space between wrapped lines (Figma's spacing between rows); `gap`'s when unset.
    pub wrap_gap: Prop<Option<SpacingGap>>,
    /// Its width: `hug` its content (the default), `fill` the `AutoLayout` around it, or
    /// `fixed` at `fixedWidth`.
    #[prop(default)]
    pub width: Prop<AutoLayoutSizing>,
    /// Its height: `hug` its content (the default), `fill` the `AutoLayout` around it, or
    /// `fixed` at `fixedHeight`.
    #[prop(default)]
    pub height: Prop<AutoLayoutSizing>,
    /// Its width in points while `width` is `fixed`.
    pub fixed_width: Prop<Option<f32>>,
    /// Its height in points while `height` is `fixed`.
    pub fixed_height: Prop<Option<f32>>,
    /// The children, any number of them, laid out in order.
    pub children: Prop<Children>,
}

/// RayX's container for several things: its children in a row or a column, spaced, padded and
/// aligned as Figma's auto layout lays out a frame.
///
/// A thin wrapper: one view whose flex properties its props set, holding the children as they
/// are. Its props are Figma's: `direction`, `gap` (a gap role) or `spaceBetween`, `padding` (an
/// inset role, on every side or each: `paddingTop` …), `align` (nine positions), `wrap` (and
/// `wrapGap`), and its own `width` and `height`: `hug`, `fill` or `fixed` (`fixedWidth`,
/// `fixedHeight`), read against the `AutoLayout` around it.
///
/// A child that is not an `AutoLayout` hugs; its `class` makes it fill (`flex-1` along the
/// direction, `self-stretch` across it) or fixes it (`w-[120px]`): see the module's docs. Several
/// things go into a component's slot through an `AutoLayout` put into it.
///
/// ```tsx
/// <AutoLayout direction="horizontal" gap="gap2" padding="inset3" align="left" width="fill">
///   <Button text="Cancel" />
///   <Button text="Save" variant="primary" />
/// </AutoLayout>
/// ```
///
/// # Rust
///
/// ```no_run
/// use nsh_core::prelude::*;
/// use rayx_components::auto_layout::{AutoLayoutDirection, SpacingGap, SpacingInset, auto_layout};
/// use rayx_components::button::button;
///
/// let _ = auto_layout()
///     .direction(AutoLayoutDirection::Horizontal)
///     .gap(SpacingGap::Gap2)
///     .padding(SpacingInset::Inset3)
///     .child(button().text("Cancel"))
///     .child(button().text("Save"));
/// ```
#[component]
pub fn AutoLayout(props: AutoLayoutProps) -> impl IntoElement {
    // The direction of the `AutoLayout` around this one (none outside one), read before this one
    // provides its own to its children.
    let parent = use_context::<ParentLayout>().and_then(|p| p.0);
    provide_context(ParentLayout(Some(props.direction)));

    // One memo of the roles it reads; it runs again when the theme or a spacing prop changes.
    let look = computed(move || {
        Look::of(
            props.gap.get(),
            props.wrap_gap.get(),
            [
                props.padding_top.get(),
                props.padding_right.get(),
                props.padding_bottom.get(),
                props.padding_left.get(),
            ]
            .map(|side| side.unwrap_or(props.padding.get())),
        )
    });

    view()
        .style_with(move |s| {
            let horizontal = props.direction.get() == AutoLayoutDirection::Horizontal;
            let (x, y) = props.align.get().axes();
            let (main, cross) = if horizontal { (x, y) } else { (y, x) };
            let s = if horizontal {
                s.flex_row()
            } else {
                s.flex_col()
            };
            let s = if props.space_between.get() {
                s.justify_between()
            } else {
                match main {
                    Place::Start => s.justify_start(),
                    Place::Center => s.justify_center(),
                    Place::End => s.justify_end(),
                }
            };
            let s = match cross {
                Place::Start => s.items_start(),
                Place::Center => s.items_center(),
                Place::End => s.items_end(),
            };
            let s = if props.wrap.get() {
                let s = s.flex_wrap();
                match cross {
                    Place::Start => s.content_start(),
                    Place::Center => s.content_center(),
                    Place::End => s.content_end(),
                }
            } else {
                s
            };
            let s = look.with(|l| {
                let (along, between) = (px(l.gap), px(l.wrap_gap));
                let s = if horizontal {
                    s.gap_x(along).gap_y(between)
                } else {
                    s.gap_y(along).gap_x(between)
                };
                let [top, right, bottom, left] = l.padding;
                s.pt(px(top)).pr(px(right)).pb(px(bottom)).pl(px(left))
            });
            // Its own size, against the `AutoLayout` around it (outside one, a column's).
            let in_row = parent.is_some_and(|p| p.get() == AutoLayoutDirection::Horizontal);
            let s = sized(
                s,
                props.width.get(),
                props.fixed_width.get(),
                in_row,
                Axis::Width,
            );
            sized(
                s,
                props.height.get(),
                props.fixed_height.get(),
                !in_row,
                Axis::Height,
            )
        })
        .child(props.children)
}

/// The direction of the `AutoLayout` a view is in, as each `AutoLayout` provides it to its
/// children (none: not directly in one; a `Slot` provides none, being the box around what it
/// holds).
#[derive(Clone, Copy)]
pub(crate) struct ParentLayout(pub(crate) Option<Prop<AutoLayoutDirection>>);

/// One axis of a box.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Axis {
    Width,
    Height,
}

/// `s` sized along `axis` as `sizing` says: `along` when the axis is the direction of the
/// `AutoLayout` around it.
fn sized(
    s: StyleRefinement,
    sizing: AutoLayoutSizing,
    fixed: Option<f32>,
    along: bool,
    axis: Axis,
) -> StyleRefinement {
    match sizing {
        AutoLayoutSizing::Hug => s,
        AutoLayoutSizing::Fill if along => s.flex_1(),
        AutoLayoutSizing::Fill => s.self_stretch(),
        AutoLayoutSizing::Fixed => match (fixed, axis) {
            (None, _) => s,
            (Some(size), Axis::Width) => s.w(px(size)),
            (Some(size), Axis::Height) => s.h(px(size)),
        },
    }
}

/// The spacing an [`AutoLayout`] lays out with, for its gap and padding props: each field the
/// value of one of RayX's roles (`theme::value`), or 0 for none. Public for the crate's tests
/// (`tests/auto_layout_roles.rs`), not API.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    /// The gap's role, between the children.
    pub gap: f32,
    /// The wrap gap's role (the gap's when unset), between wrapped lines.
    pub wrap_gap: f32,
    /// The padding's roles: top, right, bottom, left.
    pub padding: [f32; 4],
}

impl Look {
    /// The roles of these props, read where the layout is (reactive in a binding).
    pub fn of(gap: SpacingGap, wrap_gap: Option<SpacingGap>, padding: [SpacingInset; 4]) -> Self {
        let gap_of = |g: SpacingGap| g.role().map_or(0.0, value);
        Look {
            gap: gap_of(gap),
            wrap_gap: gap_of(wrap_gap.unwrap_or(gap)),
            padding: padding.map(|p| p.role().map_or(0.0, value)),
        }
    }
}
