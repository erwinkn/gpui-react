use crate::{SharedStyle, geometry::Painted};
use gpui::{prelude::*, *};
use gpui_react::{Children, ReactChildren, ReactCommands, ReactEvents, ReactQueries, ReactView};
use serde::{Deserialize, Serialize};
use std::ops::Range;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Alignment {
    #[default]
    Top,
    Bottom,
}
#[derive(Debug, Default, Deserialize, gpui_react::ComponentProps)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ListProps {
    pub style: SharedStyle,
    /// None means that all logical rows are supplied as children.
    pub item_count: Option<usize>,
    pub window_start: usize,
    /// An estimate for rows not yet measured. Defaults to the window line height when props are applied.
    #[serde(deserialize_with = "crate::style::optional_finite")]
    pub estimated_item_height: Option<f32>,
    #[serde(deserialize_with = "crate::style::optional_finite")]
    pub overdraw: Option<f32>,
    pub alignment: Alignment,
    pub follow_tail: bool,
}
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct RowRange {
    pub start: usize,
    pub end: usize,
}
#[derive(Debug, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ListEvent {
    NeedRows {
        range: RowRange,
    },
    Scroll {
        range: RowRange,
        following_tail: bool,
    },
}
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum ListCommand {
    ScrollTo {
        index: usize,
        #[serde(default, deserialize_with = "crate::style::finite")]
        offset: f32,
    },
    End,
    /// Native components whose offscreen contents change can invalidate those heights.
    Remeasure {
        start: usize,
        end: usize,
    },
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListSnapshot {
    pub item_count: usize,
    pub supplied: RowRange,
    pub anchor: Anchor,
    pub following_tail: bool,
    pub painted: Option<Painted>,
    pub revision: u64,
    pub painted_rows: Option<RowRange>,
    pub max_scroll_y: f32,
}
#[derive(Debug, Serialize)]
pub struct Anchor {
    pub index: usize,
    pub offset: f32,
}

#[derive(PartialEq)]
struct TextMetrics {
    font: Font,
    size: Pixels,
    line_height: Pixels,
    rem: Pixels,
    white_space: WhiteSpace,
    overflow: Option<TextOverflow>,
    line_clamp: Option<usize>,
}
impl TextMetrics {
    fn current(window: &Window) -> Self {
        let style = window.text_style();
        let rem = window.rem_size();
        Self {
            font: style.font(),
            size: style.font_size.to_pixels(rem),
            line_height: style.line_height_in_pixels(rem),
            rem,
            white_space: style.white_space,
            overflow: style.text_overflow,
            line_clamp: style.line_clamp,
        }
    }
}
/// GPUI ListState is the sole owner of row geometry and native scroll physics.
/// Rows are React children rendered on demand from the host tree.
pub struct VirtualList {
    props: ListProps,
    rows: Vec<u32>,
    children: Option<Children>,
    state: ListState,
    estimate: Pixels,
    revision: u64,
    painted: Option<Painted>,
    painted_rows: Option<RowRange>,
    pending_range: Option<RowRange>,
    requested_range: Option<RowRange>,
    text_metrics: Option<TextMetrics>,
}
impl VirtualList {
    pub fn new(props: ListProps, window: &mut Window, _: &mut Context<Self>) -> Self {
        let estimate = px(props
            .estimated_item_height
            .unwrap_or_else(|| window.line_height().into())
            .max(1.));
        let state = Self::make_state(&props, props.item_count.unwrap_or(0), estimate);
        Self {
            props,
            rows: Vec::new(),
            children: None,
            state,
            estimate,
            revision: 0,
            painted: None,
            painted_rows: None,
            pending_range: None,
            requested_range: None,
            text_metrics: None,
        }
    }
    fn make_state(props: &ListProps, count: usize, estimate: Pixels) -> ListState {
        let state = ListState::new(
            0,
            match props.alignment {
                Alignment::Top => ListAlignment::Top,
                Alignment::Bottom => ListAlignment::Bottom,
            },
            px(props.overdraw.unwrap_or(0.).max(0.)),
        );
        state.splice_with_uniform_height(0..0, count, estimate);
        if props.follow_tail {
            state.set_follow_mode(FollowMode::Tail);
        }
        state
    }
    pub fn list_state(&self) -> ListState {
        self.state.clone()
    }
    fn start(&self) -> usize {
        if self.props.item_count.is_some() {
            self.props.window_start
        } else {
            0
        }
    }
    fn supplied(&self) -> Range<usize> {
        let start = self.start().min(self.state.item_count());
        start
            ..start
                .saturating_add(self.rows.len())
                .min(self.state.item_count())
    }
    pub fn snapshot(&self) -> ListSnapshot {
        let anchor = self.state.logical_scroll_top();
        let range = self.supplied();
        ListSnapshot {
            item_count: self.state.item_count(),
            supplied: RowRange {
                start: range.start,
                end: range.end,
            },
            anchor: Anchor {
                index: anchor.item_ix,
                offset: anchor.offset_in_item.into(),
            },
            following_tail: self.state.is_following_tail(),
            painted: self.painted,
            revision: self.revision,
            painted_rows: self.painted_rows,
            max_scroll_y: self.state.max_offset_for_scrollbar().y.into(),
        }
    }
    fn request_row(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let first = self.pending_range.is_none();
        let pending = self.pending_range.get_or_insert(RowRange {
            start: index,
            end: index + 1,
        });
        pending.start = pending.start.min(index);
        pending.end = pending.end.max(index + 1);
        if first {
            cx.defer_in(window, |this, _, cx| {
                if let Some(range) = this.pending_range.take()
                    && this.requested_range != Some(range)
                {
                    this.requested_range = Some(range);
                    cx.emit(ListEvent::NeedRows { range });
                }
            });
        }
    }
    pub fn apply_command(
        &mut self,
        command: ListCommand,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        match command {
            ListCommand::ScrollTo { index, offset } => {
                anyhow::ensure!(
                    index <= self.state.item_count(),
                    "list index exceeds item count"
                );
                self.state.scroll_to(ListOffset {
                    item_ix: index,
                    offset_in_item: px(offset),
                });
            }
            ListCommand::End => {
                if self.props.follow_tail {
                    self.state.set_follow_mode(FollowMode::Tail);
                } else {
                    self.state.scroll_to_end();
                }
            }
            ListCommand::Remeasure { start, end } => {
                anyhow::ensure!(
                    start <= end && end <= self.state.item_count(),
                    "invalid list measurement range"
                );
                self.state.remeasure_items(start..end);
            }
        }
        self.requested_range = None;
        self.revision += 1;
        cx.notify();
        Ok(())
    }
}
impl Render for VirtualList {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.weak_entity();
        self.state.set_scroll_handler(move |event, window, cx| {
            let range = RowRange {
                start: event.visible_range.start,
                end: event.visible_range.end,
            };
            let following_tail = event.is_following_tail;
            // GPUI holds its ListState borrow while reporting this event.
            // Read the final native state only after that borrow has ended.
            let this = this.clone();
            window.defer(cx, move |_, cx| {
                this.update(cx, |this, cx| {
                    this.requested_range = None;
                    this.revision += 1;
                    cx.emit(ListEvent::Scroll {
                        range,
                        following_tail,
                    });
                })
                .ok();
            });
        });
        let this = cx.weak_entity();
        let revision = self.revision;
        let item = cx.processor(|this, index: usize, window, cx| {
            let estimate = this.estimate;
            let placeholder = move || div().w_full().h(estimate).into_any_element();
            let Some(local) = index
                .checked_sub(this.start())
                .filter(|local| *local < this.rows.len())
            else {
                this.request_row(index, window, cx);
                return placeholder();
            };
            let Some(row) = this
                .children
                .as_ref()
                .and_then(|children| children.render(local, window, cx))
            else {
                return placeholder();
            };
            let owner = cx.weak_entity();
            div()
                .id(("row", this.rows[local]))
                .w_full()
                .block_mouse_except_scroll()
                .child(row)
                .on_painted(move |_, _, cx| {
                    owner
                        .update(cx, |this, _| {
                            let range = this.painted_rows.get_or_insert(RowRange {
                                start: index,
                                end: index + 1,
                            });
                            range.start = range.start.min(index);
                            range.end = range.end.max(index + 1);
                        })
                        .ok();
                })
                .into_any_element()
        });
        self.props
            .style
            .apply_interactive(
                div()
                    .id("list")
                    .flex()
                    .min_h_0()
                    .min_w_0()
                    .block_mouse_except_scroll(),
            )
            .on_painted(move |bounds, window, cx| {
                this.update(cx, |this, cx| {
                    this.painted = Some(Painted {
                        bounds: bounds.into(),
                        revision,
                        frame: gpui_react::current_frame(window, cx),
                    });
                    this.painted_rows = None;
                })
                .ok();
            })
            .child(ListLayout {
                owner: cx.weak_entity(),
                child: gpui::list(self.state.clone(), item)
                    .w_full()
                    .h_full()
                    .into_any_element(),
            })
    }
}

