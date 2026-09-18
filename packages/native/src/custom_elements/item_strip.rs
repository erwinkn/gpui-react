//! Scroll selected items into view during native prepaint, including the first frame.
use super::{CustomElement, CustomElementFactory, CustomRenderContext};
use gpui::{
    div, prelude::*, AnyElement, App, Bounds, Element, ElementId, GlobalElementId,
    InspectorElementId, LayoutId, Pixels, ScrollHandle, Window,
};
use serde_json::Value;
use std::{cell::RefCell, rc::Rc};
pub struct ItemStripFactory;
impl CustomElementFactory for ItemStripFactory {
    fn element_type(&self) -> &str {
        "cherry-item-strip"
    }
    fn create(&self, _: u64) -> Box<dyn CustomElement> {
        Box::new(Strip {
            scroll: ScrollHandle::new(),
            state: Rc::new(RefCell::new(None)),
            index: 0,
            key: String::new(),
            vertical: false,
        })
    }
}
type State = Rc<RefCell<Option<(gpui::Size<Pixels>, usize, usize, String)>>>;
struct Strip {
    scroll: ScrollHandle,
    state: State,
    index: usize,
    key: String,
    vertical: bool,
}
impl CustomElement for Strip {
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        _: &mut Window,
        _: &mut gpui::Context<crate::renderer::GpuixView>,
    ) -> AnyElement {
        let base = div()
            .id(gpui::SharedString::from(format!("item-strip-{}", ctx.id)))
            .flex()
            .track_scroll(&self.scroll);
        let base = if self.vertical {
            base.flex_col().overflow_y_scroll().overflow_x_hidden()
        } else {
            base.flex_row()
                .items_center()
                .overflow_x_scroll()
                .overflow_y_hidden()
        };
        let mut root = super::custom_surface(base, &ctx);
        let count = ctx.children.len();
        for child in ctx.children {
            root = root.child(child);
        }
        Reveal {
            inner: root.into_any_element(),
            scroll: self.scroll.clone(),
            state: self.state.clone(),
            index: self.index,
            key: self.key.clone(),
            count,
        }
        .into_any_element()
    }
    fn set_prop(&mut self, key: &str, value: Value) {
        match key {
            "selectedIndex" => self.index = value.as_u64().unwrap_or(0) as usize,
            "selectedKey" => self.key = value.as_str().unwrap_or("").into(),
            "axis" => self.vertical = value.as_str() == Some("vertical"),
            _ => {}
        }
    }
    fn supported_props(&self) -> &'static [&'static str] {
        &["selectedIndex", "selectedKey", "axis"]
    }
    fn supported_events(&self) -> &'static [&'static str] {
        &[]
    }
    fn destroy(&mut self) {}
}
struct Reveal {
    inner: AnyElement,
    scroll: ScrollHandle,
    state: State,
    index: usize,
    key: String,
    count: usize,
}
impl IntoElement for Reveal {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for Reveal {
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
        (self.inner.request_layout(window, cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let next = (bounds.size, self.index, self.count, self.key.clone());
        if self.state.borrow().as_ref() != Some(&next) {
            if self.count > 0 {
                self.scroll.scroll_to_item(self.index.min(self.count - 1));
            }
            *self.state.borrow_mut() = Some(next);
        }
        self.inner.prepaint(window, cx);
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.inner.paint(window, cx);
    }
}
