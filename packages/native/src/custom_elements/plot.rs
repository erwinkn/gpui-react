//! Native snapshot chart and cursor. Geometry and hover painting remain in GPUI.
use super::{CustomElement, CustomElementFactory, CustomRenderContext};
use gpui::{
    canvas, div, point, prelude::*, px, size, AnyElement, App, Bounds, Element, ElementId,
    GlobalElementId, InspectorElementId, LayoutId, PathBuilder, Pixels, Window,
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    cell::RefCell,
    collections::HashMap,
    rc::{Rc, Weak},
};
#[derive(Clone, Deserialize)]
struct PlotPoint {
    x: f64,
    y: f64,
    label: String,
}
#[derive(Clone, Deserialize)]
struct Series {
    #[serde(default)]
    id: String,
    #[serde(rename = "label")]
    _label: String,
    color: String,
    points: Vec<PlotPoint>,
}
#[derive(Default)]
struct Frame {
    bounds: Bounds<Pixels>,
    cursor: Option<f32>,
    pointer: Option<gpui::Point<Pixels>>,
    series: Rc<Vec<Series>>,
    domain: (f64, f64, f64, f64),
}
type Shared = Rc<RefCell<Frame>>;
thread_local! { static FRAMES:RefCell<HashMap<String,Weak<RefCell<Frame>>>>=RefCell::default(); }
fn frame(scope: &str) -> Shared {
    FRAMES.with(|frames| {
        let mut f = frames.borrow_mut();
        f.retain(|_, v| v.strong_count() > 0);
        if let Some(v) = f.get(scope).and_then(Weak::upgrade) {
            return v;
        }
        let v = Rc::new(RefCell::new(Frame::default()));
        f.insert(scope.to_owned(), Rc::downgrade(&v));
        v
    })
}
fn nearest(series: &Series, x: f64) -> Option<usize> {
    let i = series.points.partition_point(|p| p.x < x);
    if series.points.is_empty() {
        None
    } else if i == 0 {
        Some(0)
    } else if i == series.points.len() {
        Some(i - 1)
    } else if (series.points[i - 1].x - x).abs() <= (series.points[i].x - x).abs() {
        Some(i - 1)
    } else {
        Some(i)
    }
}
fn prepare(value: &Value) -> (Rc<Vec<Series>>, (f64, f64, f64, f64)) {
    let mut series: Vec<Series> = serde_json::from_value(value.clone()).unwrap_or_default();
    let mut domain = (
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    );
    for s in &mut series {
        s.points.retain(|p| p.x.is_finite() && p.y.is_finite());
        s.points.sort_by(|a, b| a.x.total_cmp(&b.x));
        for p in &s.points {
            domain.0 = domain.0.min(p.x);
            domain.1 = domain.1.max(p.x);
            domain.2 = domain.2.min(p.y);
            domain.3 = domain.3.max(p.y);
        }
    }
    if !domain.0.is_finite() {
        domain = (0.0, 1.0, 0.0, 1.0);
    }
    if domain.0 == domain.1 {
        domain.0 -= 0.5;
        domain.1 += 0.5;
    }
    if domain.2 == domain.3 {
        let d = domain.2.abs().max(1.0) * 0.1;
        domain.2 -= d;
        domain.3 += d;
    }
    (Rc::new(series), domain)
}

