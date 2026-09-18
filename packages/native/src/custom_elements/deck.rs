//! Measure every page in one grid cell; prepaint and paint only the active page.
//! Hidden pages keep their intrinsic layout but create no focus, hitbox, or media paint work.
use super::{CustomElement, CustomElementFactory, CustomRenderContext};
use gpui::{
    prelude::*, AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId,
    LayoutId, Pixels, Window,
};
pub struct DeckFactory;
impl CustomElementFactory for DeckFactory {
    fn element_type(&self) -> &str {
        "cherry-deck"
    }
    fn create(&self, _: u64) -> Box<dyn CustomElement> {
        Box::new(Deck { selected: 0 })
    }
}
struct Deck {
    selected: usize,
}
impl CustomElement for Deck {
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        _: &mut Window,
        _: &mut gpui::Context<crate::renderer::GpuixView>,
    ) -> AnyElement {
        let mut root = super::custom_surface(
            gpui::div()
                .id(gpui::SharedString::from(format!("deck-{}", ctx.id)))
                .grid()
                .grid_cols(1),
            &ctx,
        );
        let selected = self.selected.min(ctx.children.len().saturating_sub(1));
        for (index, child) in ctx.children.into_iter().enumerate() {
            root = root.child(
                gpui::div()
                    .grid()
                    .grid_cols(1)
                    .w_full()
                    .min_w_0()
                    .col_start(1)
                    .row_start(1)
                    .child(Page {
                        inner: child,
                        active: index == selected,
                    }),
            );
        }
        root.into_any_element()
    }
    fn set_prop(&mut self, key: &str, value: serde_json::Value) {
        if key == "selected" {
            self.selected = value.as_u64().unwrap_or(0) as usize;
        }
    }
    fn supported_props(&self) -> &'static [&'static str] {
        &["selected"]
    }
    fn supported_events(&self) -> &'static [&'static str] {
        &[]
    }
    fn destroy(&mut self) {}
}
struct Page {
    inner: AnyElement,
    active: bool,
}
impl IntoElement for Page {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for Page {
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
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.active {
            self.inner.prepaint(window, cx);
        }
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
        if self.active {
            self.inner.paint(window, cx);
        }
    }
}
