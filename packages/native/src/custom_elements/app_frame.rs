//! Pane adaptation uses GPUI's container query and the actual assigned size.
//! No React resize round-trip or hidden hitboxes are needed.
use super::{CustomElement, CustomElementFactory, CustomRenderContext};
use gpui::{container_query, div, prelude::*, px, AnyElement};
use serde_json::Value;
use std::collections::HashMap;
pub struct AppFrameFactory;
impl CustomElementFactory for AppFrameFactory {
    fn element_type(&self) -> &str {
        "cherry-app-frame"
    }
    fn create(&self, _: u64) -> Box<dyn CustomElement> {
        Box::new(AppFrame {
            props: HashMap::new(),
        })
    }
}
struct AppFrame {
    props: HashMap<String, Value>,
}
impl AppFrame {
    fn number(&self, key: &str, fallback: f32) -> f32 {
        self.props
            .get(key)
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite())
            .map(|v| v as f32)
            .unwrap_or(fallback)
            .max(0.0)
    }
    fn flag(&self, key: &str) -> bool {
        self.props
            .get(key)
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }
}
impl CustomElement for AppFrame {
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        _: &mut gpui::Window,
        _: &mut gpui::Context<crate::renderer::GpuixView>,
    ) -> AnyElement {
        let root = super::custom_surface(
            div()
                .id(gpui::SharedString::from(format!("app-frame-{}", ctx.id)))
                .flex()
                .flex_col()
                .size_full()
                .min_w_0()
                .min_h_0(),
            &ctx,
        );
        let sidebar = self.number("sidebarWidth", 224.0);
        let dock = self.number("dockWidth", 360.0);
        let main = self.number("minContentWidth", 320.0);
        let gap = self.number("gap", 10.0);
        let compact_height = self.number("compactHeight", 34.0);
        let has_sidebar = self.flag("hasSidebar");
        let has_dock = self.flag("hasDock");
        let pane = self
            .props
            .get("pane")
            .and_then(Value::as_str)
            .unwrap_or("main")
            .to_owned();
        let mut children: Vec<Option<AnyElement>> = ctx.children.into_iter().map(Some).collect();
        root.child(container_query(move |size, _, _| {
            let width = f32::from(size.width);
            let full = width
                >= main
                    + if has_sidebar { sidebar } else { 0.0 }
                    + if has_dock { dock + gap } else { 0.0 };
            let paired = width >= main + if has_dock { dock + gap } else { 0.0 };
            let show_bar = !full && (has_sidebar || has_dock);
            let side_only = show_bar && pane == "sidebar" && has_sidebar;
            let dock_only = show_bar && !paired && pane == "dock" && has_dock;
            let mut column = div().flex().flex_col().size_full().min_w_0().min_h_0();
            if show_bar {
                if let Some(bar) = children.get_mut(3).and_then(Option::take) {
                    column =
                        column.child(div().flex_none().h(px(compact_height)).w_full().child(bar));
                }
            }
            let mut row = div()
                .flex()
                .flex_row()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .w_full();
            if side_only {
                if let Some(side) = children.get_mut(0).and_then(Option::take) {
                    row = row.child(div().size_full().child(side));
                }
            } else if dock_only {
                if let Some(dock) = children.get_mut(2).and_then(Option::take) {
                    row = row.child(div().size_full().child(dock));
                }
            } else {
                if full && has_sidebar {
                    if let Some(side) = children.get_mut(0).and_then(Option::take) {
                        row = row.child(div().w(px(sidebar)).h_full().flex_none().child(side));
                    }
                }
                if let Some(main) = children.get_mut(1).and_then(Option::take) {
                    row = row.child(div().flex_1().min_w_0().h_full().child(main));
                }
                if has_dock && (full || paired) {
                    if let Some(dock_child) = children.get_mut(2).and_then(Option::take) {
                        row = row.child(
                            div()
                                .ml(px(gap))
                                .w(px(dock))
                                .h_full()
                                .flex_none()
                                .child(dock_child),
                        );
                    }
                }
            }
            column.child(row)
        }))
        .into_any_element()
    }
    fn set_prop(&mut self, key: &str, value: Value) {
        if value.is_null() {
            self.props.remove(key);
        } else {
            self.props.insert(key.into(), value);
        }
    }
    fn supported_props(&self) -> &'static [&'static str] {
        &[
            "sidebarWidth",
            "dockWidth",
            "minContentWidth",
            "gap",
            "compactHeight",
            "hasSidebar",
            "hasDock",
            "pane",
        ]
    }
    fn supported_events(&self) -> &'static [&'static str] {
        &[]
    }
    fn destroy(&mut self) {}
}
