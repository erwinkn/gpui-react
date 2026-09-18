//! Incoming text is never queued. GPUI owns shaping, wrapping, selection, and caret geometry.
use super::{CustomElement, CustomElementFactory, CustomRenderContext};
use gpui::{canvas, div, point, prelude::*, px, size, Bounds, Pixels, Point, Rgba};
use serde_json::Value;
use std::{
    cell::RefCell,
    collections::HashMap,
    rc::{Rc, Weak},
};
use unicode_segmentation::UnicodeSegmentation;
#[derive(Default)]
struct Frame {
    caret: Option<Point<Pixels>>,
    font_size: f32,
    line_height: f32,
    color: Option<Rgba>,
}
type Shared = Rc<RefCell<Frame>>;
thread_local! {static FRAMES:RefCell<HashMap<String,Weak<RefCell<Frame>>>>=RefCell::default();}
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
pub struct StreamFactory(pub bool);
impl CustomElementFactory for StreamFactory {
    fn element_type(&self) -> &str {
        if self.0 {
            "cherry-stream-caret"
        } else {
            "cherry-stream-text"
        }
    }
    fn create(&self, _: u64) -> Box<dyn CustomElement> {
        Box::new(Stream {
            caret: self.0,
            props: HashMap::new(),
            frame: None,
            started: None,
        })
    }
}
struct Stream {
    caret: bool,
    props: HashMap<String, Value>,
    frame: Option<Shared>,
    started: Option<web_time::Instant>,
}
impl Stream {
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
    fn flag(&self, key: &str) -> bool {
        self.props
            .get(key)
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }
}
impl CustomElement for Stream {
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        window: &mut gpui::Window,
        _: &mut gpui::Context<crate::renderer::GpuixView>,
    ) -> gpui::AnyElement {
        let shared = frame(&self.text("scope", "default"));
        self.frame = Some(shared.clone());
        let mut root = super::custom_surface(
            div().id(gpui::SharedString::from(format!("stream-{}", ctx.id))),
            &ctx,
        );
        let reduced = self.flag("reducedMotion");
        let active = self.flag("active");
        if self.caret {
            let show = self.flag("caret");
            let started = *self.started.get_or_insert(ctx.now);
            let period = self.number("blinkMs", 1000.0).max(100.0);
            let phase = ctx.now.saturating_duration_since(started).as_secs_f32() * 1000.0 / period;
            let visible = show && (active || reduced || phase.fract() < 0.5);
            if show && !active && !reduced {
                window.request_animation_frame();
            }
            let width = self.number("caretWidth", 2.0).max(0.5);
            root = root.child(
                canvas(
                    |_, _, _| (),
                    move |_, _, window, _| {
                        if !visible {
                            return;
                        }
                        let f = shared.borrow();
                        if let Some(at) = f.caret {
                            let height = f.font_size * 1.05;
                            let bounds = Bounds::new(
                                at + point(px(width * 0.75), px((f.line_height - height) / 2.0)),
                                size(px(width), px(height)),
                            );
                            window.paint_quad(
                                gpui::fill(bounds, f.color.unwrap_or(gpui::rgb(0xf2f3f4)))
                                    .corner_radii(px(width / 2.0)),
                            );
                        }
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            );
            return root.into_any_element();
        }
        let text = self.text("text", "");
        let mut font = window.text_style().font();
        let font_size = ctx.style.and_then(|s| s.font_size).unwrap_or(13.0) as f32;
        let color = ctx
            .style
            .and_then(|s| s.color.as_deref())
            .and_then(crate::color::parse_color_rgba)
            .unwrap_or(gpui::rgb(0xf2f3f4));
        if let Some(style) = ctx.style {
            if let Some(features) = &style.font_features {
                font.features = crate::style::font_features(features);
            }
            if let Some(family) = &style.font_family {
                font.family = family.clone().into();
            }
            if let Some(weight) = &style.font_weight {
                font.weight = crate::renderer::parse_font_weight(weight);
            }
        }
        let count = if active && !reduced {
            self.number("tailGraphemes", 6.0).clamp(0.0, 64.0) as usize
        } else {
            0
        };
        let mut tail: Vec<_> = text.grapheme_indices(true).rev().take(count).collect();
        tail.reverse();
        let start = tail.first().map(|(i, _)| *i).unwrap_or(text.len());
        let opacity = self.number("tailOpacity", 0.25).clamp(0.0, 1.0);
        let run = |len: usize, alpha: f32| gpui::TextRun {
            len,
            font: font.clone(),
            color: Rgba {
                a: color.a * alpha,
                ..color
            }
            .into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let mut runs = Vec::new();
        if start > 0 {
            runs.push(run(start, 1.0));
        }
        for (i, (_, g)) in tail.iter().enumerate() {
            let fraction = (i + 1) as f32 / tail.len().max(1) as f32;
            runs.push(run(g.len(), 1.0 - (1.0 - opacity) * fraction));
        }
        let len = text.len();
        let mut options = crate::text::SelectableText::new(
            ctx.id,
            0,
            text.into(),
            Some(runs),
            ctx.selection.clone(),
            ctx.selection_wash,
        );
        options.selectable = ctx.selectable;
        options.highlight = ctx
            .highlight_set
            .clone()
            .map(crate::text::HighlightSource::Native);
        options.extra_wash = Some(Box::new(move |layout, _| {
            let mut f = shared.borrow_mut();
            f.caret = Some(
                layout
                    .position_for_index(len)
                    .unwrap_or(layout.bounds().origin),
            );
            f.font_size = font_size;
            f.line_height = f32::from(layout.line_height());
            f.color = Some(color);
        }));
        root.child(crate::text::selectable_text(options))
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
            "scope",
            "text",
            "active",
            "caret",
            "tailGraphemes",
            "tailOpacity",
            "blinkMs",
            "caretWidth",
            "reducedMotion",
        ]
    }
    fn supported_events(&self) -> &'static [&'static str] {
        &[]
    }
    fn destroy(&mut self) {
        self.frame = None;
    }
}
