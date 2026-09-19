use crate::{
    SharedStyle,
    geometry::{Offset, Painted},
};
use gpui::{prelude::*, *};
use gpui_react::{ElementCommands, ElementContext, ElementQueries, ReactElement, RenderContext};
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
    pub style: SharedStyle,
    pub scroll: Scroll,
    pub focusable: bool,
    pub label: String,
    pub scroll_group: Option<String>,
    pub block_mouse: bool,
    /// Record painted bounds for queries. Off by default: it costs a paint
    /// callback per frame.
    pub measure: bool,
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
/// A GPUI div as a host-owned row, 32 bytes. Scroll and focus handles exist
/// only when the props ask for them; hover and active state live in GPUI
/// element state. Label, scroll group, focus, and painted geometry are rare
/// and live behind one optional box.
pub struct Container {
    style: SharedStyle,
    scroll: Option<Rc<ScrollHandle>>,
    rare: Option<Box<Rare>>,
    revision: u32,
    scroll_mode: Scroll,
    block_mouse: bool,
    measure: bool,
}
#[derive(Default)]
struct Rare {
    label: SharedString,
    scroll_group: Option<String>,
    focus: Option<FocusHandle>,
    painted: Option<Painted>,
}
impl Container {
    fn offset(&self) -> Offset {
        let p = self.scroll.as_ref().map(|scroll| scroll.offset()).unwrap_or_default();
        Offset {
            x: -f32::from(p.x),
            y: -f32::from(p.y),
        }
    }
    pub fn scroll_handle(&self) -> Option<ScrollHandle> {
        self.scroll.as_ref().map(|scroll| scroll.as_ref().clone())
    }
    fn focus(&self) -> Option<&FocusHandle> {
        self.rare.as_ref().and_then(|rare| rare.focus.as_ref())
    }
    fn focused(&self, window: &Window) -> bool {
        self.focus().is_some_and(|focus| focus.is_focused(window))
    }
    fn label(&self) -> Option<&SharedString> {
        self.rare
            .as_ref()
            .map(|rare| &rare.label)
            .filter(|label| !label.is_empty())
    }
    fn group_name(&self) -> Option<&str> {
        (self.scroll_mode == Scroll::X)
            .then(|| self.rare.as_ref()?.scroll_group.as_deref())
            .flatten()
            .filter(|name| !name.is_empty())
    }
    fn rare(props: &ContainerProps, previous: Option<Box<Rare>>, cx: &mut ElementContext) -> Option<Box<Rare>> {
        if props.label.is_empty() && props.scroll_group.is_none() && !props.focusable {
            return None;
        }
        let mut rare = previous.map(|rare| *rare).unwrap_or_default();
        rare.label = props.label.clone().into();
        rare.scroll_group = props.scroll_group.clone();
        rare.focus = match (props.focusable, rare.focus.take()) {
            (true, Some(focus)) => Some(focus),
            (true, None) => Some(cx.cx.focus_handle()),
            (false, _) => None,
        };
        Some(Box::new(rare))
    }
}
impl ReactElement for Container {
    type Props = ContainerProps;
    fn create(props: ContainerProps, cx: &mut ElementContext) -> Self {
        Self {
            scroll: resolve_scroll(&props, cx.window, cx.cx),
            rare: Self::rare(&props, None, cx),
            style: props.style,
            scroll_mode: props.scroll,
            block_mouse: props.block_mouse,
            measure: props.measure,
            revision: 0,
        }
    }
    fn set_props(&mut self, props: ContainerProps, cx: &mut ElementContext) {
        if !props.focusable && self.focused(cx.window) {
            cx.window.blur();
        }
        if props.scroll != self.scroll_mode || group_name(&props) != self.group_name() {
            self.scroll = resolve_scroll(&props, cx.window, cx.cx);
        }
        let painted = self.rare.as_ref().and_then(|rare| rare.painted);
        self.rare = Self::rare(&props, self.rare.take(), cx);
        if let (Some(painted), Some(rare)) = (painted, self.rare.as_mut()) {
            rare.painted = Some(painted);
        }
        self.style = props.style;
        self.scroll_mode = props.scroll;
        self.block_mouse = props.block_mouse;
        self.measure = props.measure;
        self.revision += 1;
    }
    fn render(&self, cx: &mut RenderContext) -> AnyElement {
        let id = cx.id;
        let emitter = cx.emitter();
        // A container needs GPUI element state only when it scrolls, focuses,
        // listens, or carries hover, active, or focus styles. Everything else
        // is a plain div with no per-frame state.
        if self.scroll.is_none()
            && self.rare.is_none()
            && emitter.is_none()
            && !self.style.is_interactive()
            && !self.block_mouse
        {
            let children = cx.children();
            let mut el = self.style.apply(div().flex().flex_col()).children(children);
            if self.measure {
                el = el.on_painted(self.measure_callback(cx));
            }
            return el.into_any_element();
        }
        let mut el = div().id(cx.element_id()).flex().flex_col();
        if let Some(label) = self.label() {
            el = el.aria_label(label.clone());
        }
        let scrolls_vertically = matches!(self.scroll_mode, Scroll::Y | Scroll::Both);
        if scrolls_vertically || self.measure {
            // GPUI keeps one paint listener per element, so vertical scroll
            // registration and measurement share it.
            let scroll_area = scrolls_vertically.then(|| (cx.host(), self.scroll.clone().unwrap()));
            let measure = self.measure.then(|| self.measure_callback(cx));
            el = el.on_painted(move |bounds, window, cx| {
                if let Some((host, scroll)) = &scroll_area {
                    let scroll = scroll.clone();
                    let host = host.clone();
                    crate::document::register_scroll_area(id as u64, bounds, window, cx, move |distance, cx| {
                        let old = scroll.offset();
                        let y = (old.y - distance).clamp(-scroll.max_offset().y, px(0.));
                        if y == old.y {
                            return false;
                        }
                        scroll.set_offset(point(old.x, y));
                        host.update(cx, |_, cx| cx.notify()).ok();
                        true
                    });
                }
                if let Some(measure) = &measure {
                    measure(bounds, window, cx);
                }
            });
        }
        if let Some(emitter) = emitter {
            let click = emitter.clone();
            el = el.on_click(move |event: &ClickEvent, _, cx| {
                click.emit(
                    &ContainerEvent::Click {
                        x: event.position().x.into(),
                        y: event.position().y.into(),
                    },
                    cx,
                )
            });
            let scroll = self.scroll.clone();
            el = el.on_scroll_wheel(move |event: &ScrollWheelEvent, window, cx| {
                let position = event.position;
                let delta = event.delta.pixel_delta(window.line_height());
                let emitter = emitter.clone();
                let scroll = scroll.clone();
                // Report the offset after GPUI has applied this wheel movement.
                cx.defer(move |cx| {
                    let p = scroll.as_ref().map(|scroll| scroll.offset()).unwrap_or_default();
                    emitter.emit(
                        &ContainerEvent::Wheel {
                            x: position.x.into(),
                            y: position.y.into(),
                            dx: delta.x.into(),
                            dy: delta.y.into(),
                            offset: Offset {
                                x: -f32::from(p.x),
                                y: -f32::from(p.y),
                            },
                        },
                        cx,
                    )
                });
            });
        }
        // BlockMouse also excludes ancestor hitboxes. A general composition
        // container keeps GPUI's normal hit testing so its parent can handle a
        // click. Native overlays can opt into block_mouse_except_scroll().
        if self.block_mouse {
            el = el.block_mouse_except_scroll();
        }
        if let Some(focus) = self.focus() {
            el = el.track_focus(focus);
        }
        if let Some(scroll) = &self.scroll {
            el = match self.scroll_mode {
                Scroll::None => el,
                Scroll::X => el
                    .overflow_x_scroll()
                    .restrict_scroll_to_axis()
                    .track_scroll(scroll),
                Scroll::Y => el
                    .overflow_y_scroll()
                    .restrict_scroll_to_axis()
                    .track_scroll(scroll),
                Scroll::Both => {
                    el.style().allow_concurrent_scroll = Some(true);
                    el.overflow_scroll().track_scroll(scroll)
                }
            };
        }
        let children = cx.children();
        self.style
            .apply_interactive(el)
            .children(children)
            .into_any_element()
    }
    fn unmount(&mut self, cx: &mut ElementContext) {
        if self.focused(cx.window) {
            cx.window.blur();
        }
    }
}
impl Container {
    fn measure_callback(
        &self,
        cx: &RenderContext,
    ) -> impl Fn(Bounds<Pixels>, &mut Window, &mut App) + 'static {
        let host = cx.host();
        let id = cx.id;
        let revision = self.revision;
        move |bounds, window, cx| {
            let painted = Painted {
                bounds: bounds.into(),
                revision: revision as u64,
                frame: gpui_react::current_frame(window, cx),
            };
            host.update(cx, |host, _| {
                host.update_element::<Container, _>(id, |container| {
                    container
                        .rare
                        .get_or_insert_with(Default::default)
                        .painted = Some(painted)
                });
            })
            .ok();
        }
    }
}
impl ElementCommands for Container {
    type Command = ContainerCommand;
    fn command(&mut self, command: ContainerCommand, cx: &mut ElementContext) -> anyhow::Result<()> {
        match command {
            ContainerCommand::Focus => {
                let focus = self
                    .focus()
                    .ok_or_else(|| anyhow::anyhow!("container is not focusable"))?;
                cx.window.focus(focus, cx.cx);
            }
            ContainerCommand::Blur => {
                if self.focused(cx.window) {
                    cx.window.blur();
                }
            }
            ContainerCommand::ScrollTo { x, y } => {
                let scroll = self
                    .scroll
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("container does not scroll"))?;
                scroll.set_offset(point(px(-x), px(-y)));
            }
        }
        Ok(())
    }
}
impl ElementQueries for Container {
    type Query = ();
    type Reply = ContainerSnapshot;
    fn query(&mut self, _: (), cx: &mut ElementContext) -> anyhow::Result<Self::Reply> {
        Ok(ContainerSnapshot {
            painted: self.rare.as_ref().and_then(|rare| rare.painted),
            revision: self.revision as u64,
            offset: self.offset(),
            child_count: cx.child_count,
            focused: self.focused(cx.window),
        })
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
fn resolve_scroll(props: &ContainerProps, window: &Window, cx: &mut App) -> Option<Rc<ScrollHandle>> {
    if props.scroll == Scroll::None {
        return None;
    }
    let Some(name) = group_name(props) else {
        return Some(Rc::new(ScrollHandle::new()));
    };
    let groups = &mut cx.default_global::<ScrollGroups>().0;
    groups.retain(|_, handle| handle.strong_count() > 0);
    let key = (window.window_handle().window_id(), name.to_owned());
    if let Some(handle) = groups.get(&key).and_then(Weak::upgrade) {
        return Some(handle);
    }
    let handle = Rc::new(ScrollHandle::new());
    groups.insert(key, Rc::downgrade(&handle));
    Some(handle)
}

#[cfg(test)]
mod layout {
    #[test]
    fn container_row_stays_small() {
        assert!(std::mem::size_of::<super::Container>() <= 32, "{}", std::mem::size_of::<super::Container>());
    }
}
