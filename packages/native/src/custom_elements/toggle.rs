//! Native switch paint. Checked state is immediate; paint interpolation is interruptible.
use super::{CustomElement, CustomElementFactory, CustomRenderContext};
use gpui::{canvas, div, point, prelude::*, px, size, Bounds, Rgba};
use serde_json::Value;
use std::collections::HashMap;
pub struct ToggleFactory;
impl CustomElementFactory for ToggleFactory {
    fn element_type(&self) -> &str {
        "cherry-toggle"
    }
    fn create(&self, _: u64) -> Box<dyn CustomElement> {
        Box::new(Toggle {
            props: HashMap::new(),
            from: 0.0,
            to: None,
            started: None,
        })
    }
}
struct Toggle {
    props: HashMap<String, Value>,
    from: f32,
    to: Option<f32>,
    started: Option<web_time::Instant>,
}
impl Toggle {
    fn color(&self, key: &str, default: &str) -> Rgba {
        crate::color::parse_color_rgba(
            self.props
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or(default),
        )
        .unwrap_or(gpui::rgb(0xffffff))
    }
}
impl CustomElement for Toggle {
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        window: &mut gpui::Window,
        _: &mut gpui::Context<crate::renderer::GpuixView>,
    ) -> gpui::AnyElement {
        let checked = self
            .props
            .get("checked")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let desired = if checked { 1.0 } else { 0.0 };
        let duration = self
            .props
            .get("durationMs")
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite())
            .unwrap_or(200.0)
            .max(0.0) as f32;
        let reduced = self
            .props
            .get("reducedMotion")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || window.last_input_was_keyboard()
            || duration == 0.0;
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
        let progress = self
            .started
            .map(|s| {
                ctx.now.saturating_duration_since(s).as_secs_f32() * 1000.0 / duration.max(1.0)
            })
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);
        let mut current = self.from
            + (self.to.unwrap_or(desired) - self.from) * super::glide::ease(progress, curve);
        if self.to != Some(desired) {
            if self.to.is_none() {
                current = desired;
            }
            self.from = current;
            self.to = Some(desired);
            self.started = Some(ctx.now);
        }
        if reduced {
            current = desired;
            self.from = desired;
        }
        if (current - desired).abs() > 0.0001 {
            window.request_animation_frame();
        }
        let layer = self
            .props
            .get("layer")
            .and_then(Value::as_str)
            .unwrap_or("track")
            .to_owned();
        let off = self.color("offColor", "#3a3c40");
        let on = self.color("onColor", "#f2f3f4");
        let thumb = self.color("thumbColor", "#ffffff");
        let mut root = super::custom_surface(
            div().id(gpui::SharedString::from(format!("toggle-{}", ctx.id))),
            &ctx,
        );
        root = root.child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    let h = f32::from(bounds.size.height);
                    let w = f32::from(bounds.size.width);
                    if layer == "track" {
                        let color = Rgba {
                            r: off.r + (on.r - off.r) * current,
                            g: off.g + (on.g - off.g) * current,
                            b: off.b + (on.b - off.b) * current,
                            a: off.a + (on.a - off.a) * current,
                        };
                        window.paint_quad(gpui::fill(bounds, color).corner_radii(px(h / 2.0)));
                    } else {
                        let inset = h / 12.0;
                        let diameter = (h - 2.0 * inset).max(0.0).min(w - 2.0 * inset).max(0.0);
                        let thumb_bounds = Bounds::new(
                            bounds.origin
                                + point(
                                    px(inset + (w - 2.0 * inset - diameter) * current),
                                    px(inset),
                                ),
                            size(px(diameter), px(diameter)),
                        );
                        window.paint_drop_shadows(
                            thumb_bounds,
                            gpui::Corners::all(px(diameter / 2.0)),
                            &[gpui::BoxShadow {
                                color: gpui::rgba(0x00000033).into(),
                                offset: point(px(0.0), px(h / 24.0)),
                                blur_radius: px(h / 12.0),
                                spread_radius: px(0.0),
                                inset: false,
                            }],
                        );
                        window.paint_quad(
                            gpui::fill(thumb_bounds, thumb).corner_radii(px(diameter / 2.0)),
                        );
                    }
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        );
        for child in ctx.children {
            root = root.child(child);
        }
        root.into_any_element()
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
            "checked",
            "layer",
            "durationMs",
            "ease",
            "reducedMotion",
            "onColor",
            "offColor",
            "thumbColor",
        ]
    }
    fn supported_events(&self) -> &'static [&'static str] {
        &[]
    }
    fn destroy(&mut self) {}
}
