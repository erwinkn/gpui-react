use crate::{
    Style,
    geometry::{Offset, Painted},
};
use gpui::{prelude::*, *};
use gpui_react::{ReactChildren, ReactCommands, ReactEvents, ReactQueries, ReactView};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    rc::{Rc, Weak},
};

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Scroll {
    #[default]
    None,
    X,
    Y,
    Both,
}
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ContainerProps {
    pub style: Style,
    pub scroll: Scroll,
    pub focusable: bool,
    pub label: String,
    pub scroll_group: Option<String>,
    pub block_mouse: bool,
}
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ContainerEvent {
    Click {
        x: f32,
        y: f32,
    },
    Wheel {
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
        offset: Offset,
    },
}
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum ContainerCommand {
    Focus,
    Blur,
    ScrollTo {
        #[serde(deserialize_with = "crate::style::finite")]
        x: f32,
        #[serde(deserialize_with = "crate::style::finite")]
        y: f32,
    },
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerSnapshot {
    pub painted: Option<Painted>,
    pub revision: u64,
    pub offset: Offset,
    pub child_count: usize,
    pub focused: bool,
}
/// An ordinary GPUI div backed by stable child entity handles and native scroll state.
pub struct Container {
    props: ContainerProps,
    children: Vec<AnyView>,
    scroll: Rc<ScrollHandle>,
    focus: FocusHandle,
    revision: u64,
    painted: Option<Painted>,
}
impl Container {
    pub fn new(props: ContainerProps, window: &Window, cx: &mut Context<Self>) -> Self {
        let scroll = resolve_scroll(&props, window, cx);
        Self {
            props,
            children: Vec::new(),
            scroll,
            focus: cx.focus_handle(),
            revision: 0,
            painted: None,
        }
    }
    fn offset(&self) -> Offset {
        let p = self.scroll.offset();
        Offset {
            x: -f32::from(p.x),
            y: -f32::from(p.y),
        }
    }
    pub fn scroll_handle(&self) -> ScrollHandle {
        self.scroll.as_ref().clone()
    }
    pub fn snapshot(&self, window: &Window) -> ContainerSnapshot {
        ContainerSnapshot {
            painted: self.painted,
            revision: self.revision,
            offset: self.offset(),
            child_count: self.children.len(),
            focused: self.focus.is_focused(window),
        }
    }
}
impl Render for Container {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.weak_entity();
        let scroll_owner = this.clone();
        let scrolls_vertically = matches!(self.props.scroll, Scroll::Y | Scroll::Both);
        let revision = self.revision;
        let mut el = div()
            .id("container")
            .flex()
            .flex_col()
            .aria_label(self.props.label.clone())
            .on_painted(move |bounds, window, cx| {
                if scrolls_vertically {
                    let owner = scroll_owner.clone();
                    crate::document::register_scroll_area(
                        owner.entity_id(),
                        bounds,
                        window,
                        cx,
                        move |distance, cx| {
                            owner
                                .update(cx, |container, cx| {
                                    let old = container.scroll.offset();
                                    let y = (old.y - distance)
                                        .clamp(-container.scroll.max_offset().y, px(0.));
                                    if y == old.y {
                                        return false;
                                    }
                                    container.scroll.set_offset(point(old.x, y));
                                    container.revision += 1;
                                    cx.notify();
                                    true
                                })
                                .unwrap_or(false)
                        },
                    );
                }
                this.update(cx, |this, cx| {
                    this.painted = Some(Painted {
                        bounds: bounds.into(),
                        revision,
                        frame: gpui_react::current_frame(window, cx),
                    })
                })
                .ok();
            })
            .on_click(cx.listener(|_, event: &ClickEvent, _, cx| {
                cx.emit(ContainerEvent::Click {
                    x: event.position().x.into(),
                    y: event.position().y.into(),
                })
            }))
            .on_scroll_wheel(cx.listener(|_, event: &ScrollWheelEvent, window, cx| {
                let position = event.position;
                let delta = event.delta.pixel_delta(window.line_height());
                cx.defer_in(window, move |this, _, cx| {
                    cx.emit(ContainerEvent::Wheel {
                        x: position.x.into(),
                        y: position.y.into(),
                        dx: delta.x.into(),
                        dy: delta.y.into(),
                        offset: this.offset(),
                    })
                });
            }));
        // BlockMouse also excludes ancestor hitboxes. A general composition
        // container keeps GPUI's normal hit testing so its parent can handle a
        // click. Native overlays can opt into block_mouse_except_scroll().
        if self.props.block_mouse {
            el = el.block_mouse_except_scroll();
        }
        if self.props.focusable {
            el = el.track_focus(&self.focus);
        }
        el = match self.props.scroll {
            Scroll::None => el,
            Scroll::X => el
                .overflow_x_scroll()
                .restrict_scroll_to_axis()
                .track_scroll(&self.scroll),
            Scroll::Y => el
                .overflow_y_scroll()
                .restrict_scroll_to_axis()
                .track_scroll(&self.scroll),
            Scroll::Both => {
                el.style().allow_concurrent_scroll = Some(true);
                el.overflow_scroll().track_scroll(&self.scroll)
            }
        };
        self.props
            .style
            .apply_interactive(el)
            .children(self.children.clone())
    }
}
impl ReactView for Container {
    type Props = ContainerProps;
    fn create(props: Self::Props, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new(props, window, cx)
    }
    fn set_props(&mut self, props: Self::Props, window: &mut Window, cx: &mut Context<Self>) {
        if !props.focusable && self.focus.is_focused(window) {
            window.blur();
        }
        if group_name(&props) != group_name(&self.props) {
            self.scroll = resolve_scroll(&props, window, cx);
        }
        self.props = props;
        self.revision += 1;
        cx.notify();
    }
    fn unmounting(&mut self, window: &mut Window, _: &mut Context<Self>) {
        if self.focus.is_focused(window) {
            window.blur();
        }
    }
}
impl ReactChildren for Container {
    fn set_children(&mut self, children: Vec<AnyView>, _: &mut Window, cx: &mut Context<Self>) {
        self.children = children;
        self.revision += 1;
        cx.notify();
    }
}
impl EventEmitter<ContainerEvent> for Container {}
impl ReactEvents for Container {
    type Event = ContainerEvent;
}
impl ReactCommands for Container {
    type Command = ContainerCommand;
    fn command(
        &mut self,
        command: Self::Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        match command {
            ContainerCommand::Focus => {
                anyhow::ensure!(self.props.focusable, "container is not focusable");
                window.focus(&self.focus, cx);
            }
            ContainerCommand::Blur => {
                if self.focus.is_focused(window) {
                    window.blur();
                }
            }
            ContainerCommand::ScrollTo { x, y } => {
                self.scroll.set_offset(point(px(-x), px(-y)));
                cx.notify();
            }
        }
        Ok(())
    }
}
impl ReactQueries for Container {
    type Query = ();
    type Reply = ContainerSnapshot;
    fn query(
        &mut self,
        _: (),
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> anyhow::Result<Self::Reply> {
        Ok(self.snapshot(window))
    }
}

#[derive(Default)]
struct ScrollGroups(HashMap<(WindowId, String), Weak<ScrollHandle>>);
impl Global for ScrollGroups {}
fn group_name(props: &ContainerProps) -> Option<&str> {
    (props.scroll == Scroll::X)
        .then_some(props.scroll_group.as_deref())
        .flatten()
        .filter(|name| !name.is_empty())
}
fn resolve_scroll(props: &ContainerProps, window: &Window, cx: &mut App) -> Rc<ScrollHandle> {
    let Some(name) = group_name(props) else {
        return Rc::new(ScrollHandle::new());
    };
    if !cx.has_global::<ScrollGroups>() {
        cx.set_global(ScrollGroups::default());
    }
    let groups = &mut cx.global_mut::<ScrollGroups>().0;
    groups.retain(|_, handle| handle.strong_count() > 0);
    let key = (window.window_handle().window_id(), name.to_owned());
    if let Some(handle) = groups.get(&key).and_then(Weak::upgrade) {
        return handle;
    }
    let handle = Rc::new(ScrollHandle::new());
    groups.insert(key, Rc::downgrade(&handle));
    handle
}
