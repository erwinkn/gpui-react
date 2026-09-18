//! A selectable native paragraph with stable UTF-16 range addressing.
use super::{CustomElement, CustomElementFactory, CustomRenderContext};
use gpui::prelude::*;
use serde_json::Value;
use std::collections::HashMap;
pub struct SelectionTextFactory;
impl CustomElementFactory for SelectionTextFactory {
    fn element_type(&self) -> &str {
        "cherry-selection-text"
    }
    fn create(&self, _: u64) -> Box<dyn CustomElement> {
        Box::new(SelectionText {
            props: HashMap::new(),
            applied: None,
        })
    }
}
struct SelectionText {
    props: HashMap<String, Value>,
    applied: Option<(String, Value)>,
}
fn byte_offset(text: &str, utf16: usize) -> usize {
    let mut count = 0;
    for (byte, c) in text.char_indices() {
        if count >= utf16 {
            return byte;
        }
        // A position inside a surrogate pair snaps to the leading boundary.
        if count + c.len_utf16() > utf16 {
            return byte;
        }
        count += c.len_utf16();
    }
    text.len()
}
impl CustomElement for SelectionText {
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        _: &mut gpui::Window,
        _: &mut gpui::Context<crate::renderer::GpuixView>,
    ) -> gpui::AnyElement {
        let text = self
            .props
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let locked = self
            .props
            .get("lockSelection")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut caret = None;
        if let Some(range) = self.props.get("selectedRange").filter(|v| v.is_object()) {
            let next = (text.clone(), range.clone());
            let start = byte_offset(
                &text,
                range.get("start").and_then(Value::as_u64).unwrap_or(0) as usize,
            );
            let end = byte_offset(
                &text,
                range.get("end").and_then(Value::as_u64).unwrap_or(0) as usize,
            );
            if locked && start == end {
                caret = Some(start);
            }
            if self.applied.as_ref() != Some(&next)
                || (locked
                    && ctx
                        .selection
                        .lock()
                        .wash_range(&crate::text::selection_key(ctx.id, 0))
                        != Some(start.min(end)..end.max(start)))
            {
                let mut selection = ctx.selection.lock();
                selection.begin_with_span(
                    &crate::text::selection_key(ctx.id, 0),
                    &text,
                    start.min(end)..end.max(start),
                );
                selection.end_active_drag();
                self.applied = Some(next);
            }
        } else {
            self.applied = None;
        }
        super::custom_surface(
            gpui::div().id(gpui::SharedString::from(format!(
                "selection-text-{}",
                ctx.id
            ))),
            &ctx,
        )
        .child(crate::text::selectable_text(crate::text::SelectableText {
            selectable: ctx.selectable,
            anchor_caret: caret,
            ..crate::text::SelectableText::new(
                ctx.id,
                0,
                text.into(),
                None,
                ctx.selection.clone(),
                ctx.selection_wash,
            )
        }))
        .into_any_element()
    }
    fn set_prop(&mut self, key: &str, value: Value) {
        self.props.insert(key.to_owned(), value);
    }
    fn supported_props(&self) -> &'static [&'static str] {
        &["text", "selectedRange", "lockSelection"]
    }
    fn supported_events(&self) -> &'static [&'static str] {
        &["mouseDown", "mouseUp", "click", "keyDown", "focus", "blur"]
    }
    fn destroy(&mut self) {}
}
#[cfg(test)]
mod tests {
    use super::byte_offset;
    #[test]
    fn utf16_positions_keep_unicode_boundaries() {
        assert_eq!(byte_offset("a🍒é", 1), 1);
        assert_eq!(byte_offset("a🍒é", 2), 1);
        assert_eq!(byte_offset("a🍒é", 3), 5);
        assert_eq!(byte_offset("a🍒é", 4), 7);
        assert_eq!(byte_offset("a🍒é", 999), 7);
    }
}
