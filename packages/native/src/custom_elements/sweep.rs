//! A decorative GPU sweep. The native clock controls paint; React never runs per frame.
use super::{CustomElement, CustomElementFactory, CustomRenderContext};
use gpui::{canvas, div, prelude::*, px, IridescentSweep};
use serde_json::Value;
use std::collections::HashMap;
pub struct SweepFactory;
impl CustomElementFactory for SweepFactory {
    fn element_type(&self) -> &str {
        "cherry-sweep"
    }
    fn create(&self, _: u64) -> Box<dyn CustomElement> {
        Box::new(Sweep {
            props: HashMap::new(),
            trigger: None,
            started: None,
        })
    }
}
struct Sweep {
    props: HashMap<String, Value>,
    trigger: Option<Value>,
    started: Option<web_time::Instant>,
}
impl Sweep {
    fn number(&self, key: &str, fallback: f32) -> f32 {
        self.props
            .get(key)
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite())
            .map(|v| v as f32)
            .unwrap_or(fallback)
    }
    fn palette(&self, key: &str, fallback: [f32; 3]) -> [f32; 3] {
        let Some(values) = self
            .props
            .get("palette")
            .and_then(|v| v.get(key))
            .and_then(Value::as_array)
        else {
            return fallback;
        };
        std::array::from_fn(|i| {
            values
                .get(i)
                .and_then(Value::as_f64)
                .filter(|v| v.is_finite())
                .map(|v| v as f32)
                .unwrap_or(fallback[i])
        })
    }
}
impl CustomElement for Sweep {
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        window: &mut gpui::Window,
        _: &mut gpui::Context<crate::renderer::GpuixView>,
    ) -> gpui::AnyElement {
        let mut root = super::custom_surface(
            div().id(gpui::SharedString::from(format!("sweep-{}", ctx.id))),
            &ctx,
        );
        let trigger = self.props.get("trigger").cloned().unwrap_or(Value::from(0));
        if self.trigger.as_ref() != Some(&trigger) {
            self.started = if trigger == Value::from(0) || trigger.is_null() {
                None
            } else {
                Some(ctx.now)
            };
            self.trigger = Some(trigger);
        }
        let enabled = self.props.get("enabled").and_then(Value::as_bool) != Some(false)
            && self.props.get("reducedMotion").and_then(Value::as_bool) != Some(true);
        if !enabled {
            self.started = None;
        }
        let duration = self.number("sweepMs", 570.).clamp(0., 60_000.);
        let outro = self.number("outroMs", 80.).clamp(0., 60_000.);
        let seconds = self
            .started
            .map(|t| ctx.now.saturating_duration_since(t).as_secs_f32())
            .unwrap_or(0.);
        let elapsed = seconds * 1000.;
        let fixed = self
            .props
            .get("progress")
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite());
        let active = enabled && self.started.is_some() && elapsed < duration + outro;
        let raw = if duration > 0. {
            (elapsed / duration).clamp(0., 1.)
        } else {
            1.
        };
        let progress = fixed
            .map(|v| v as f32)
            .unwrap_or_else(|| {
                if self.props.get("easing").and_then(Value::as_str) == Some("linear") {
                    raw
                } else if raw >= 1. {
                    1.
                } else {
                    1. - 2.0f32.powf(-10. * raw)
                }
            })
            .clamp(0., 1.);
        let fade = if elapsed <= duration {
            1.
        } else if outro <= 0. {
            0.
        } else {
            let t = ((elapsed - duration) / outro).clamp(0., 1.);
            1. - if t < 0.5 {
                4. * t * t * t
            } else {
                1. - (-2. * t + 2.).powi(3) / 2.
            }
        };
        let alpha = if !enabled || (fixed.is_none() && !active) {
            0.
        } else {
            self.number("peakAlpha", 1.3).clamp(0., 1.5) * if fixed.is_some() { 1. } else { fade }
        };
        if active && fixed.is_none() {
            window.request_animation_frame();
        }
        if alpha > 0. {
            let sweep = IridescentSweep {
                palette_a: self.palette("a", [0.5, 0.5, 0.5]),
                palette_b: self.palette("b", [0.5, 0.5, 0.5]),
                palette_c: self.palette("c", [1., 1., 1.]),
                palette_d: self.palette("d", [0., 0.33, 0.67]),
                progress,
                time: if fixed.is_some() {
                    self.number("time", 0.)
                } else {
                    seconds
                },
                alpha,
                direction: match self.props.get("direction").and_then(Value::as_str) {
                    Some("rtl") => 1.,
                    Some("ttb") => 2.,
                    Some("btt") => 3.,
                    _ => 0.,
                },
                band_tight: self.number("bandTight", 10.).clamp(0.1, 200.),
                wave_amount: self.number("waveAmount", 0.).clamp(0., 2.),
                ripple_amount: self.number("rippleAmount", 1.).clamp(0., 2.),
                wave_speed: self.number("waveSpeed", 1.8).clamp(0., 3.),
                brightness: self.number("brightness", 1.4).clamp(0., 1.5),
                swell_amount: self.number("swellAmount", 1.).clamp(0., 1.),
                hue_shift: self.number("hueShift", 0.),
                padding: 0.,
            };
            let radius = self.number("radius", 14.).max(0.);
            root = root.child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        window.paint_quad(gpui::quad(
                            bounds,
                            px(radius
                                .min(f32::from(bounds.size.width) / 2.)
                                .min(f32::from(bounds.size.height) / 2.)),
                            gpui::iridescent_sweep(sweep),
                            px(0.),
                            gpui::transparent_black(),
                            gpui::BorderStyle::Solid,
                        ));
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            );
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
            "trigger",
            "enabled",
            "reducedMotion",
            "progress",
            "time",
            "sweepMs",
            "outroMs",
            "easing",
            "peakAlpha",
            "direction",
            "bandTight",
            "waveAmount",
            "rippleAmount",
            "waveSpeed",
            "brightness",
            "swellAmount",
            "hueShift",
            "radius",
            "palette",
        ]
    }
    fn supported_events(&self) -> &'static [&'static str] {
        &[]
    }
    fn destroy(&mut self) {
        self.started = None;
    }
}
