use crate::{Style, geometry::Painted};
use gpui::{prelude::*, *};
use gpui_react::{ReactChildren, ReactCommands, ReactEvents, ReactQueries, ReactView};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, ops::Range};

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Alignment {
    #[default]
    Top,
    Bottom,
}
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ListProps {
    pub style: Style,
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
struct Row {
    view: AnyView,
    focus: FocusHandle,
}
/// GPUI ListState is the sole owner of row geometry and native scroll physics.
pub struct VirtualList {
    props: ListProps,
    rows: Vec<Row>,
    state: ListState,
    estimate: Pixels,
    revision: u64,
    painted: Option<Painted>,
    painted_rows: Option<RowRange>,
    pending_range: Option<RowRange>,
    requested_range: Option<RowRange>,
}
impl VirtualList {
    pub fn new(props: ListProps, window: &mut Window, _: &mut Context<Self>) -> Self {
        let estimate = px(props
            .estimated_item_height
            .unwrap_or(window.line_height().into())
            .max(1.));
        let state = Self::make_state(&props, props.item_count.unwrap_or(0), estimate);
        Self {
            props,
            rows: Vec::new(),
            state,
            estimate,
            revision: 0,
            painted: None,
            painted_rows: None,
            pending_range: None,
            requested_range: None,
        }
    }
    fn make_state(props: &ListProps, count: usize, estimate: Pixels) -> ListState {
        let state = ListState::new(
            count,
            match props.alignment {
                Alignment::Top => ListAlignment::Top,
                Alignment::Bottom => ListAlignment::Bottom,
            },
            px(props.overdraw.unwrap_or(0.).max(0.)),
        )
        .with_uniform_item_height(estimate);
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
    fn set_focus_handles(&self) {
        let count = self.supplied().len();
        self.state.set_item_focus_handles(
            self.start(),
            self.rows
                .iter()
                .take(count)
                .map(|row| Some(row.focus.clone())),
        );
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
            let Some(row) = index
                .checked_sub(this.start())
                .and_then(|local| this.rows.get(local))
            else {
                this.request_row(index, window, cx);
                return div().w_full().h(this.estimate).into_any_element();
            };
            let owner = cx.weak_entity();
            div()
                .id(("row", row.view.entity_id()))
                .w_full()
                .track_focus(&row.focus)
                .tab_stop(false)
                .block_mouse_except_scroll()
                .child(row.view.clone())
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
            .child(gpui::list(self.state.clone(), item).w_full().h_full())
    }
}
impl ReactView for VirtualList {
    type Props = ListProps;
    fn create(props: Self::Props, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new(props, window, cx)
    }
    fn set_props(&mut self, props: Self::Props, window: &mut Window, cx: &mut Context<Self>) {
        let old_range = self.supplied();
        self.state
            .set_item_focus_handles(old_range.start, old_range.clone().map(|_| None));
        let estimate = px(props
            .estimated_item_height
            .unwrap_or(window.line_height().into())
            .max(1.));
        let count = props.item_count.unwrap_or(self.rows.len());
        if props.alignment != self.props.alignment
            || props.overdraw != self.props.overdraw
            || estimate != self.estimate
            || props.item_count.is_some() != self.props.item_count.is_some()
        {
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
                self.state
                    .splice(count.min(old)..old, count.saturating_sub(old));
                self.state = self.state.clone().with_uniform_item_height(estimate);
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
        self.set_focus_handles();
        self.state.remeasure_items(self.supplied());
        self.requested_range = None;
        self.revision += 1;
        cx.notify();
    }
}
impl ReactChildren for VirtualList {
    fn children_changed(&mut self, children: &[EntityId], _: &mut Window, cx: &mut Context<Self>) {
        let changed: std::collections::HashSet<_> = children.iter().copied().collect();
        let start = self.start();
        for (index, row) in self.rows.iter().enumerate() {
            if changed.contains(&row.view.entity_id()) && start + index < self.state.item_count() {
                self.state.remeasure_items(start + index..start + index + 1);
            }
        }
        self.revision += 1;
        cx.notify();
    }
    fn set_children(&mut self, children: Vec<AnyView>, _: &mut Window, cx: &mut Context<Self>) {
        let old_range = self.supplied();
        let top = self.state.logical_scroll_top();
        let anchored = if self.props.item_count.is_none() && !self.state.is_following_tail() {
            self.rows.get(top.item_ix).map(|row| row.view.entity_id())
        } else {
            None
        };
        let pinned_top = self.props.alignment == Alignment::Top
            && !self.state.is_following_tail()
            && top.item_ix == 0
            && top.offset_in_item <= px(0.);
        let mut focus: HashMap<_, _> = self
            .rows
            .iter()
            .map(|row| (row.view.entity_id(), row.focus.clone()))
            .collect();
        let rows: Vec<Row> = children
            .into_iter()
            .map(|view| {
                let focus = focus
                    .remove(&view.entity_id())
                    .unwrap_or_else(|| cx.focus_handle());
                Row { view, focus }
            })
            .collect();
        if self.props.item_count.is_none() {
            let prefix = self
                .rows
                .iter()
                .zip(&rows)
                .take_while(|(a, b)| a.view.entity_id() == b.view.entity_id())
                .count();
            let suffix = self.rows[prefix..]
                .iter()
                .rev()
                .zip(rows[prefix..].iter().rev())
                .take_while(|(a, b)| a.view.entity_id() == b.view.entity_id())
                .count();
            self.state.splice_focusable(
                prefix..self.rows.len() - suffix,
                rows[prefix..rows.len() - suffix]
                    .iter()
                    .map(|row| Some(row.focus.clone())),
            );
            self.state = self.state.clone().with_uniform_item_height(self.estimate);
        } else {
            self.state
                .set_item_focus_handles(old_range.start, old_range.map(|_| None));
        }
        self.rows = rows;
        self.set_focus_handles();
        self.state.remeasure_items(self.supplied());
        if pinned_top {
            self.state.scroll_to(ListOffset::default());
        } else if let Some(index) =
            anchored.and_then(|id| self.rows.iter().position(|row| row.view.entity_id() == id))
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
