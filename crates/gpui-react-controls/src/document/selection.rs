//! Cross-element text selection state.
//!
//! Ported from Comet, MIT.
//! Upstream: https://github.com/zeronsh/comet/blob/main/crates/ui/src/markdown/selection.rs
//! Reviewed fix: https://github.com/zeronsh/comet/commit/3536a3702ca405fec1321e95f54e280240c5d38f
//!
//! Each registered text is a complete logical paragraph with optional styled
//! runs. Selection uses paint order and shares its source strings. It can keep
//! selected content after virtualization removes the corresponding views.
//! State belongs to one Document. No window or React tree is needed here.

use gpui::SharedString;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

/// One element's slice of the selection, in document order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    /// Stable logical key inside the document.
    pub key: SharedString,
    pub range: Range<usize>,
    /// Shared immutable source, retained until the selection is cleared.
    pub text: SharedString,
}

#[derive(Clone, Debug)]
pub struct RegisteredText {
    pub key: SharedString,
    pub text: SharedString,
}

/// Live selection for one document.
#[derive(Clone, Debug, Default)]
pub struct SelectionState {
    /// Element that owns the drag (where the mouse went down).
    anchor_key: String,
    /// Byte offset of the anchor within its element.
    anchor_ix: usize,
    dragging: bool,
    /// Direction established while the anchor is still painted.
    forward: Option<bool>,
    /// Resolved spans in document order. Empty while a click has not moved.
    spans: Vec<Span>,
    active: bool,
    /// A press keeps ordinary click routing until a move starts a drag.
    pending: bool,
}

impl SelectionState {
    /// Remember a press without selecting. The next dragging move promotes it.
    pub fn arm(&mut self, key: &str, ix: usize) -> bool {
        let changed = self.has_selection();
        self.anchor_key = key.to_string();
        self.anchor_ix = ix;
        self.dragging = false;
        self.forward = None;
        self.active = false;
        self.pending = true;
        self.spans.clear();
        changed
    }

    /// Turn a pending press into a live drag. True when this call started it.
    pub fn promote_pending(&mut self) -> bool {
        if !self.pending {
            return false;
        }
        self.pending = false;
        self.dragging = true;
        self.active = true;
        true
    }

    pub fn cancel_pending(&mut self) {
        if self.pending {
            *self = SelectionState::default();
        }
    }

    pub fn is_pending(&self) -> bool {
        self.pending
    }

    /// Begin with an immediate span — double or triple click inside one element.
    pub fn begin_with_span(
        &mut self,
        key: &SharedString,
        text: &SharedString,
        range: Range<usize>,
    ) -> bool {
        let spans = vec![Span {
            key: key.clone(),
            text: text.clone(),
            range: range.clone(),
        }];
        let changed = self.spans != spans;
        self.anchor_key = key.to_string();
        self.anchor_ix = range.start;
        self.dragging = true;
        self.forward = None;
        self.active = true;
        self.pending = false;
        self.spans = spans;
        changed
    }

    pub fn is_dragging(&self) -> bool {
        self.active && self.dragging
    }

    /// Resolve a drag head against this frame's visible runs.
    ///
    /// Once virtualization removes the anchor, an overlapping selected run
    /// joins the visible frame to the spans retained from earlier frames.
    pub fn update_drag(&mut self, elements: &[RegisteredText], head: (usize, usize)) -> bool {
        if !self.is_dragging() {
            return false;
        }
        let spans = if let Some(anchor_element) = elements
            .iter()
            .position(|element| element.key.as_ref() == self.anchor_key)
        {
            let anchor = (anchor_element, self.anchor_ix);
            self.forward = Some(anchor <= head);
            resolve_spans(elements, anchor, head)
        } else {
            let Some(forward) = self.forward else {
                return false;
            };
            let Some(spans) = extend_virtualized_drag(&self.spans, elements, head, forward) else {
                return false;
            };
            spans
        };
        self.update_spans(spans)
    }

    /// Replace the resolved spans. Returns true when they changed.
    pub fn update_spans(&mut self, spans: Vec<Span>) -> bool {
        if !self.active || self.spans == spans {
            return false;
        }
        self.spans = spans;
        true
    }

    /// End the active drag even when its anchor is no longer painted.
    pub fn end_active_drag(&mut self) {
        self.dragging = false;
        if !self.has_selection() {
            self.clear();
        }
    }