// GPUI requires ListState owners to invalidate heights when item geometry
// changes. At this point the enclosing div has applied inherited, hover, and
// focus text styles. Invalidating before native layout also preserves commands
// that need newly measured preceding rows to resolve a negative offset.
struct ListLayout {
    owner: WeakEntity<VirtualList>,
    child: AnyElement,
}
impl Element for ListLayout {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let metrics = TextMetrics::current(window);
        self.owner
            .update(cx, |list, _| {
                if list
                    .text_metrics
                    .as_ref()
                    .is_some_and(|old| old != &metrics)
                {
                    list.state.remeasure_items(0..list.state.item_count());
                }
                list.text_metrics = Some(metrics);
            })
            .ok();
        (self.child.request_layout(window, cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.prepaint(window, cx);
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let owner = self.owner.clone();
        crate::document::register_scroll_area(
            owner.entity_id().as_u64(),
            bounds,
            window,
            cx,
            move |distance, cx| {
                owner
                    .update(cx, |list, cx| {
                        let top = list.state.logical_scroll_top();
                        if list.state.max_offset_for_scrollbar().y <= px(0.)
                            || (distance > px(0.) && list.state.is_scrolled_to_end() == Some(true))
                            || (distance < px(0.)
                                && top.item_ix == 0
                                && top.offset_in_item <= px(0.))
                        {
                            return false;
                        }
                        list.state.scroll_by(distance);
                        list.requested_range = None;
                        list.revision += 1;
                        cx.notify();
                        true
                    })
                    .unwrap_or(false)
            },
        );
        self.child.paint(window, cx);
    }
}
impl IntoElement for ListLayout {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl ReactView for VirtualList {
    type Props = ListProps;
    fn create(props: Self::Props, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new(props, window, cx)
    }
    fn set_props(&mut self, props: Self::Props, window: &mut Window, cx: &mut Context<Self>) {
        let old_range = self.supplied();
        let estimate = px(props
            .estimated_item_height
            .unwrap_or_else(|| window.line_height().into())
            .max(1.));
        let count = props.item_count.unwrap_or(self.rows.len());
        let start = if props.item_count.is_some() {
            props.window_start.min(count)
        } else {
            0
        };
        let range = start..start.saturating_add(self.rows.len()).min(count);
        let remapped = range != old_range;
        let style_changed = props.style != self.props.style;
        let rebuilt = props.alignment != self.props.alignment
            || props.overdraw != self.props.overdraw
            || estimate != self.estimate
            || props.item_count.is_some() != self.props.item_count.is_some();
        if rebuilt {
            let top = self.state.logical_scroll_top();
            let follow =
                props.follow_tail && (!self.props.follow_tail || self.state.is_following_tail());
            self.state = Self::make_state(&props, count, estimate);
            if !follow {
                self.state.scroll_to(top);
            }
        } else {
            let old = self.state.item_count();
            if count != old {
                self.state.splice_with_uniform_height(
                    count.min(old)..old,
                    count.saturating_sub(old),
                    estimate,
                );
            }
            if props.follow_tail != self.props.follow_tail {
                self.state.set_follow_mode(if props.follow_tail {
                    FollowMode::Tail
                } else {
                    FollowMode::Normal
                });
            }
        }
        self.props = props;
        self.estimate = estimate;
        if style_changed || remapped {
            self.state.remeasure_items(self.supplied());
        }
        if rebuilt || remapped {
            self.requested_range = None;
        }
        self.revision += 1;
        cx.notify();
    }
}
impl ReactChildren for VirtualList {
    fn child_changed(&mut self, child: u32, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.rows.iter().position(|row| *row == child) {
            let index = self.start() + index;
            if index < self.state.item_count() {
                self.state.remeasure_items(index..index + 1);
            }
        }
        self.revision += 1;
        cx.notify();
    }
    fn set_children(&mut self, children: Children, _: &mut Window, cx: &mut Context<Self>) {
        if children.ids() == self.rows.as_slice() {
            self.children = Some(children);
            return;
        }
        let rows = children.ids().to_vec();
        let top = self.state.logical_scroll_top();
        let anchored = if self.props.item_count.is_none() && !self.state.is_following_tail() {
            self.rows.get(top.item_ix).copied()
        } else {
            None
        };
        let pinned_top = self.props.alignment == Alignment::Top
            && !self.state.is_following_tail()
            && top.item_ix == 0
            && top.offset_in_item <= px(0.);
        if self.props.item_count.is_none() {
            let prefix = self
                .rows
                .iter()
                .zip(&rows)
                .take_while(|(a, b)| a == b)
                .count();
            let suffix = self.rows[prefix..]
                .iter()
                .rev()
                .zip(rows[prefix..].iter().rev())
                .take_while(|(a, b)| a == b)
                .count();
            self.state.splice_with_uniform_height(
                prefix..self.rows.len() - suffix,
                rows.len() - prefix - suffix,
                self.estimate,
            );
        }
        self.rows = rows;
        self.children = Some(children);
        if self.props.item_count.is_some() {
            self.state.remeasure_items(self.supplied());
        }
        if pinned_top {
            self.state.scroll_to(ListOffset::default());
        } else if let Some(index) =
            anchored.and_then(|id| self.rows.iter().position(|row| *row == id))
        {
            self.state.scroll_to(ListOffset {
                item_ix: index,
                offset_in_item: top.offset_in_item,
            });
        }
        self.requested_range = None;
        self.revision += 1;
        cx.notify();
    }
}
impl EventEmitter<ListEvent> for VirtualList {}
impl ReactEvents for VirtualList {
    type Event = ListEvent;
}
impl ReactCommands for VirtualList {
    type Command = ListCommand;
    fn command(
        &mut self,
        command: Self::Command,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        self.apply_command(command, cx)
    }
}
impl ReactQueries for VirtualList {
    type Query = ();
    type Reply = ListSnapshot;
    fn query(
        &mut self,
        _: (),
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> anyhow::Result<Self::Reply> {
        Ok(self.snapshot())
    }
}
