//! Media-frame input coordinates and a remote cursor, resolved in native layout.
use super::{CustomElement, CustomElementFactory, CustomRenderContext};
use gpui::{canvas, point, prelude::*, px, Bounds, PathBuilder, Pixels};
use serde_json::{json, Value};
use std::{cell::RefCell, collections::HashMap, rc::Rc};
pub struct ScreenFactory;
impl CustomElementFactory for ScreenFactory {
    fn element_type(&self) -> &str {
        "cherry-screen"
    }
    fn create(&self, _: u64) -> Box<dyn CustomElement> {
        Box::new(Screen {
            props: HashMap::new(),
            bounds: Rc::new(RefCell::new(Bounds::default())),
        })
    }
}
struct Screen {
    props: HashMap<String, Value>,
    bounds: Rc<RefCell<Bounds<Pixels>>>,
}
fn emit(
    callback: &Option<crate::renderer::EventCallback>,
    id: u64,
    kind: &str,
    position: gpui::Point<Pixels>,
    bounds: Bounds<Pixels>,
    button: Option<gpui::MouseButton>,
) {
    if bounds.size.width <= px(0.) || bounds.size.height <= px(0.) {
        return;
    }
    let x = (f32::from(position.x - bounds.left()) / f32::from(bounds.size.width)).clamp(0., 1.);
    let y = (f32::from(position.y - bounds.top()) / f32::from(bounds.size.height)).clamp(0., 1.);
    crate::renderer::emit_event_full(callback, id, "change", |p| {
        p.value = Some(
            json!({"kind":kind,"x":x,"y":y,"button":button.map(|b|format!("{:?}",b))}).to_string(),
        );
    });
}
impl CustomElement for Screen {
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        _: &mut gpui::Window,
        _: &mut gpui::Context<crate::renderer::GpuixView>,
    ) -> gpui::AnyElement {
        let mut root = super::custom_surface(
            gpui::div()
                .id(gpui::SharedString::from(format!("screen-{}", ctx.id)))
                .relative()
                .overflow_hidden(),
            &ctx,
        );
        if ctx.events.contains("change") {
            let state = self.bounds.clone();
            let cb = ctx.event_callback.clone();
            let id = ctx.id;
            root = root.on_mouse_move(move |e, _, _| {
                emit(
                    &cb,
                    id,
                    "move",
                    e.position,
                    *state.borrow(),
                    e.pressed_button,
                )
            });
            let state = self.bounds.clone();
            let cb = ctx.event_callback.clone();
            root = root.on_mouse_down(gpui::MouseButton::Left, move |e, _, _| {
                emit(&cb, id, "down", e.position, *state.borrow(), Some(e.button))
            });
            let state = self.bounds.clone();
            let cb = ctx.event_callback.clone();
            root = root.on_mouse_up(gpui::MouseButton::Left, move |e, _, _| {
                emit(&cb, id, "up", e.position, *state.borrow(), Some(e.button))
            });
        }
        let cursor = self
            .props
            .get("cursor")
            .and_then(|p| Some((p.get("x")?.as_f64()? as f32, p.get("y")?.as_f64()? as f32)));
        for child in ctx.children {
            root = root.child(child);
        }
        let state = self.bounds.clone();
        root = root.child(
            canvas(
                move |bounds, _, _| {
                    *state.borrow_mut() = bounds;
                },
                move |bounds, _, window, _| {
                    if let Some((x, y)) = cursor.filter(|(x, y)| x.is_finite() && y.is_finite()) {
                        let origin = point(
                            bounds.left() + bounds.size.width * x.clamp(0., 1.),
                            bounds.top() + bounds.size.height * y.clamp(0., 1.),
                        );
                        let pts = [(0., 0.), (20., 8.), (11., 10.5), (8., 20.), (0., 0.)];
                        for (stroke, color) in
                            [(false, gpui::rgb(0x111318)), (true, gpui::rgb(0xffffff))]
                        {
                            let mut p = if stroke {
                                PathBuilder::stroke(px(1.4))
                            } else {
                                PathBuilder::fill()
                            };
                            for (i, (x, y)) in pts.iter().enumerate() {
                                let point = origin + point(px(*x), px(*y));
                                if i == 0 {
                                    p.move_to(point);
                                } else {
                                    p.line_to(point);
                                }
                            }
                            if let Ok(path) = p.build() {
                                window.paint_path(path, color);
                            }
                        }
                    }
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        );
        root.into_any_element()
    }
    fn set_prop(&mut self, key: &str, value: Value) {
        self.props.insert(key.to_owned(), value);
    }
    fn supported_props(&self) -> &'static [&'static str] {
        &["cursor"]
    }
    fn supported_events(&self) -> &'static [&'static str] {
        &["change"]
    }
    fn destroy(&mut self) {}
}