    pub fn clear(&mut self) -> bool {
        let changed = self.has_selection();
        *self = SelectionState::default();
        changed
    }

    pub fn has_selection(&self) -> bool {
        self.active && self.spans.iter().any(|span| !span.range.is_empty())
    }

    pub fn spans(&self) -> &[Span] {
        &self.spans
    }

    /// The wash range for `key` this frame. `None` means nothing to paint.
    pub fn wash_range(&self, key: &str) -> Option<Range<usize>> {
        if !self.active {
            return None;
        }
        self.spans
            .iter()
            .find(|s| s.key.as_ref() == key && !s.range.is_empty())
            .map(|s| s.range.clone())
    }

    /// The full selected text, spans joined in document order.
    pub fn selected_text(&self) -> Option<String> {
        if !self.has_selection() {
            return None;
        }
        Some(join_spans(&self.spans))
    }
}

fn extend_virtualized_drag(
    existing: &[Span],
    elements: &[RegisteredText],
    head: (usize, usize),
    forward: bool,
) -> Option<Vec<Span>> {
    if forward {
        let (element_index, span_index) =
            elements
                .iter()
                .enumerate()
                .find_map(|(element_index, element)| {
                    existing
                        .iter()
                        .position(|span| span.key == element.key)
                        .map(|span_index| (element_index, span_index))
                })?;
        let start = existing.get(span_index)?.range.start;
        let mut merged = existing.get(..span_index)?.to_vec();
        merged.extend(resolve_spans(elements, (element_index, start), head));
        Some(merged)
    } else {
        let (element_index, span_index) =
            elements
                .iter()
                .enumerate()
                .rev()
                .find_map(|(element_index, element)| {
                    existing
                        .iter()
                        .position(|span| span.key == element.key)
                        .map(|span_index| (element_index, span_index))
                })?;
        let end = existing.get(span_index)?.range.end;
        let mut merged = resolve_spans(elements, head, (element_index, end));
        merged.extend_from_slice(existing.get(span_index + 1..)?);
        Some(merged)
    }
}

/// Resolve the spans for a selection between `a` and `b`, each an
/// `(element index, byte offset)` into `elements` (document-ordered painted
/// runs). Handles either direction; empty slices are skipped.
pub fn resolve_spans(
    elements: &[RegisteredText],
    a: (usize, usize),
    b: (usize, usize),
) -> Vec<Span> {
    let (start, end) = if (a.0, a.1) <= (b.0, b.1) {
        (a, b)
    } else {
        (b, a)
    };
    let mut spans = Vec::new();
    for (ei, entry) in elements.iter().enumerate().take(end.0 + 1).skip(start.0) {
        let text = &entry.text;
        let from = if ei == start.0 { start.1 } else { 0 };
        let to = if ei == end.0 { end.1 } else { text.len() };
        let (from, to) = (clamp_boundary(text, from), clamp_boundary(text, to));
        if from < to {
            spans.push(Span {
                key: entry.key.clone(),
                range: from..to,
                text: text.clone(),
            });
        }
    }
    spans
}

/// Clamp a byte offset into `text` and snap it down to a char boundary.
/// Mouse-derived indices are already on boundaries; this is defensive so a
/// stale index from a previous frame's text can never panic on slicing.
fn clamp_boundary(text: &str, mut ix: usize) -> usize {
    ix = ix.min(text.len());
    while ix > 0 && !text.is_char_boundary(ix) {
        ix -= 1;
    }
    ix
}

/// Join complete logical texts with one newline between them.
fn join_spans(spans: &[Span]) -> String {
    let mut out = String::new();
    for (index, span) in spans
        .iter()
        .filter(|span| !span.range.is_empty())
        .enumerate()
    {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(&span.text[span.range.clone()]);
    }
    out
}

