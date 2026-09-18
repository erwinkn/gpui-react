//! Small native animated indicators. Animation never updates React or advances work state.
use super::{CustomElement, CustomElementFactory, CustomRenderContext};
use gpui::{canvas, div, point, prelude::*, px, size, Bounds, PathBuilder, Rgba};
use serde_json::Value;
use std::collections::HashMap;
use unicode_segmentation::UnicodeSegmentation;

pub struct IndicatorFactory;
impl CustomElementFactory for IndicatorFactory {
    fn element_type(&self) -> &str {
        "cherry-indicator"
    }
    fn create(&self, _: u64) -> Box<dyn CustomElement> {
        Box::new(Indicator {
            props: HashMap::new(),
            started: None,
            progress_to: None,
            progress_from: 0.0,
            progress_changed: None,
            counter_text: String::new(),
            counter_from: String::new(),
            counter_changed: None,
        })
    }
}
struct Indicator {
    props: HashMap<String, Value>,
    started: Option<web_time::Instant>,
    progress_to: Option<f32>,
    progress_from: f32,
    progress_changed: Option<web_time::Instant>,
    counter_text: String,
    counter_from: String,
    counter_changed: Option<web_time::Instant>,
}
impl Indicator {
    fn number(&self, key: &str, fallback: f32) -> f32 {
        self.props
            .get(key)
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite())
            .map(|v| v as f32)
            .unwrap_or(fallback)
    }
    fn string(&self, key: &str, fallback: &str) -> String {
        self.props
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or(fallback)
            .to_owned()
    }
    fn color(&self, key: &str, fallback: &str) -> Rgba {
        crate::color::parse_color_rgba(&self.string(key, fallback)).unwrap_or(gpui::rgb(0xaaaaaa))
    }
}
impl CustomElement for Indicator {
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        window: &mut gpui::Window,
        _: &mut gpui::Context<crate::renderer::GpuixView>,
    ) -> gpui::AnyElement {
        let started = *self.started.get_or_insert(ctx.now);
        let active = self
            .props
            .get("active")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let reduce = self
            .props
            .get("reducedMotion")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let elapsed = self
            .props
            .get("timeMs")
            .and_then(Value::as_f64)
            .unwrap_or_else(|| ctx.now.saturating_duration_since(started).as_secs_f64() * 1000.0)
            as f32;
        let elapsed = if reduce { 0.0 } else { elapsed };
        if active
            && !reduce
            && !self.props.contains_key("timeMs")
            && !matches!(
                self.string("kind", "Drive").as_str(),
                "counter" | "ring" | "hatch"
            )
        {
            window.request_animation_frame();
        }
        let kind = self.string("kind", "Drive");
        let foreground = self.color("color", "#f2f3f4");
        let track = self.color("track", "#3a3c40");
        let mut root = super::custom_surface(
            div().id(gpui::SharedString::from(format!("indicator-{}", ctx.id))),
            &ctx,
        );
        if kind == "counter" {
            let text = self.string("text", "");
            if text != self.counter_text {
                self.counter_changed = if self.counter_text.is_empty() {
                    None
                } else {
                    Some(ctx.now)
                };
                self.counter_from = std::mem::replace(&mut self.counter_text, text.clone());
            }
            let p = if reduce {
                1.0
            } else {
                self.counter_changed
                    .map(|t| {
                        (ctx.now.saturating_duration_since(t).as_secs_f32() / 0.35).clamp(0.0, 1.0)
                    })
                    .unwrap_or(1.0)
            };
            if p < 1.0 {
                window.request_animation_frame();
            }
            let eased = 1.0 - (1.0 - p).powi(3);
            let line = ctx.style.and_then(|s| s.line_height).unwrap_or(19.0) as f32;
            let old: Vec<_> = self.counter_from.chars().collect();
            let ascending = text
                .split_whitespace()
                .next()
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(0)
                >= self
                    .counter_from
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(0);
            let direction = if ascending { 1.0 } else { -1.0 };
            root = root.flex().flex_row();
            for (i, c) in text.chars().enumerate() {
                let before = old.get(i).copied().unwrap_or(c);
                let mut digit = div().relative().overflow_hidden().h(px(line));
                if before != c && p < 1.0 {
                    digit = digit
                        .child(
                            div()
                                .relative()
                                .top(px(direction * (1.0 - eased) * line))
                                .child(ctx.chrome_text(c.to_string(), None)),
                        )
                        .child(
                            div()
                                .absolute()
                                .top(px(-direction * eased * line))
                                .child(ctx.chrome_text(before.to_string(), None)),
                        );
                } else {
                    digit = digit.child(ctx.chrome_text(c.to_string(), None));
                }
                root = root.child(digit);
            }
            return root.into_any_element();
        }
        if kind == "shimmer" {
            let text = self.string("text", "");
            let base = self.color("track", "#6c6f75");
            let graphemes: Vec<_> = text.graphemes(true).collect();
            let count = graphemes.len().max(1) as f32;
            let mut font = window.text_style().font();
            if let Some(style) = ctx.style {
                if let Some(family) = &style.font_family {
                    font.family = family.clone().into();
                }
                if let Some(features) = &style.font_features {
                    font.features = crate::style::font_features(features);
                }
                if let Some(weight) = &style.font_weight {
                    font.weight = crate::renderer::parse_font_weight(weight);
                }
            }
            let period = self.number("periodMs", 1800.0).max(100.0);
            let runs = graphemes
                .iter()
                .enumerate()
                .map(|(i, g)| {
                    let p = i as f32 / count;
                    let center = -0.5 + 2.0 * (elapsed / period).fract();
                    let brightness = if active && !reduce {
                        ((p - center).abs() / 0.3).clamp(0.0, 1.0)
                    } else {
                        1.0
                    };
                    let mix = |a: f32, b: f32| a + (b - a) * brightness;
                    gpui::TextRun {
                        len: g.len(),
                        font: font.clone(),
                        color: Rgba {
                            r: mix(foreground.r, base.r),
                            g: mix(foreground.g, base.g),
                            b: mix(foreground.b, base.b),
                            a: 1.0,
                        }
                        .into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    }
                })
                .collect();
            return root.child(ctx.text(0, text, Some(runs))).into_any_element();
        }
        let cell = self
            .props
            .get("cellIndex")
            .and_then(Value::as_u64)
            .filter(|i| *i < 9)
            .map(|i| i as usize);
        let target = self.number("progress", 0.0).clamp(0.0, 1.0);
        let duration = self.number("durationMs", 0.0).max(0.0);
        let p = self
            .progress_changed
            .map(|t| {
                ctx.now.saturating_duration_since(t).as_secs_f32() * 1000.0 / duration.max(1.0)
            })
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);
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
        let mut progress = self.progress_from
            + (self.progress_to.unwrap_or(target) - self.progress_from)
                * super::glide::ease(p, curve);
        if self.progress_to != Some(target) {
            if self.progress_to.is_none() {
                progress = target;
            }
            self.progress_from = progress;
            self.progress_to = Some(target);
            self.progress_changed = Some(ctx.now);
        }
        if reduce || duration == 0.0 {
            progress = target;
            self.progress_from = target;
        }
        if kind == "ring" && (progress - target).abs() > 0.0001 {
            window.request_animation_frame();
        }

        let stroke = self.number("stroke", 2.0).max(0.5);
        root = root.child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    if kind == "hatch" {
                        let width = f32::from(bounds.size.width);
                        let height = f32::from(bounds.size.height);
                        let mut y = -width;
                        while y < height {
                            let mut path = PathBuilder::stroke(px(stroke));
                            path.move_to(bounds.origin + point(px(0.0), px(y)));
                            path.line_to(bounds.origin + point(px(width), px(y + width)));
                            if let Ok(path) = path.build() {
                                window.paint_path(path, foreground);
                            }
                            y += 4.0;
                        }
                    } else if kind == "ring" || kind == "spinner" {
                        let radius =
                            (f32::from(bounds.size.width.min(bounds.size.height)) - stroke) / 2.0;
                        let center = bounds.center();
                        let arc =
                            |start: f32, sweep: f32, color: Rgba, window: &mut gpui::Window| {
                                let mut p = PathBuilder::stroke(px(stroke));
                                let steps = (sweep.abs() * radius / 2.0).ceil().max(8.0) as usize;
                                for i in 0..=steps {
                                    let a = start + sweep * i as f32 / steps as f32;
                                    let q =
                                        center + point(px(a.cos() * radius), px(a.sin() * radius));
                                    if i == 0 {
                                        p.move_to(q)
                                    } else {
                                        p.line_to(q)
                                    }
                                }
                                if let Ok(path) = p.build() {
                                    window.paint_path(path, color);
                                }
                            };
                        arc(0.0, std::f32::consts::TAU, track, window);
                        let start = if kind == "spinner" && active && !reduce {
                            elapsed / 1100.0 * std::f32::consts::TAU
                        } else {
                            -std::f32::consts::FRAC_PI_2
                        };
                        let span = if kind == "spinner" { 0.28 } else { progress };
                        if span > 0.0 {
                            arc(start, span * std::f32::consts::TAU, foreground, window);
                            if span < 1.0 {
                                for angle in [start, start + span * std::f32::consts::TAU] {
                                    let at = center
                                        + point(px(angle.cos() * radius), px(angle.sin() * radius));
                                    window.paint_quad(
                                        gpui::fill(
                                            Bounds::new(
                                                at - point(px(stroke / 2.0), px(stroke / 2.0)),
                                                size(px(stroke), px(stroke)),
                                            ),
                                            foreground,
                                        )
                                        .corner_radii(px(stroke / 2.0)),
                                    );
                                }
                            }
                        }
                    } else {
                        let edge = (f32::from(bounds.size.width.min(bounds.size.height))
                            / if cell.is_some() { 1.0 } else { 4.0 })
                        .max(1.0);
                        let gap = edge * 0.375;
                        let orbit = [0, 1, 2, 5, 8, 7, 6, 3];
                        for i in cell.unwrap_or(0)..cell.map(|i| i + 1).unwrap_or(9) {
                            let row = i / 3;
                            let col = i % 3;
                            let delay = if kind == "Orbit" {
                                orbit.iter().position(|n| *n == i).map(|n| n as f32 * 110.0)
                            } else {
                                Some((col + (row as i32 - 1).unsigned_abs() as usize) as f32 * 90.0)
                            };
                            let period = if kind == "Orbit" { 950.0 } else { 650.0 };
                            let phase =
                                (elapsed - delay.unwrap_or(0.0)).rem_euclid(period) / period;
                            let alpha = if !active || reduce || delay.is_none() {
                                0.15
                            } else {
                                0.15 + 0.85 * (1.0 - ((phase - 0.28) / 0.22).abs()).max(0.0)
                            };
                            let rect = Bounds::new(
                                bounds.origin
                                    + point(
                                        px(if cell.is_some() {
                                            0.0
                                        } else {
                                            col as f32 * (edge + gap)
                                        }),
                                        px(if cell.is_some() {
                                            0.0
                                        } else {
                                            row as f32 * (edge + gap)
                                        }),
                                    ),
                                size(px(edge), px(edge)),
                            );
                            let mut color = foreground;
                            color.a *= alpha;
                            window.paint_quad(
                                gpui::fill(rect, color).corner_radii(px(if kind == "Dots" {
                                    edge / 2.0
                                } else {
                                    1.0
                                })),
                            );
                        }
                    }
                },
            )
            .size_full(),
        );
        root.into_any_element()
    }
    fn set_prop(&mut self, key: &str, value: Value) {
        if value.is_null() {
            self.props.remove(key);
        } else {
            self.props.insert(key.to_string(), value);
        }
    }
    fn supported_props(&self) -> &'static [&'static str] {
        &[
            "kind",
            "active",
            "reducedMotion",
            "timeMs",
            "color",
            "track",
            "text",
            "progress",
            "stroke",
            "cellIndex",
            "periodMs",
            "durationMs",
            "ease",
        ]
    }
    fn supported_events(&self) -> &'static [&'static str] {
        &[]
    }
    fn destroy(&mut self) {}
}
