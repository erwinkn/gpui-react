//! Native same-frame workflow layout and connector painting.
//! React owns graph data. GPUI measures cards and resolves connectors after prepaint.
use super::{CustomElement, CustomElementFactory, CustomRenderContext};
use gpui::{
    canvas, div, point, prelude::*, px, size, AnyElement, App, Bounds, Element, ElementId,
    GlobalElementId, InspectorElementId, LayoutId, PathBuilder, Pixels, Window,
};
use serde_json::Value;
use std::{
    cell::RefCell,
    collections::HashMap,
    rc::{Rc, Weak},
};

#[derive(Default)]
struct Frame {
    canvas: Bounds<Pixels>,
    anchors: HashMap<String, Bounds<Pixels>>,
}
type SharedFrame = Rc<RefCell<Frame>>;
thread_local! { static FRAMES: RefCell<HashMap<String, Weak<RefCell<Frame>>>> = RefCell::default(); }
fn frame(scope: &str) -> SharedFrame {
    FRAMES.with(|frames| {
        let mut frames = frames.borrow_mut();
        frames.retain(|_, value| value.strong_count() > 0);
        if let Some(value) = frames.get(scope).and_then(Weak::upgrade) {
            return value;
        }
        let value = Rc::new(RefCell::new(Frame::default()));
        frames.insert(scope.to_owned(), Rc::downgrade(&value));
        value
    })
}
#[derive(Clone, Copy)]
pub enum FlowKind {
    Canvas,
    Node,
    Anchor,
    Edge,
}
pub struct FlowFactory(pub FlowKind);
impl CustomElementFactory for FlowFactory {
    fn element_type(&self) -> &str {
        match self.0 {
            FlowKind::Canvas => "cherry-flow-canvas",
            FlowKind::Node => "cherry-flow-node",
            FlowKind::Anchor => "cherry-flow-anchor",
            FlowKind::Edge => "cherry-flow-edge",
        }
    }
    fn create(&self, _: u64) -> Box<dyn CustomElement> {
        Box::new(Flow {
            kind: self.0,
            props: HashMap::new(),
            frame: None,
        })
    }
}
struct Flow {
    kind: FlowKind,
    props: HashMap<String, Value>,
    frame: Option<SharedFrame>,
}
impl Flow {
    fn text(&self, key: &str, fallback: &str) -> String {
        self.props
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or(fallback)
            .to_owned()
    }
    fn number(&self, key: &str, fallback: f32) -> f32 {
        self.props
            .get(key)
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite())
            .map(|v| v as f32)
            .unwrap_or(fallback)
    }
}
impl CustomElement for Flow {
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        _: &mut Window,
        _: &mut gpui::Context<crate::renderer::GpuixView>,
    ) -> AnyElement {
        let frame = frame(&self.text("scope", "default"));
        self.frame = Some(frame.clone());
        let mut root = super::custom_surface(
            div()
                .id(gpui::SharedString::from(format!("flow-{}", ctx.id)))
                .flex()
                .flex_col(),
            &ctx,
        );
        match self.kind {
            FlowKind::Canvas => {
                let measured = frame.clone();
                let color = crate::color::parse_color_rgba(&self.text("lineColor", "#3a3c40"))
                    .unwrap_or(gpui::rgb(0x3a3c40));
                let dot = self.number("gridSize", 22.0).max(8.0);
                root = root.child(
                    canvas(
                        move |bounds, _, _| {
                            let mut f = measured.borrow_mut();
                            f.canvas = bounds;
                            f.anchors.clear();
                        },
                        move |bounds, _, window, _| {
                            let width = f32::from(bounds.size.width);
                            let height = f32::from(bounds.size.height);
                            let mut y = height % dot / 2.0;
                            while y < height {
                                let mut x = width % dot / 2.0;
                                while x < width {
                                    window.paint_quad(
                                        gpui::fill(
                                            Bounds::new(
                                                bounds.origin + point(px(x), px(y)),
                                                size(px(1.5), px(1.5)),
                                            ),
                                            color,
                                        )
                                        .corner_radii(px(0.75)),
                                    );
                                    x += dot;
                                }
                                y += dot;
                            }
                        },
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                );
            }
            FlowKind::Anchor => {
                let id = self.text("node", "");
                root = root.child(
                    canvas(
                        move |bounds, _, _| {
                            frame.borrow_mut().anchors.insert(id.clone(), bounds);
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                );
            }
            FlowKind::Edge => {
                let from = self.text("from", "");
                let to = self.text("to", "");
                let color = crate::color::parse_color_rgba(&self.text("lineColor", "#3a3c40"))
                    .unwrap_or(gpui::rgb(0x3a3c40));
                let width = self.number("strokeWidth", 1.25).max(0.1);
                root = root.child(
                    canvas(
                        |_, _, _| (),
                        move |_, _, window, _| {
                            let frame = frame.borrow();
                            if let (Some(a), Some(b)) =
                                (frame.anchors.get(&from), frame.anchors.get(&to))
                            {
                                let start = point(a.center().x, a.bottom());
                                let end = point(b.center().x, b.top());
                                let k = (f32::from(end.y - start.y).abs() * 0.55).clamp(24.0, 84.0);
                                let mut path = PathBuilder::stroke(px(width));
                                path.move_to(start);
                                path.cubic_bezier_to(
                                    end,
                                    start + point(px(0.0), px(k)),
                                    end - point(px(0.0), px(k)),
                                );
                                if let Ok(path) = path.build() {
                                    window.paint_path(path, color);
                                }
                            }
                        },
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                );
            }
            FlowKind::Node => {}
        }
        for child in ctx.children {
            root = root.child(child);
        }
        let inner = root.into_any_element();
        if matches!(self.kind, FlowKind::Node) {
            NodeFrame {
                inner,
                frame: self.frame.as_ref().unwrap().clone(),
                x: self.number("x", 0.5).clamp(0.0, 1.0),
                dx: self.number("dx", 0.0),
                dy: self.number("dy", 0.0),
                inset: self.number("inset", 8.0).max(0.0),
            }
            .into_any_element()
        } else {
            inner
        }
    }
    fn set_prop(&mut self, key: &str, value: Value) {
        if value.is_null() {
            self.props.remove(key);
        } else {
            self.props.insert(key.to_owned(), value);
        }
    }
    fn supported_props(&self) -> &'static [&'static str] {
        &[
            "scope",
            "node",
            "from",
            "to",
            "strokeWidth",
            "lineColor",
            "gridSize",
            "x",
            "dx",
            "dy",
            "inset",
        ]
    }
    fn supported_events(&self) -> &'static [&'static str] {
        &["click", "mouseEnter", "mouseLeave"]
    }
    fn destroy(&mut self) {
        self.frame = None;
    }
}
struct NodeFrame {
    inner: AnyElement,
    frame: SharedFrame,
    x: f32,
    dx: f32,
    dy: f32,
    inset: f32,
}
impl IntoElement for NodeFrame {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for NodeFrame {
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
        let c = self.frame.borrow().canvas;
        let left = f32::from(c.left()) + self.inset;
        let right = (f32::from(c.right() - bounds.size.width) - self.inset).max(left);
        let top = f32::from(c.top()) + self.inset;
        let bottom = (f32::from(c.bottom() - bounds.size.height) - self.inset).max(top);
        let x = (f32::from(c.left()) + f32::from(c.size.width) * self.x
            - f32::from(bounds.size.width) / 2.0
            + self.dx)
            .clamp(left, right);
        let y = (f32::from(bounds.top()) + self.dy).clamp(top, bottom);
        let offset = point(px(x) - bounds.left(), px(y) - bounds.top());
        window.with_element_offset(offset, |window| {
            self.inner.prepaint(window, cx);
        });
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