/// Select a Unicode word or a whole non-space grapheme under the pointer.
/// At whitespace or end of text, retain the preceding word-end behavior.
pub fn word_range(text: &str, ix: usize) -> Range<usize> {
    let ix = clamp_boundary(text, ix);
    let mut preceding = None;
    for (start, word) in text.unicode_word_indices() {
        let range = start..start + word.len();
        if range.contains(&ix) {
            return range;
        }
        if range.end == ix {
            preceding = Some(range);
        }
        if start > ix {
            break;
        }
    }
    if let Some((start, grapheme)) = text
        .grapheme_indices(true)
        .find(|(start, grapheme)| *start <= ix && ix < *start + grapheme.len())
        && !grapheme.chars().all(char::is_whitespace)
    {
        return start..start + grapheme.len();
    }
    preceding.unwrap_or(ix..ix)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg(key: &str, text: &str) -> RegisteredText {
        RegisteredText {
            key: key.to_owned().into(),
            text: text.to_owned().into(),
        }
    }
    fn elems() -> Vec<RegisteredText> {
        vec![
            reg("p1", "first paragraph"),
            reg("p2", "second"),
            reg("p3", "third one"),
        ]
    }

    #[test]
    fn word_selection_keeps_combining_marks_and_emoji_graphemes() {
        let text = "e\u{301}cole";
        assert_eq!(word_range(text, 0), 0..text.len());
        let family = "👩‍👩‍👦";
        assert_eq!(word_range(family, 0), 0..family.len());
    }

    #[test]
    fn selected_spans_share_source_bytes() {
        let text = "long selected text".repeat(1000);
        let entries = [reg("source", &text)];
        let spans = resolve_spans(&entries, (0, 0), (0, text.len()));
        assert_eq!(
            spans[0].text.as_ptr(),
            entries[0].text.as_ptr(),
            "drag frames must retain shared source text, not copy its bytes"
        );
    }

    #[test]
    fn spans_within_one_element() {
        let spans = resolve_spans(&elems(), (0, 6), (0, 15));
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].key, "p1");
        assert_eq!(&spans[0].text[spans[0].range.clone()], "paragraph");
        assert_eq!(resolve_spans(&elems(), (0, 15), (0, 6)), spans);
    }

    #[test]
    fn spans_across_elements_cover_middles_whole() {
        let spans = resolve_spans(&elems(), (0, 6), (2, 5));
        assert_eq!(spans.len(), 3);
        assert_eq!(&spans[0].text[spans[0].range.clone()], "paragraph");
        assert_eq!(&spans[1].text[spans[1].range.clone()], "second");
        assert_eq!(&spans[2].text[spans[2].range.clone()], "third");
        assert_eq!(resolve_spans(&elems(), (2, 5), (0, 6)), spans);
    }

    #[test]
    fn drag_lifecycle_and_copy_joins() {
        let mut sel = SelectionState::default();
        sel.arm("p1", 6);
        assert!(sel.promote_pending());
        assert!(sel.is_dragging());
        let spans = resolve_spans(&elems(), (0, 6), (1, 6));
        assert!(sel.update_spans(spans.clone()));
        assert!(!sel.update_spans(spans));
        assert_eq!(sel.wash_range("p1"), Some(6..15));
        assert_eq!(sel.wash_range("p2"), Some(0..6));
        assert_eq!(sel.wash_range("p3"), None);
        assert_eq!(
            {
                sel.end_active_drag();
                sel.selected_text()
            }
            .as_deref(),
            Some("paragraph\nsecond")
        );
        assert_eq!(sel.selected_text().as_deref(), Some("paragraph\nsecond"));
        assert!(sel.has_selection());
        assert!(sel.clear());
        assert!(!sel.clear());
        assert_eq!(sel.selected_text(), None);
    }

    #[test]
    fn drag_survives_forward_virtualization() {
        let mut sel = SelectionState::default();
        sel.arm("p1", 6);
        assert!(sel.promote_pending());
        assert!(sel.update_drag(&elems(), (2, 5)));
        let shifted = [
            reg("p2", "second"),
            reg("p3", "third one"),
            reg("p4", "fourth"),
        ];
        assert!(sel.update_drag(&shifted, (2, 4)));
        assert_eq!(
            sel.selected_text().as_deref(),
            Some("paragraph\nsecond\nthird one\nfour")
        );
        assert_eq!(
            {
                sel.end_active_drag();
                sel.selected_text()
            }
            .as_deref(),
            Some("paragraph\nsecond\nthird one\nfour")
        );
        assert!(!sel.is_dragging());
    }

    #[test]
    fn drag_survives_backward_virtualization() {
        let mut sel = SelectionState::default();
        sel.arm("p5", 4);
        assert!(sel.promote_pending());
        let first = [reg("p3", "third"), reg("p4", "fourth"), reg("p5", "fifth")];
        assert!(sel.update_drag(&first, (0, 2)));
        let shifted = [reg("p2", "second"), reg("p3", "third"), reg("p4", "fourth")];
        assert!(sel.update_drag(&shifted, (0, 3)));
        assert_eq!(
            sel.selected_text().as_deref(),
            Some("ond\nthird\nfourth\nfift")
        );
        assert_eq!(
            {
                sel.end_active_drag();
                sel.selected_text()
            }
            .as_deref(),
            Some("ond\nthird\nfourth\nfift")
        );
    }

    #[test]
    fn virtualized_drag_requires_overlap() {
        let mut sel = SelectionState::default();
        sel.arm("p1", 6);
        assert!(sel.promote_pending());
        assert!(sel.update_drag(&elems(), (2, 5)));
        let unrelated = [reg("p8", "eighth"), reg("p9", "ninth")];
        assert!(!sel.update_drag(&unrelated, (1, 3)));
        assert_eq!(
            sel.selected_text().as_deref(),
            Some("paragraph\nsecond\nthird")
        );
    }

    #[test]
    fn virtualized_drag_waits_until_direction_is_known() {
        let mut sel = SelectionState::default();
        sel.arm("p1", 6);
        assert!(sel.promote_pending());
        let shifted = [reg("p2", "second"), reg("p3", "third")];
        assert!(!sel.update_drag(&shifted, (1, 3)));
        assert_eq!(sel.selected_text(), None);
    }

    #[test]
    fn empty_click_clears_on_release() {
        let mut sel = SelectionState::default();
        sel.arm("p1", 3);
        assert!(sel.promote_pending());
        sel.end_active_drag();
        assert_eq!(sel.selected_text(), None);
    }

    #[test]
    fn tap_does_not_select_until_drag() {
        let mut sel = SelectionState::default();
        sel.arm("p1", 3);
        assert!(sel.is_pending());
        assert!(!sel.has_selection());
        assert!(!sel.is_dragging());
        sel.cancel_pending();
        assert!(!sel.is_pending());
        assert_eq!(sel.selected_text(), None);
    }

    #[test]
    fn pending_press_promotes_on_drag() {
        let mut sel = SelectionState::default();
        sel.arm("p1", 6);
        assert!(sel.promote_pending());
        assert!(!sel.promote_pending());
        assert!(sel.is_dragging());
        let spans = resolve_spans(&elems(), (0, 6), (0, 15));
        assert!(sel.update_spans(spans));
        assert_eq!(
            {
                sel.end_active_drag();
                sel.selected_text()
            }
            .as_deref(),
            Some("paragraph")
        );
    }

    #[test]
    fn double_click_span() {
        let mut sel = SelectionState::default();
        sel.begin_with_span(&"p1".into(), &"hello world".into(), 6..11);
        assert_eq!(sel.wash_range("p1"), Some(6..11));
        assert_eq!(
            {
                sel.end_active_drag();
                sel.selected_text()
            }
            .as_deref(),
            Some("world")
        );
    }

    #[test]
    fn word_ranges() {
        let t = "let foo_bar = 12;";
        assert_eq!(word_range(t, 5), 4..11);
        assert_eq!(word_range(t, 4), 4..11);
        assert_eq!(word_range(t, 11), 4..11);
        assert_eq!(word_range(t, 15), 14..16);
        assert_eq!(&t[word_range(t, 12)], "=");
        assert_eq!(word_range(t, 3), 0..3);
        let u = "héllo wörld";
        assert_eq!(&u[word_range(u, 2)], "héllo");
    }

    /// A stale index past a shrunk element's text must clamp, not panic.
    #[test]
    fn resolve_spans_clamps_out_of_range_offsets() {
        let spans = resolve_spans(&[reg("a", "hé")], (0, 0), (0, 99));
        assert_eq!(&spans[0].text[spans[0].range.clone()], "hé");
    }

    /// Each logical line stays separate even when one native view paints them all.
    #[test]
    fn logical_texts_always_separate() {
        let elements = vec![reg("7:0", "let a = 1;"), reg("7:1", "let b = 2;")];
        let mut sel = SelectionState::default();
        sel.arm("7:0", 0);
        assert!(sel.promote_pending());
        assert!(sel.update_spans(resolve_spans(&elements, (0, 0), (1, 10))));
        assert_eq!(
            sel.selected_text().as_deref(),
            Some("let a = 1;\nlet b = 2;")
        );
    }
}
