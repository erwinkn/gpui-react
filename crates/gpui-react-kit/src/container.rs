use crate::{
    SharedStyle, Style,
    geometry::{Offset, Painted},
};
use gpui::{prelude::*, *};
use gpui_react::{ElementCommands, ElementContext, ElementQueries, ReactElement, RenderContext, Shared};
use serde::{Deserialize, Serialize};
use rustc_hash::FxHashMap;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Scroll {
    #[default]
    None,
    X,
    Y,
    Both,
}
#[derive(Debug, Default, Deserialize, gpui_react::ComponentProps)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ContainerProps {
    pub style: Shared<Style>,
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
const BLOCK_MOUSE: u8 = 1;
const MEASURE: u8 = 2;
const FOCUS: u8 = 4;
const LABEL: u8 = 8;
const GROUP: u8 = 16;

/// A GPUI div as a host-owned row, 16 bytes. Hover and active state live in
/// GPUI element state. Scroll and focus handles, labels, scroll groups, and
/// painted geometry exist only for the rows that ask for them and live in
/// `ContainerExtras`, keyed by slot.
pub struct Container {
    style: SharedStyle,
    revision: u32,
    scroll_mode: Scroll,
    flags: u8,
}
#[derive(Default)]
pub struct ContainerExtras {
    scroll: FxHashMap<u32, ScrollHandle>,
    focus: FxHashMap<u32, FocusHandle>,
    labels: FxHashMap<u32, SharedString>,
    groups: FxHashMap<u32, String>,
    painted: FxHashMap<u32, Painted>,
}
impl ContainerExtras {
    fn offset(&self, slot: u32) -> Offset {
        let p = self.scroll.get(&slot).map(|scroll| scroll.offset()).unwrap_or_default();
        Offset {
            x: -f32::from(p.x),
            y: -f32::from(p.y),
        }
    }
    fn focused(&self, slot: u32, window: &Window) -> bool {
        self.focus.get(&slot).is_some_and(|focus| focus.is_focused(window))
    }
    /// Point the row at the scroll handle its props ask for: none, a private
    /// one, or the one shared by its scroll group in this window.
    fn resolve_scroll(&mut self, slot: u32, props: &ContainerProps, window: &Window, cx: &mut App) {
        self.release_group(slot, window, cx);
        if props.scroll == Scroll::None {
            self.scroll.remove(&slot);
            return;
        }
        let Some(name) = group_name(props) else {
            self.scroll.insert(slot, ScrollHandle::new());
            return;
        };
        let key = (window.window_handle().window_id(), name.to_owned());
        let (handle, count) = cx
            .default_global::<ScrollGroups>()
            .0
            .entry(key)
            .or_insert_with(|| (ScrollHandle::new(), 0));
        *count += 1;
        self.scroll.insert(slot, handle.clone());
        self.groups.insert(slot, name.to_owned());
    }
    fn release_group(&mut self, slot: u32, window: &Window, cx: &mut App) {
        let Some(name) = self.groups.remove(&slot) else {
            return;
        };
        let groups = &mut cx.default_global::<ScrollGroups>().0;
        let key = (window.window_handle().window_id(), name);
        if let Some((_, count)) = groups.get_mut(&key) {
            *count -= 1;
            if *count == 0 {
                groups.remove(&key);
            }
        }
    }
}
impl Container {
    fn flags(props: &ContainerProps, extras: &mut ContainerExtras, slot: u32, cx: &mut App) -> u8 {
        let mut flags = (props.block_mouse as u8 * BLOCK_MOUSE) | (props.measure as u8 * MEASURE);
        if props.label.is_empty() {
            extras.labels.remove(&slot);
        } else {
            extras.labels.insert(slot, props.label.clone().into());
            flags |= LABEL;
        }
        if props.focusable {
            extras.focus.entry(slot).or_insert_with(|| cx.focus_handle());
            flags |= FOCUS;
        } else {
            extras.focus.remove(&slot);
        }
        if extras.groups.contains_key(&slot) {
            flags |= GROUP;
        }
        flags
    }
    fn label<'a>(&self, extras: &'a ContainerExtras, slot: u32) -> Option<&'a SharedString> {
        (self.flags & LABEL != 0).then(|| &extras.labels[&slot])
    }
}
impl ReactElement for Container {
    type Props = ContainerProps;
    type Extras = ContainerExtras;
    fn create(props: ContainerProps, extras: &mut ContainerExtras, cx: &mut ElementContext) -> Self {
        extras.resolve_scroll(cx.slot, &props, cx.window, cx.cx);
        Self {
            flags: Self::flags(&props, extras, cx.slot, cx.cx),
            style: props.style,
            scroll_mode: props.scroll,
            revision: 0,
        }
    }
    fn set_props(&mut self, props: ContainerProps, extras: &mut ContainerExtras, cx: &mut ElementContext) {
        let slot = cx.slot;
        if !props.focusable && extras.focused(slot, cx.window) {
            cx.window.blur();
        }
        let group = (self.flags & GROUP != 0).then(|| extras.groups[&slot].as_str());
        if props.scroll != self.scroll_mode || group_name(&props) != group {
            extras.resolve_scroll(slot, &props, cx.window, cx.cx);
        }
        self.flags = Self::flags(&props, extras, slot, cx.cx);
        self.style = props.style;
        self.scroll_mode = props.scroll;
        self.revision += 1;
    }
    fn render(&self, extras: &ContainerExtras, cx: &mut RenderContext) -> AnyElement {
        let id = cx.id;
        let slot = cx.slot;
        let emitter = cx.emitter();
        let measure = self.flags & MEASURE != 0;
        // A container needs GPUI element state only when it scrolls, focuses,
        // listens, or carries hover, active, or focus styles. Everything else
        // is a plain div with no per-frame state.
        if self.scroll_mode == Scroll::None
            && self.flags & (FOCUS | LABEL | BLOCK_MOUSE) == 0
            && emitter.is_none()
            && !self.style.is_interactive()
        {
            let children = cx.children();
            let mut el = self.style.apply(div().flex().flex_col()).children(children);
            if measure {
                el = el.on_painted(self.measure_callback(cx));
            }
            return el.into_any_element();
        }
        let mut el = div().id(cx.element_id()).flex().flex_col();
        if let Some(label) = self.label(extras, slot) {
            el = el.aria_label(label.clone());
        }
        let scroll = extras.scroll.get(&slot).cloned();
        let scrolls_vertically = matches!(self.scroll_mode, Scroll::Y | Scroll::Both);
        if scrolls_vertically || measure {
            // GPUI keeps one paint listener per element, so vertical scroll
            // registration and measurement share it.
            let scroll_area = scrolls_vertically.then(|| (cx.host(), scroll.clone().unwrap()));
            let measure = measure.then(|| self.measure_callback(cx));
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
            let scroll = scroll.clone();
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
        if self.flags & BLOCK_MOUSE != 0 {
            el = el.block_mouse_except_scroll();
        }
        if self.flags & FOCUS != 0 {
            el = el.track_focus(&extras.focus[&slot]);
        }
        if let Some(scroll) = &scroll {
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
    fn unmount(&mut self, extras: &mut ContainerExtras, cx: &mut ElementContext) {
        let slot = cx.slot;
        if extras.focused(slot, cx.window) {
            cx.window.blur();
        }
        extras.release_group(slot, cx.window, cx.cx);
        extras.scroll.remove(&slot);
        extras.focus.remove(&slot);
        extras.labels.remove(&slot);
        extras.painted.remove(&slot);
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
                host.update_element::<Container, _>(id, |_, extras, slot| {
                    extras.painted.insert(slot, painted);
                });
            })
            .ok();
        }
    }
}
impl ElementCommands for Container {
    type Command = ContainerCommand;
    fn command(
        &mut self,
        command: ContainerCommand,
        extras: &mut ContainerExtras,
        cx: &mut ElementContext,
    ) -> anyhow::Result<()> {
        let slot = cx.slot;
        match command {
            ContainerCommand::Focus => {
                let focus = extras
                    .focus
                    .get(&slot)
                    .ok_or_else(|| anyhow::anyhow!("container is not focusable"))?;
                cx.window.focus(focus, cx.cx);
            }
            ContainerCommand::Blur => {
                if extras.focused(slot, cx.window) {
                    cx.window.blur();
                }
            }
            ContainerCommand::ScrollTo { x, y } => {
                let scroll = extras
                    .scroll
                    .get(&slot)
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
    fn query(&mut self, _: (), extras: &mut ContainerExtras, cx: &mut ElementContext) -> anyhow::Result<Self::Reply> {
        Ok(ContainerSnapshot {
            painted: extras.painted.get(&cx.slot).copied(),
            revision: self.revision as u64,
            offset: extras.offset(cx.slot),
            child_count: cx.child_count,
            focused: extras.focused(cx.slot, cx.window),
        })
    }
}

/// Horizontal scroll groups share one handle per window and group name. The
/// count is the number of live rows pointing at the handle.
#[derive(Default)]
struct ScrollGroups(HashMap<(WindowId, String), (ScrollHandle, u32)>);
impl Global for ScrollGroups {}
fn group_name(props: &ContainerProps) -> Option<&str> {
    (props.scroll == Scroll::X)
        .then_some(props.scroll_group.as_deref())
        .flatten()
        .filter(|name| !name.is_empty())
}

#[cfg(test)]
mod layout {
    #[test]
    fn container_row_stays_small() {
        assert!(std::mem::size_of::<super::Container>() <= 16, "{}", std::mem::size_of::<super::Container>());
    }
}
