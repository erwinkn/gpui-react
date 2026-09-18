//! Current-frame target geometry and native, interruptible selection motion.
//! React owns items and focus. Moving a pointer never changes the retained tree.
use super::{CustomElement, CustomElementFactory, CustomRenderContext};
use gpui::{
    canvas, div, point, prelude::*, px, size, AnyElement, App, Bounds, Element, ElementId,
    GlobalElementId, InspectorElementId, LayoutId, Pixels, Point, Window,
};
use serde_json::Value;
use std::{
    cell::RefCell,
    collections::HashMap,
    rc::{Rc, Weak},
};

#[derive(Clone, Copy, Default, PartialEq, Debug)]
struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    alpha: f32,
}
impl Rect {
    fn mix(self, b: Self, t: f32) -> Self {
        Self {
            x: self.x + (b.x - self.x) * t,
            y: self.y + (b.y - self.y) * t,
            w: self.w + (b.w - self.w) * t,
            h: self.h + (b.h - self.h) * t,
            alpha: self.alpha + (b.alpha - self.alpha) * t,
        }
    }
}
#[derive(Default)]
struct Frame {
    bounds: Bounds<Pixels>,
    targets: HashMap<String, (Bounds<Pixels>, bool)>,
    pointer: Option<Point<Pixels>>,
    key: String,
    from: Rect,
    to: Rect,
    started: Option<web_time::Instant>,
}
type Shared = Rc<RefCell<Frame>>;
thread_local! { static FRAMES: RefCell<HashMap<String, Weak<RefCell<Frame>>>> = RefCell::default(); }
fn frame(scope: &str) -> Shared {
    FRAMES.with(|all| {
        let mut all = all.borrow_mut();
        all.retain(|_, f| f.strong_count() > 0);
        if let Some(f) = all.get(scope).and_then(Weak::upgrade) {
            return f;
        }
        let f = Rc::new(RefCell::new(Frame::default()));
        all.insert(scope.into(), Rc::downgrade(&f));
        f
    })
}
// Solve the actual cubic-bezier x coordinate. A y-only approximation changes the feel.
pub(crate) fn ease(t: f32, c: [f32; 4]) -> f32 {
    fn axis(t: f32, a: f32, b: f32) -> f32 {
        3.0 * (1.0 - t).powi(2) * t * a + 3.0 * (1.0 - t) * t * t * b + t * t * t
    }
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..14 {
        let mid = (lo + hi) / 2.0;
        if axis(mid, c[0], c[2]) < t {
            lo = mid
        } else {
            hi = mid
        }
    }
    axis((lo + hi) / 2.0, c[1], c[3])
}
impl Frame {
    fn sample(&self, now: web_time::Instant, duration: f32, curve: [f32; 4]) -> (Rect, bool) {
        let p = self
            .started
            .map(|s| now.saturating_duration_since(s).as_secs_f32() * 1000.0 / duration.max(1.0))
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);
        (
            self.from.mix(self.to, ease(p, curve)),
            p < 1.0 && self.from != self.to,
        )
    }
}
pub struct GlideFactory(pub bool);
impl CustomElementFactory for GlideFactory {
    fn element_type(&self) -> &str {
        if self.0 {
            "cherry-glide-target"
        } else {
            "cherry-glide"
        }
    }
    fn create(&self, _: u64) -> Box<dyn CustomElement> {
        Box::new(Glide {
            target: self.0,
            props: HashMap::new(),
            frame: None,
        })
    }
}
struct Glide {
    target: bool,
    props: HashMap<String, Value>,
    frame: Option<Shared>,
}
impl Glide {
    fn text(&self, key: &str, fallback: &str) -> String {
        self.props
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or(fallback)
            .into()
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
impl CustomElement for Glide {
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        window: &mut Window,
        _: &mut gpui::Context<crate::renderer::GpuixView>,
    ) -> AnyElement {
        let shared = frame(&self.text("scope", "default"));
        self.frame = Some(shared.clone());
        let mut root = super::custom_surface(
            div()
                .id(gpui::SharedString::from(format!("glide-{}", ctx.id)))
                .flex()
                .flex_col(),
            &ctx,
        );
        if !self.target {
            let hover = self
                .props
                .get("hover")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let keyboard = window.last_input_was_keyboard();
            let snap = keyboard
                || self
                    .props
                    .get("reducedMotion")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
            let key = self.text("activeKey", "");
            let now = ctx.now;
            let duration = self.number("durationMs", 220.0).max(0.0);
            let radius = self.number("radius", 8.0).max(0.0);
            let border = self.number("borderWidth", 0.0).max(0.0);
            let color = crate::color::parse_color_rgba(&self.text("color", "#2a2b2e"))
                .unwrap_or(gpui::rgb(0x2a2b2e));
            let line = crate::color::parse_color_rgba(&self.text("borderColor", "#00000000"))
                .unwrap_or(gpui::rgba(0));
            let shadows = self
                .props
                .get("boxShadow")
                .and_then(|v| {
                    serde_json::from_value::<crate::style::BoxShadowStack>(v.clone()).ok()
                })
                .map(|stack| {
                    stack
                        .shadows()
                        .iter()
                        .rev()
                        .filter_map(|shadow| {
                            let color = crate::color::parse_color_rgba(&shadow.color)?;
                            let value = gpui::BoxShadow::new(
                                px(shadow.offset_x as f32),
                                px(shadow.offset_y as f32),
                                color.into(),
                            )
                            .blur_radius(px(shadow.blur_radius.max(0.0) as f32))
                            .spread_radius(px(shadow.spread_radius as f32));
                            Some(if shadow.inset { value.inset() } else { value })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let curve = self
                .props
                .get("ease")
                .and_then(Value::as_array)
                .filter(|v| v.len() == 4)
                .and_then(|v| {
                    Some([
                        v[0].as_f64()? as f32,
                        v[1].as_f64()? as f32,
                        v[2].as_f64()? as f32,
                        v[3].as_f64()? as f32,
                    ])
                })
                .unwrap_or([0.23, 1.0, 0.32, 1.0]);
            let before = shared.clone();
            let paint = shared.clone();
            root = root.child(
                canvas(
                    move |bounds, _, _| {
                        let mut f = before.borrow_mut();
                        f.bounds = bounds;
                        f.targets.clear();
                    },
                    move |bounds, _, window, _| {
                        let mut f = paint.borrow_mut();
                        let target = if hover && !keyboard {
                            f.pointer.and_then(|p| {
                                f.targets
                                    .iter()
                                    .find(|(_, (b, enabled))| *enabled && b.contains(&p))
                                    .map(|(key, (b, _))| (key.clone(), *b))
                            })
                        } else {
                            f.targets
                                .get(&key)
                                .filter(|(_, enabled)| *enabled)
                                .map(|(b, _)| (key.clone(), *b))
                        };
                        let next_key = target
                            .as_ref()
                            .map(|(key, _)| key.clone())
                            .unwrap_or_default();
                        let same_key = next_key == f.key;
                        f.key = next_key;
                        let mut desired = target
                            .map(|(_, b)| Rect {
                                x: f32::from(b.left() - bounds.left()),
                                y: f32::from(b.top() - bounds.top()),
                                w: f32::from(b.size.width),
                                h: f32::from(b.size.height),
                                alpha: 1.0,
                            })
                            .unwrap_or(Rect { alpha: 0.0, ..f.to });
                        // A layout change must not animate a control away from its actual row.
                        // Pointer/selection changes animate; resize/font/scroll corrections snap.
                        if same_key && desired.alpha > 0.0 && f.to.alpha > 0.0 && desired != f.to {
                            f.from = desired;
                            f.to = desired;
                            f.started = None;
                        }
                        if !hover && f.started.is_none() && f.to.alpha == 0.0 {
                            f.from = desired;
                            f.to = desired;
                        }
                        if desired != f.to {
                            let (current, _) = f.sample(now, duration, curve);
                            if f.to.alpha == 0.0 && desired.alpha > 0.0 {
                                f.from = Rect {
                                    alpha: current.alpha,
                                    ..desired
                                };
                            } else {
                                f.from = current;
                            }
                            if desired.alpha == 0.0 {
                                desired = Rect {
                                    alpha: 0.0,
                                    ..current
                                };
                            }
                            f.to = desired;
                            f.started = Some(now);
                        }
                        let (current, animating) = if snap || duration == 0.0 {
                            f.from = f.to;
                            (f.to, false)
                        } else {
                            f.sample(now, duration, curve)
                        };
                        if animating {
                            window.request_animation_frame();
                        }
                        if current.alpha > 0.0 && current.w > 0.0 && current.h > 0.0 {
                            let mut fill = color;
                            fill.a *= current.alpha;
                            let mut stroke = line;
                            stroke.a *= current.alpha;
                            let target_bounds = Bounds::new(
                                bounds.origin + point(px(current.x), px(current.y)),
                                size(px(current.w), px(current.h)),
                            );
                            let corners = gpui::Corners::all(px(radius
                                .min(current.w / 2.0)
                                .min(current.h / 2.0)));
                            let shadow_layers = shadows
                                .iter()
                                .map(|shadow| {
                                    let mut layer = shadow.clone();
                                    layer.color.a *= current.alpha;
                                    layer
                                })
                                .collect::<Vec<_>>();
                            window.paint_drop_shadows(target_bounds, corners, &shadow_layers);
                            window.paint_quad(gpui::quad(
                                target_bounds,
                                corners,
                                fill,
                                px(border),
                                stroke,
                                gpui::BorderStyle::Solid,
                            ));
                            window.paint_inset_shadows(target_bounds, corners, &shadow_layers);
                        }
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            );
            if hover {
                let moved = shared.clone();
                let left = shared.clone();
                root = root
                    .on_mouse_move(move |e, window, _| {
                        let mut f = moved.borrow_mut();
                        if f.pointer != Some(e.position) {
                            f.pointer = Some(e.position);
                            window.refresh();
                        }
                    })
                    .on_hover(move |inside, window, _| {
                        if !inside {
                            left.borrow_mut().pointer = None;
                            window.refresh();
                        }
                    });
            }
        }
        for child in ctx.children {
            root = root.child(child);
        }
        let inner = root.into_any_element();
        if self.target {
            Target {
                inner,
                shared,
                key: self.text("targetKey", ""),
                enabled: !self
                    .props
                    .get("disabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
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
            self.props.insert(key.into(), value);
        }
    }
    fn supported_props(&self) -> &'static [&'static str] {
        &[
            "scope",
            "targetKey",
            "activeKey",
            "hover",
            "disabled",
            "color",
            "radius",
            "borderWidth",
            "borderColor",
            "boxShadow",
            "durationMs",
            "ease",
            "reducedMotion",
        ]
    }
    fn supported_events(&self) -> &'static [&'static str] {
        &["click", "mouseEnter", "mouseLeave"]
    }
    fn destroy(&mut self) {
        self.frame = None;
    }
}
struct Target {
    inner: AnyElement,
    shared: Shared,
    key: String,
    enabled: bool,
}
impl IntoElement for Target {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for Target {
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
        self.shared
            .borrow_mut()
            .targets
            .insert(self.key.clone(), (bounds, self.enabled));
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
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn easing_is_monotonic_and_has_exact_endpoints() {
        let c = [0.23, 1.0, 0.32, 1.0];
        assert_eq!(ease(0.0, c), 0.0);
        assert_eq!(ease(1.0, c), 1.0);
        for i in 1..100 {
            assert!(ease(i as f32 / 100.0, c) >= ease((i - 1) as f32 / 100.0, c));
        }
    }
}