pub struct PlotFactory;
pub struct TooltipFactory;
impl CustomElementFactory for PlotFactory {
    fn element_type(&self) -> &str {
        "cherry-plot"
    }
    fn create(&self, _: u64) -> Box<dyn CustomElement> {
        Box::new(Plot {
            tooltip: false,
            props: HashMap::new(),
            frame: None,
            series: Rc::default(),
            domain: (0.0, 1.0, 0.0, 1.0),
            revision: 0,
            seen_revision: 0,
            transition_key: String::new(),
            displayed: None,
            animation: None,
        })
    }
}
impl CustomElementFactory for TooltipFactory {
    fn element_type(&self) -> &str {
        "cherry-plot-tooltip"
    }
    fn create(&self, _: u64) -> Box<dyn CustomElement> {
        Box::new(Plot {
            tooltip: true,
            props: HashMap::new(),
            frame: None,
            series: Rc::default(),
            domain: (0.0, 1.0, 0.0, 1.0),
            revision: 0,
            seen_revision: 0,
            transition_key: String::new(),
            displayed: None,
            animation: None,
        })
    }
}
struct Plot {
    tooltip: bool,
    props: HashMap<String, Value>,
    frame: Option<Shared>,
    series: Rc<Vec<Series>>,
    domain: (f64, f64, f64, f64),
    revision: u64,
    seen_revision: u64,
    transition_key: String,
    displayed: Option<Snapshot>,
    animation: Option<(web_time::Instant, Snapshot)>,
}
#[derive(Clone)]
struct Snapshot {
    series: Rc<Vec<Series>>,
    domain: (f64, f64, f64, f64),
}
fn interpolate_value(points: &[PlotPoint], x: f64) -> f64 {
    let i = points.partition_point(|p| p.x < x);
    if i == 0 {
        return points.first().map(|p| p.y).unwrap_or(0.);
    }
    if i == points.len() {
        return points[i - 1].y;
    }
    let a = &points[i - 1];
    let b = &points[i];
    let t = ((x - a.x) / (b.x - a.x).max(f64::EPSILON)).clamp(0., 1.);
    a.y + (b.y - a.y) * t
}
impl Plot {
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
impl CustomElement for Plot {
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        window: &mut Window,
        _: &mut gpui::Context<crate::renderer::GpuixView>,
    ) -> AnyElement {
        let state = frame(&self.text("scope", "default"));
        if self.frame.is_none() && !self.tooltip {
            state.borrow_mut().cursor = self
                .props
                .get("cursor")
                .and_then(Value::as_f64)
                .map(|v| (v as f32).clamp(0.0, 1.0));
        }
        self.frame = Some(state.clone());
        let mut root = super::custom_surface(
            div().id(gpui::SharedString::from(format!("plot-{}", ctx.id))),
            &ctx,
        );
        let mut domain = self.domain;
        if let Some(values) = self.props.get("domain").and_then(Value::as_array) {
            let nums: Vec<_> = values
                .iter()
                .filter_map(Value::as_f64)
                .filter(|n| n.is_finite())
                .collect();
            if nums.len() == 4 && nums[1] > nums[0] && nums[3] > nums[2] {
                domain = (nums[0], nums[1], nums[2], nums[3]);
            }
        }
        if self.tooltip {
            let f = state.borrow();
            let Some(cursor) = f.cursor else {
                return gpui::Empty.into_any_element();
            };
            let current = self.series.clone();
            let x = domain.0 + (domain.1 - domain.0) * f64::from(cursor);
            root = root.flex().gap(px(8.0));
            for series in current.iter() {
                if let Some(i) = nearest(series, x) {
                    let color = crate::color::parse_color_rgba(&series.color)
                        .unwrap_or(gpui::rgb(0xaaaaaa));
                    root = root.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .child(div().size(px(5.0)).rounded_full().bg(color))
                            .child(ctx.chrome_text(series.points[i].label.clone(), None)),
                    );
                }
            }
            drop(f);
            return TooltipFrame {
                inner: root.into_any_element(),
                state,
            }
            .into_any_element();
        }
        let key = self.text("transitionKey", "");
        let duration = self.number("transitionMs", 0.).clamp(0., 2000.);
        let inspecting = state.borrow().cursor.is_some();
        if self.seen_revision != self.revision
            || self.displayed.as_ref().map(|d| d.domain) != Some(domain) && self.animation.is_none()
            || self.transition_key != key
        {
            self.animation = if duration > 0.
                && !inspecting
                && self.transition_key == key
                && self.displayed.as_ref().is_some_and(|d| {
                    d.series.len() == self.series.len()
                        && d.series
                            .iter()
                            .zip(self.series.iter())
                            .all(|(a, b)| a.id == b.id)
                }) {
                self.displayed.clone().map(|d| (ctx.now, d))
            } else {
                None
            };
            self.seen_revision = self.revision;
            self.transition_key = key;
        }
        let mut series = self.series.clone();
        if let Some((started, from)) = &self.animation {
            let raw = if duration > 0. && !inspecting {
                (ctx.now.saturating_duration_since(*started).as_secs_f32() * 1000. / duration)
                    .clamp(0., 1.)
            } else {
                1.
            };
            if raw < 1. {
                let t = f64::from(1. - (1. - raw).powi(3));
                let mix = |a: f64, b: f64| a + (b - a) * t;
                domain = (
                    mix(from.domain.0, domain.0),
                    mix(from.domain.1, domain.1),
                    domain.2,
                    domain.3,
                );
                series = Rc::new(
                    self.series
                        .iter()
                        .map(|target| {
                            let old = from.series.iter().find(|s| s.id == target.id);
                            let mut s = target.clone();
                            if let Some(old) = old.filter(|s| !s.points.is_empty()) {
                                for p in &mut s.points {
                                    p.y = mix(interpolate_value(&old.points, p.x), p.y);
                                }
                            }
                            s
                        })
                        .collect(),
                );
                window.request_animation_frame();
            } else {
                self.animation = None;
            }
        }
        self.displayed = Some(Snapshot {
            series: series.clone(),
            domain,
        });
        {
            let mut f = state.borrow_mut();
            f.domain = domain;
            f.series = self.series.clone();
        }
        let filled = self.props.get("fill").and_then(Value::as_bool) == Some(true);
        let endpoint = self.props.get("endpoint").and_then(Value::as_bool) == Some(true);
        let pulse = self.props.get("pulse").and_then(Value::as_bool) == Some(true)
            && !inspecting
            && series.iter().any(|s| !s.points.is_empty());
        // Use the native clock; no JS heartbeat is required for the endpoint ring.
        thread_local! { static EPOCH:web_time::Instant=web_time::Instant::now(); }
        let phase = EPOCH
            .with(|epoch| ctx.now.saturating_duration_since(*epoch).as_secs_f32())
            .fract();
        if pulse {
            window.request_animation_frame();
        }
        let measured = state.clone();
        let painted = state.clone();
        let line = crate::color::parse_color_rgba(&self.text("gridColor", "#3a3c40"))
            .unwrap_or(gpui::rgb(0x3a3c40));
        let grid = self
            .props
            .get("grid")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let smooth = self
            .props
            .get("smooth")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let stroke = self.number("lineWidth", 2.25).max(0.5);
        let top = self.number("paddingTop", 34.0).max(0.0);
        let bottom = self.number("paddingBottom", 22.0).max(0.0);
        let left = self.number("paddingLeft", 0.).max(0.);
        let right = self.number("paddingRight", 0.).max(0.);
        let rows = self.number("gridCount", 4.).clamp(2., 20.) as usize;
        root = root.child(
            canvas(
                move |bounds, _, _| {
                    measured.borrow_mut().bounds = Bounds::new(
                        bounds.origin + point(px(left), px(0.)),
                        size(
                            (bounds.size.width - px(left + right)).max(px(1.)),
                            bounds.size.height,
                        ),
                    );
                },
                move |bounds, _, window, _| {
                    window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
                        let bounds = Bounds::new(
                            bounds.origin + point(px(left), px(0.)),
                            size(
                                (bounds.size.width - px(left + right)).max(px(1.)),
                                bounds.size.height,
                            ),
                        );
                        let f = painted.borrow();
                        let width = f32::from(bounds.size.width).max(1.0);
                        let height = (f32::from(bounds.size.height) - top - bottom).max(1.0);
                        let position = |p: &PlotPoint| {
                            point(
                                bounds.left()
                                    + px(((p.x - domain.0) / (domain.1 - domain.0)) as f32 * width),
                                bounds.top()
                                    + px(top
                                        + (1.0 - (p.y - domain.2) / (domain.3 - domain.2)) as f32
                                            * height),
                            )
                        };
                        if grid {
                            for row in 0..rows {
                                let y = bounds.top()
                                    + px(top + height * row as f32 / (rows - 1) as f32);
                                let mut p = PathBuilder::stroke(px(0.5));
                                p.move_to(point(bounds.left(), y));
                                p.line_to(point(bounds.right(), y));
                                if let Ok(p) = p.build() {
                                    window.paint_path(p, line);
                                }
                            }
                        }
                        for s in series.iter() {
                            let color = crate::color::parse_color_rgba(&s.color)
                                .unwrap_or(gpui::rgb(0x3d9aff));
                            let pts: Vec<_> = s.points.iter().map(position).collect();
                            if let Some(first) = pts.first() {
                                if filled && pts.len() > 1 {
                                    let baseline = bounds.bottom() - px(bottom);
                                    let mut area = PathBuilder::fill();
                                    area.move_to(point(first.x, baseline));
                                    area.line_to(*first);
                                    for p in pts.iter().skip(1) {
                                        area.line_to(*p);
                                    }
                                    area.line_to(point(pts.last().unwrap().x, baseline));
                                    area.close();
                                    if let Ok(path) = area.build() {
                                        window.paint_path(
                                            path,
                                            gpui::linear_gradient(
                                                180.,
                                                gpui::linear_color_stop(color.opacity(0.16), 0.),
                                                gpui::linear_color_stop(color.opacity(0.), 1.),
                                            ),
                                        );
                                    }
                                }
                                if endpoint {
                                    let p = *pts.last().unwrap();
                                    if pulse {
                                        let radius = 3. + phase * 7.;
                                        window.paint_quad(
                                            gpui::fill(
                                                Bounds::new(
                                                    p - point(px(radius), px(radius)),
                                                    size(px(radius * 2.), px(radius * 2.)),
                                                ),
                                                color.opacity((1. - phase) * 0.2),
                                            )
                                            .corner_radii(px(radius)),
                                        );
                                    }
                                    window.paint_quad(
                                        gpui::fill(
                                            Bounds::new(
                                                p - point(px(3.), px(3.)),
                                                size(px(6.), px(6.)),
                                            ),
                                            color,
                                        )
                                        .corner_radii(px(3.)),
                                    );
                                }
                                let mut p = PathBuilder::stroke(px(stroke));
                                p.move_to(*first);
                                for i in 1..pts.len() {
                                    if smooth {
                                        let a = pts[i - 1];
                                        let b = pts[i];
                                        let prev = pts[i.saturating_sub(2)];
                                        let next = pts[(i + 1).min(pts.len() - 1)];
                                        p.cubic_bezier_to(
                                            b,
                                            a + (b - prev) / 6.0,
                                            b - (next - a) / 6.0,
                                        );
                                    } else {
                                        p.line_to(pts[i]);
                                    }
                                }
                                if pts.len() > 1 {
                                    if let Ok(p) = p.build() {
                                        window.paint_path(p, color);
                                    }
                                } else {
                                    window.paint_quad(
                                        gpui::fill(
                                            Bounds::new(
                                                *first - point(px(2.0), px(2.0)),
                                                size(px(4.0), px(4.0)),
                                            ),
                                            color,
                                        )
                                        .corner_radii(px(2.0)),
                                    );
                                }
                                if let Some(cursor) = f.cursor {
                                    let x = domain.0 + (domain.1 - domain.0) * f64::from(cursor);
                                    if let Some(i) = nearest(s, x) {
                                        let p = position(&s.points[i]);
                                        window.paint_quad(
                                            gpui::fill(
                                                Bounds::new(
                                                    p - point(px(3.0), px(3.0)),
                                                    size(px(6.0), px(6.0)),
                                                ),
                                                color,
                                            )
                                            .corner_radii(px(3.0)),
                                        );
                                    }
                                }
                            }
                        }
                        if let Some(cursor) = f.cursor {
                            let x = bounds.left() + px(cursor * width);
                            let mut p = PathBuilder::stroke(px(1.0));
                            p.move_to(point(x, bounds.top() + px(top)));
                            p.line_to(point(x, bounds.bottom() - px(bottom)));
                            if let Ok(p) = p.build() {
                                window.paint_path(p, line);
                            }
                        }
                    });
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        );
        let moving = state.clone();
        let callback = ctx.event_callback.clone();
        let id = ctx.id;
        root = root.on_mouse_move(move |event, window, _| {
            let mut f = moving.borrow_mut();
            if f.pointer == Some(event.position) {
                return;
            }
            f.pointer = Some(event.position);
            let old = f.cursor;
            let ratio = (f32::from(event.position.x - f.bounds.left())
                / f32::from(f.bounds.size.width).max(1.0))
            .clamp(0.0, 1.0);
            let indices = |r: f32| {
                f.series
                    .iter()
                    .map(|s| nearest(s, f.domain.0 + (f.domain.1 - f.domain.0) * f64::from(r)))
                    .collect::<Vec<_>>()
            };
            let changed = old.map(indices) != Some(indices(ratio));
            f.cursor = Some(ratio);
            drop(f);
            window.refresh();
            if changed {
                crate::renderer::emit_event_full(&callback, id, "change", |p| {
                    p.value = Some(ratio.to_string())
                });
            }
        });
        let leaving = state.clone();
        let callback = ctx.event_callback.clone();
        root = root.on_hover(move |hovered, window, _| {
            if !hovered {
                let mut frame = leaving.borrow_mut();
                if window.last_input_was_keyboard() {
                    return;
                }
                frame.cursor = None;
                frame.pointer = None;
                drop(frame);
                window.refresh();
                crate::renderer::emit_event_full(&callback, id, "change", |p| {
                    p.value = Some(String::new())
                });
            }
        });
        for child in ctx.children {
            root = root.child(child);
        }
        root.into_any_element()
    }
    fn set_prop(&mut self, key: &str, value: Value) {
        if key == "series" {
            (self.series, self.domain) = prepare(&value);
            self.revision = self.revision.wrapping_add(1);
        }
        if key == "cursor" {
            if let Some(state) = &self.frame {
                state.borrow_mut().cursor = value
                    .as_f64()
                    .filter(|v| v.is_finite())
                    .map(|v| (v as f32).clamp(0.0, 1.0));
            }
        }
        if value.is_null() {
            self.props.remove(key);
        } else {
            self.props.insert(key.to_owned(), value);
        }
    }
    fn supported_props(&self) -> &'static [&'static str] {
        &[
            "scope",
            "series",
            "cursor",
            "grid",
            "smooth",
            "gridColor",
            "lineWidth",
            "paddingTop",
            "paddingBottom",
            "paddingLeft",
            "paddingRight",
            "gridCount",
            "domain",
            "fill",
            "endpoint",
            "pulse",
            "transitionMs",
            "transitionKey",
        ]
    }
    fn supported_events(&self) -> &'static [&'static str] {
        &["change"]
    }
    fn destroy(&mut self) {
        self.frame = None;
    }
}
struct TooltipFrame {
    inner: AnyElement,
    state: Shared,
}
impl IntoElement for TooltipFrame {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for TooltipFrame {
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
        let f = self.state.borrow();
        let c = f.bounds;
        let cursor = f.cursor.unwrap_or(0.5);
        let left = f32::from(c.left()) + 4.0;
        let right = (f32::from(c.right() - bounds.size.width) - 4.0).max(left);
        let x = (f32::from(c.left()) + cursor * f32::from(c.size.width)
            - f32::from(bounds.size.width) / 2.0)
            .clamp(left, right);
        let offset = point(px(x) - bounds.left(), c.top() + px(6.0) - bounds.top());
        drop(f);
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
