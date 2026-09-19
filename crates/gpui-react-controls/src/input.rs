//! Native single-line and multiline text editors with platform IME support.
//!
//! The editor follows GPUI's input example:
//! https://github.com/zed-industries/zed/blob/main/crates/gpui/examples/input.rs
//! Caret blinking, double-click, drag autoscroll, and bounded undo follow
//! Comet's composer (MIT). Use Comet only as a generic editor behavior
//! reference; its composer contains app-specific code.
//! Upstream: https://github.com/zeronsh/comet/blob/main/crates/ui/src/composer.rs
//! Reviewed at: https://github.com/zeronsh/comet/blob/b3fa51872f70c8f973c241b659cf0c166766f4f5/crates/ui/src/composer.rs

use std::collections::VecDeque;
use std::ops::Range;
use std::time::Duration;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, DispatchPhase, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, GlobalElementId, KeyBinding, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, ScrollWheelEvent,
    SharedString, Style, Task, TextRun, TextStyle, UTF16Selection, UnderlineStyle, Window,
    WrappedLine, actions, div, fill, point, prelude::*, px, relative, size,
};
use unicode_segmentation::UnicodeSegmentation;
use web_time::Instant;

use crate::style::{Color, Style as ControlStyle};
use gpui_react::{ReactCommands, ReactEvents, ReactQueries, ReactView};
use serde::{Deserialize, Serialize};

actions!(
    gpui_react_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        Up,
        Down,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectAll,
        Home,
        End,
        DocStart,
        DocEnd,
        SelectHome,
        SelectEnd,
        SelectDocStart,
        SelectDocEnd,
        WordLeft,
        WordRight,
        SelectWordLeft,
        SelectWordRight,
        DeleteWordLeft,
        DeleteWordRight,
        DeleteToLineStart,
        DeleteToLineEnd,
        Copy,
        Cut,
        Paste,
        Undo,
        Redo,
        Newline,
        Submit,
    ]
);

const INPUT_KEY_CONTEXT: &str = "ReactInput";
const TEXTAREA_KEY_CONTEXT: &str = "ReactTextarea";
const TEXTAREA_SUBMIT_KEY_CONTEXT: &str = "ReactTextareaSubmit";
const CARET_BLINK_MS: u64 = 500;
const CARET_WIDTH: Pixels = px(2.0);
const CARET_HEIGHT_RATIO: f32 = 0.75;
const DRAG_SCROLL_FRAME_MS: u64 = 16;
const UNDO_COALESCE: Duration = Duration::from_millis(700);
const UNDO_LIMIT: usize = 200;

fn caret_visible(ms_since_activity: u64) -> bool {
    (ms_since_activity / CARET_BLINK_MS).is_multiple_of(2)
}

// Size the bar to cap height, not the line box. Default leading is phi, so a
// full-height caret sticks out above and below the glyphs. Cap height is about
// 0.75em; the em square itself still looks taller than the letters.
fn caret_rect(origin: Point<Pixels>, line_height: Pixels, font_size: Pixels) -> Bounds<Pixels> {
    let height = (font_size * CARET_HEIGHT_RATIO).min(line_height);
    let y_offset = (line_height - height) / 2.;
    Bounds::new(
        point(origin.x, origin.y + y_offset),
        size(CARET_WIDTH, height),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PressIntent {
    SelectAll,
    SelectWord,
    ExtendSelection,
    PlaceCaret,
}

impl PressIntent {
    fn arms_drag(self) -> bool {
        matches!(self, Self::ExtendSelection | Self::PlaceCaret)
    }
}

fn press_intent(click_count: usize, shift: bool) -> PressIntent {
    match click_count {
        n if n >= 3 => PressIntent::SelectAll,
        2 => PressIntent::SelectWord,
        _ if shift => PressIntent::ExtendSelection,
        _ => PressIntent::PlaceCaret,
    }
}

fn drag_scroll_delta(
    pointer_y: f32,
    viewport_top: f32,
    viewport_bottom: f32,
    line_height: f32,
) -> f32 {
    let distance = if pointer_y < viewport_top {
        pointer_y - viewport_top
    } else if pointer_y > viewport_bottom {
        pointer_y - viewport_bottom
    } else {
        return 0.0;
    };
    distance.signum() * (distance.abs() * 0.2).clamp(1.0, line_height)
}

fn utf16_offset_to_utf8(text: &str, offset: usize) -> usize {
    let mut utf8_offset = 0;
    let mut utf16_count = 0;
    for character in text.chars() {
        if utf16_count >= offset {
            break;
        }
        utf16_count += character.len_utf16();
        utf8_offset += character.len_utf8();
    }
    utf8_offset
}

fn single_line_text(text: &str) -> String {
    text.replace("\r\n", " ").replace(['\r', '\n'], " ")
}

struct Installed;
impl gpui::Global for Installed {}
fn init(cx: &mut App) {
    if cx.has_global::<Installed>() {
        return;
    }
    cx.set_global(Installed);
    let word_navigation_uses_alt = word_navigation_uses_alt();
    let bind_paste_shortcut = !cfg!(all(target_arch = "wasm32", target_os = "unknown"));
    let mut bindings = text_editor_bindings(
        INPUT_KEY_CONTEXT,
        false,
        true,
        word_navigation_uses_alt,
        bind_paste_shortcut,
    );
    bindings.extend(text_editor_bindings(
        TEXTAREA_KEY_CONTEXT,
        true,
        false,
        word_navigation_uses_alt,
        bind_paste_shortcut,
    ));
    bindings.extend(text_editor_bindings(
        TEXTAREA_SUBMIT_KEY_CONTEXT,
        true,
        true,
        word_navigation_uses_alt,
        bind_paste_shortcut,
    ));
    cx.bind_keys(bindings);
}

fn text_editor_bindings(
    context: &'static str,
    multiline: bool,
    enter_submits: bool,
    word_navigation_uses_alt: bool,
    bind_paste_shortcut: bool,
) -> Vec<KeyBinding> {
    let context = Some(context);
    let mut bindings = vec![
        if enter_submits {
            KeyBinding::new("enter", Submit, context)
        } else {
            KeyBinding::new("enter", Newline, context)
        },
        KeyBinding::new("shift-enter", Newline, context),
        KeyBinding::new("backspace", Backspace, context),
        KeyBinding::new("delete", Delete, context),
        KeyBinding::new("left", Left, context),
        KeyBinding::new("right", Right, context),
        KeyBinding::new("shift-left", SelectLeft, context),
        KeyBinding::new("shift-right", SelectRight, context),
        KeyBinding::new("home", Home, context),
        KeyBinding::new("end", End, context),
        KeyBinding::new("shift-home", SelectHome, context),
        KeyBinding::new("shift-end", SelectEnd, context),
        KeyBinding::new("cmd-left", Home, context),
        KeyBinding::new("cmd-right", End, context),
        KeyBinding::new("cmd-backspace", DeleteToLineStart, context),
        KeyBinding::new("cmd-delete", DeleteToLineEnd, context),
        KeyBinding::new("cmd-up", DocStart, context),
        KeyBinding::new("cmd-down", DocEnd, context),
        KeyBinding::new("shift-cmd-left", SelectHome, context),
        KeyBinding::new("shift-cmd-right", SelectEnd, context),
        KeyBinding::new("shift-cmd-up", SelectDocStart, context),
        KeyBinding::new("shift-cmd-down", SelectDocEnd, context),
    ];
    if multiline {
        bindings.extend([
            KeyBinding::new("up", Up, context),
            KeyBinding::new("down", Down, context),
            KeyBinding::new("shift-up", SelectUp, context),
            KeyBinding::new("shift-down", SelectDown, context),
        ]);
    }

    let word_prefix = if word_navigation_uses_alt {
        "alt"
    } else {
        "ctrl"
    };
    bindings.extend([
        KeyBinding::new(&format!("{word_prefix}-backspace"), DeleteWordLeft, context),
        KeyBinding::new(&format!("{word_prefix}-delete"), DeleteWordRight, context),
        KeyBinding::new(&format!("{word_prefix}-left"), WordLeft, context),
        KeyBinding::new(&format!("{word_prefix}-right"), WordRight, context),
        KeyBinding::new(
            &format!("shift-{word_prefix}-left"),
            SelectWordLeft,
            context,
        ),
        KeyBinding::new(
            &format!("shift-{word_prefix}-right"),
            SelectWordRight,
            context,
        ),
    ]);
    for prefix in ["cmd", "ctrl"] {
        bindings.extend([
            KeyBinding::new(&format!("{prefix}-a"), SelectAll, context),
            KeyBinding::new(&format!("{prefix}-c"), Copy, context),
            KeyBinding::new(&format!("{prefix}-x"), Cut, context),
            KeyBinding::new(&format!("{prefix}-z"), Undo, context),
            KeyBinding::new(&format!("shift-{prefix}-z"), Redo, context),
        ]);
        if bind_paste_shortcut {
            bindings.push(KeyBinding::new(&format!("{prefix}-v"), Paste, context));
        }
    }
    bindings
}

fn word_navigation_uses_alt() -> bool {
    cfg!(target_os = "macos")
}

#[derive(Clone)]
struct EditSnapshot {
    content: String,
    selected_range: Range<usize>,
    selection_reversed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditKind {
    Insert,
    DeleteBackward,
    DeleteForward,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CoalescingEdit {
    kind: EditKind,
    anchor: usize,
}

#[derive(Clone, Copy)]
struct LastEdit {
    edit: CoalescingEdit,
    when: Instant,
}

fn coalescing_edit(
    range: &Range<usize>,
    new_text: &str,
    selection_reversed: bool,
) -> Option<CoalescingEdit> {
    if new_text.is_empty() {
        if range.is_empty() {
            return None;
        }
        return Some(CoalescingEdit {
            kind: if selection_reversed {
                EditKind::DeleteBackward
            } else {
                EditKind::DeleteForward
            },
            anchor: range.start,
        });
    }

    let mut characters = new_text.chars();
    let character = characters.next()?;
    (range.is_empty() && characters.next().is_none() && !character.is_whitespace()).then_some(
        CoalescingEdit {
            kind: EditKind::Insert,
            anchor: range.start + new_text.len(),
        },
    )
}

fn edits_coalesce(
    previous: CoalescingEdit,
    current: Option<CoalescingEdit>,
    range: &Range<usize>,
    elapsed: Duration,
) -> bool {
    let Some(current) = current else {
        return false;
    };
    if previous.kind != current.kind || elapsed >= UNDO_COALESCE {
        return false;
    }
    match current.kind {
        EditKind::Insert | EditKind::DeleteForward => range.start == previous.anchor,
        EditKind::DeleteBackward => range.end == previous.anchor,
    }
}

fn push_undo_snapshot(history: &mut VecDeque<EditSnapshot>, snapshot: EditSnapshot) {
    if history.len() == UNDO_LIMIT {
        history.pop_front();
    }
    history.push_back(snapshot);
}

pub struct Input {
    revision: u64,
    submit_on_enter: bool,
    style: ControlStyle,
    selection_color: gpui::Hsla,
    painted_bounds: Option<Bounds<Pixels>>,
    painted_revision: Option<u64>,
    focus_handle: FocusHandle,
    content: String,
    placeholder: SharedString,
    label: String,
    multiline: bool,
    read_only: bool,
    min_rows: usize,
    max_rows: usize,
    capture_keys: Vec<String>,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    is_selecting: bool,
    drag_position: Option<Point<Pixels>>,
    drag_generation: u64,
    drag_autoscroll_active: bool,
    scroll_top: f32,
    scroll_left: f32,
    follow_cursor: bool,
    last_lines: Vec<WrappedLine>,
    line_starts: Vec<usize>,
    last_bounds: Option<Bounds<Pixels>>,
    line_height: Pixels,
    font_size: Pixels,
    content_height: f32,
    content_width: f32,
    display_is_placeholder: bool,
    caret_color: gpui::Hsla,
    blink_anchor: Instant,
    blink_task: Option<Task<()>>,
    undo_stack: VecDeque<EditSnapshot>,
    redo_stack: Vec<EditSnapshot>,
    last_edit: Option<LastEdit>,
}

impl Input {
    fn reset_blink(&mut self, cx: &Context<Self>) {
        self.blink_anchor = cx.background_executor().now();
    }

    fn caret_shown(&mut self, window: &Window, cx: &mut Context<Self>) -> bool {
        if !self.focus_handle.is_focused(window) || !window.is_window_active() {
            self.blink_task = None;
            return false;
        }
        if self.blink_task.is_none() {
            self.reset_blink(cx);
            self.blink_task = Some(cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(CARET_BLINK_MS))
                        .await;
                    if this.update(cx, |_, cx| cx.notify()).is_err() {
                        break;
                    }
                }
            }));
        }
        caret_visible(
            cx.background_executor()
                .now()
                .duration_since(self.blink_anchor)
                .as_millis() as u64,
        )
    }

    fn snapshot(&self) -> EditSnapshot {
        EditSnapshot {
            content: self.content.clone(),
            selected_range: self.selected_range.clone(),
            selection_reversed: self.selection_reversed,
        }
    }

    fn emit_change(&mut self, cx: &mut Context<Self>) {
        self.revision += 1;
        cx.emit(InputEvent::Change {
            snapshot: self.current_snapshot(),
        });
    }
    fn emit_selection(&mut self, cx: &mut Context<Self>) {
        self.revision += 1;
        cx.emit(InputEvent::Selection {
            revision: self.revision,
            selection: self.selection(),
            composing: self.marked_range.is_some(),
        });
    }
    fn emit_submit(&self, cx: &mut Context<Self>) {
        cx.emit(InputEvent::Submit {
            value: self.content.clone(),
            revision: self.revision,
        });
    }
    fn restore(&mut self, snapshot: EditSnapshot, cx: &mut Context<Self>) {
        self.content = snapshot.content;
        self.selected_range = snapshot.selected_range;
        self.selection_reversed = snapshot.selection_reversed;
        self.marked_range = None;
        self.follow_cursor = true;
        self.last_edit = None;
        self.reset_blink(cx);
        self.emit_change(cx);
        cx.notify();
    }

    fn record_edit(&mut self, range: &Range<usize>, new_text: &str, now: Instant) {
        let current = coalescing_edit(range, new_text, self.selection_reversed);
        let mergeable = self.last_edit.is_some_and(|previous| {
            edits_coalesce(
                previous.edit,
                current,
                range,
                now.duration_since(previous.when),
            )
        });
        if !mergeable {
            let snapshot = self.snapshot();
            push_undo_snapshot(&mut self.undo_stack, snapshot);
        }
        self.redo_stack.clear();
        self.last_edit = current.map(|edit| LastEdit { edit, when: now });
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = offset.min(self.content.len());
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        self.follow_cursor = true;
        self.reset_blink(cx);
        self.emit_selection(cx);
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = offset.min(self.content.len());
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        self.follow_cursor = true;
        self.reset_blink(cx);
        self.emit_selection(cx);
        cx.notify();
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(index, _)| (index < offset).then_some(index))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(index, _)| (index > offset).then_some(index))
            .unwrap_or(self.content.len())
    }

    fn previous_word_boundary(&self, offset: usize) -> usize {
        self.content
            .split_word_bound_indices()
            .rev()
            .find_map(|(index, word)| (index < offset && !word.trim().is_empty()).then_some(index))
            .unwrap_or(0)
    }

    fn next_word_boundary(&self, offset: usize) -> usize {
        self.content
            .split_word_bound_indices()
            .find_map(|(index, word)| {
                let end = index + word.len();
                (end > offset && !word.trim().is_empty()).then_some(end)
            })
            .unwrap_or(self.content.len())
    }

    fn line_range_at(&self, offset: usize) -> Range<usize> {
        let start = self.content[..offset]
            .rfind('\n')
            .map(|index| index + 1)
            .unwrap_or(0);
        let end = self.content[offset..]
            .find('\n')
            .map(|index| offset + index)
            .unwrap_or(self.content.len());
        start..end
    }

    fn visual_line_boundary(&self, end: bool) -> usize {
        let Some(cursor) = self.point_for_index(self.cursor_offset()) else {
            let line = self.line_range_at(self.cursor_offset());
            return if end { line.end } else { line.start };
        };
        self.index_for_point(point(
            if end { px(1_000_000.0) } else { px(0.0) },
            cursor.y + px(0.5),
        ))
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        if self.selected_range.is_empty() {
            let previous = self.previous_boundary(self.cursor_offset());
            if previous == self.cursor_offset() {
                return;
            }
            self.select_to(previous, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        if self.selected_range.is_empty() {
            let next = self.next_boundary(self.cursor_offset());
            if next == self.cursor_offset() {
                return;
            }
            self.select_to(next, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        let offset = if self.selected_range.is_empty() {
            self.previous_boundary(self.cursor_offset())
        } else {
            self.selected_range.start
        };
        self.move_to(offset, cx);
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        let offset = if self.selected_range.is_empty() {
            self.next_boundary(self.cursor_offset())
        } else {
            self.selected_range.end
        };
        self.move_to(offset, cx);
    }

    fn capture_navigation_key(&self, key: &str, cx: &mut Context<Self>) -> bool {
        if self.marked_range.is_some() || !self.capture_keys.iter().any(|k| k == key) {
            return false;
        }
        cx.emit(InputEvent::Key { key: key.into() });
        true
    }
    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        if self.capture_navigation_key("up", cx) {
            return;
        }
        if let Some(offset) = self.vertical_target(-1.0) {
            self.move_to(offset, cx);
        }
    }

    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        if self.capture_navigation_key("down", cx) {
            return;
        }
        if let Some(offset) = self.vertical_target(1.0) {
            self.move_to(offset, cx);
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(offset) = self.vertical_target(-1.0) {
            self.select_to(offset, cx);
        }
    }

    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(offset) = self.vertical_target(1.0) {
            self.select_to(offset, cx);
        }
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.selected_range = 0..self.content.len();
        self.selection_reversed = false;
        self.reset_blink(cx);
        cx.notify();
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.visual_line_boundary(false), cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.visual_line_boundary(true), cx);
    }

    fn doc_start(&mut self, _: &DocStart, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn doc_end(&mut self, _: &DocEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn select_home(&mut self, _: &SelectHome, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.visual_line_boundary(false), cx);
    }

    fn select_end(&mut self, _: &SelectEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.visual_line_boundary(true), cx);
    }

    fn select_doc_start(&mut self, _: &SelectDocStart, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(0, cx);
    }

    fn select_doc_end(&mut self, _: &SelectDocEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.content.len(), cx);
    }

    fn word_left(&mut self, _: &WordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.previous_word_boundary(self.cursor_offset()), cx);
    }

    fn word_right(&mut self, _: &WordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.next_word_boundary(self.cursor_offset()), cx);
    }

    fn select_word_left(&mut self, _: &SelectWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_word_boundary(self.cursor_offset()), cx);
    }

    fn select_word_right(&mut self, _: &SelectWordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_word_boundary(self.cursor_offset()), cx);
    }

    fn delete_word_left(
        &mut self,
        _: &DeleteWordLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        if self.selected_range.is_empty() {
            self.select_to(self.previous_word_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete_word_right(
        &mut self,
        _: &DeleteWordRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        if self.selected_range.is_empty() {
            self.select_to(self.next_word_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete_to_line_start(
        &mut self,
        _: &DeleteToLineStart,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        if self.selected_range.is_empty() {
            let start = self.line_range_at(self.cursor_offset()).start;
            if start == self.cursor_offset() {
                return;
            }
            self.select_to(start, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete_to_line_end(
        &mut self,
        _: &DeleteToLineEnd,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        if self.selected_range.is_empty() {
            let end = self.line_range_at(self.cursor_offset()).end;
            if end == self.cursor_offset() {
                return;
            }
            self.select_to(end, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only || self.selected_range.is_empty() {
            return;
        }
        self.copy(&Copy, window, cx);
        self.replace_text_in_range(None, "", window, cx);
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        if let Some(previous) = self.undo_stack.pop_back() {
            self.redo_stack.push(self.snapshot());
            self.restore(previous, cx);
        }
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        if let Some(next) = self.redo_stack.pop() {
            let snapshot = self.snapshot();
            push_undo_snapshot(&mut self.undo_stack, snapshot);
            self.restore(next, cx);
        }
    }

    fn newline(&mut self, _: &Newline, window: &mut Window, cx: &mut Context<Self>) {
        if self.multiline && !self.read_only {
            self.replace_text_in_range(None, "\n", window, cx);
        }
    }

    fn submit(&mut self, _: &Submit, _: &mut Window, cx: &mut Context<Self>) {
        self.emit_submit(cx);
    }

    fn vertical_target(&self, direction: f32) -> Option<usize> {
        let current = self.point_for_index(self.cursor_offset())?;
        let target_y = f32::from(current.y) + direction * f32::from(self.line_height);
        if target_y < 0.0 {
            return Some(0);
        }
        if target_y >= self.content_height {
            return Some(self.content.len());
        }
        Some(self.index_for_point(point(current.x, px(target_y))))
    }

    fn point_for_index(&self, index: usize) -> Option<Point<Pixels>> {
        for (line_index, line) in self.last_lines.iter().enumerate() {
            let line_start = *self.line_starts.get(line_index)?;
            if index < line_start || index > line_start + line.len() {
                continue;
            }
            let local = line.position_for_index(index - line_start, self.line_height)?;
            let y_offset: Pixels = self
                .last_lines
                .iter()
                .take(line_index)
                .map(|line| line.size(self.line_height).height)
                .sum();
            return Some(point(local.x, local.y + y_offset));
        }
        None
    }

    fn index_for_point(&self, position: Point<Pixels>) -> usize {
        if self.display_is_placeholder {
            return 0;
        }
        let mut y = f32::from(position.y).max(0.0);
        for (line_index, line) in self.last_lines.iter().enumerate() {
            let height = f32::from(line.size(self.line_height).height);
            let line_start = self.line_starts.get(line_index).copied().unwrap_or(0);
            if y < height || line_index + 1 == self.last_lines.len() {
                let local = point(position.x, px(y.min(height - 1.0).max(0.0)));
                let index = line
                    .closest_index_for_position(local, self.line_height)
                    .unwrap_or_else(|index| index);
                return (line_start + index).min(self.content.len());
            }
            y -= height;
        }
        self.content.len()
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        let Some(bounds) = self.last_bounds else {
            return 0;
        };
        self.index_for_point(point(
            position.x - bounds.left() + px(self.scroll_left),
            position.y - bounds.top() + px(self.scroll_top),
        ))
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.read_only {
            window.request_text_input();
        }
        window.focus(&self.focus_handle, cx);
        let intent = press_intent(event.click_count, event.modifiers.shift);
        self.is_selecting = intent.arms_drag();
        self.drag_position = intent.arms_drag().then_some(event.position);
        self.drag_generation = self.drag_generation.wrapping_add(1);
        self.drag_autoscroll_active = false;
        match intent {
            PressIntent::SelectAll => {
                self.move_to(0, cx);
                self.select_to(self.content.len(), cx);
            }
            PressIntent::SelectWord => {
                let index = self.index_for_mouse_position(event.position);
                let range = word_range(&self.content, index);
                self.move_to(range.start, cx);
                self.select_to(range.end, cx);
            }
            PressIntent::ExtendSelection => {
                self.select_to(self.index_for_mouse_position(event.position), cx);
            }
            PressIntent::PlaceCaret => {
                self.move_to(self.index_for_mouse_position(event.position), cx);
            }
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
        self.drag_position = None;
        self.drag_generation = self.drag_generation.wrapping_add(1);
        self.drag_autoscroll_active = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.drag_position = Some(event.position);
            let position = self.drag_selection_position(event.position);
            self.select_to(self.index_for_mouse_position(position), cx);
            if self.multiline
                && self.drag_scroll_delta(event.position) != 0.0
                && !self.drag_autoscroll_active
            {
                self.start_drag_autoscroll(cx);
            }
        }
    }

    fn start_drag_autoscroll(&mut self, cx: &mut Context<Self>) {
        self.drag_autoscroll_active = true;
        let generation = self.drag_generation;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(DRAG_SCROLL_FRAME_MS))
                    .await;
                let keep_running = this
                    .update(cx, |input, cx| input.drag_autoscroll_tick(generation, cx))
                    .unwrap_or(false);
                if !keep_running {
                    break;
                }
            }
        })
        .detach();
    }

    fn drag_selection_position(&self, position: Point<Pixels>) -> Point<Pixels> {
        let Some(bounds) = self.last_bounds else {
            return position;
        };
        let x = if self.multiline {
            position.x.clamp(bounds.left(), bounds.right() - px(0.5))
        } else {
            position.x
        };
        point(x, position.y.clamp(bounds.top(), bounds.bottom() - px(0.5)))
    }

    fn drag_scroll_delta(&self, position: Point<Pixels>) -> f32 {
        let Some(bounds) = self.last_bounds else {
            return 0.0;
        };
        drag_scroll_delta(
            f32::from(position.y),
            f32::from(bounds.top()),
            f32::from(bounds.bottom()),
            f32::from(self.line_height),
        )
    }

    fn drag_autoscroll_tick(&mut self, generation: u64, cx: &mut Context<Self>) -> bool {
        if !self.multiline || !self.is_selecting || self.drag_generation != generation {
            return false;
        }
        let (Some(position), Some(bounds)) = (self.drag_position, self.last_bounds) else {
            self.drag_autoscroll_active = false;
            return false;
        };
        let delta = self.drag_scroll_delta(position);
        if delta == 0.0 {
            self.drag_autoscroll_active = false;
            return false;
        }
        let max_scroll = (self.content_height - f32::from(bounds.size.height)).max(0.0);
        let next = (self.scroll_top + delta).clamp(0.0, max_scroll);
        if next == self.scroll_top {
            self.drag_autoscroll_active = false;
            return false;
        }
        self.scroll_top = next;
        let edge_position = self.drag_selection_position(position);
        self.select_to(self.index_for_mouse_position(edge_position), cx);
        self.follow_cursor = false;
        true
    }

    fn on_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.default_prevented() {
            return;
        }
        let Some(bounds) = self.last_bounds else {
            return;
        };
        let viewport_height = f32::from(bounds.size.height);
        let max_scroll = (self.content_height - viewport_height).max(0.0);
        if max_scroll == 0.0 {
            return;
        }
        let delta = f32::from(event.delta.pixel_delta(self.line_height).y);
        let next = (self.scroll_top - delta).clamp(0.0, max_scroll);
        if next == self.scroll_top {
            return;
        }
        self.scroll_top = next;
        self.follow_cursor = false;
        window.prevent_default();
        cx.notify();
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;
        for character in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += character.len_utf16();
            utf8_offset += character.len_utf8();
        }
        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;
        for character in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += character.len_utf8();
            utf16_offset += character.len_utf16();
        }
        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }

    fn layout_text(&mut self, width: Pixels, style: &TextStyle, window: &mut Window) -> f32 {
        let (display, is_placeholder) = if self.content.is_empty() {
            (self.placeholder.clone(), true)
        } else {
            (SharedString::from(self.content.clone()), false)
        };
        let rem_size = window.rem_size();
        let font_size = style.font_size.to_pixels(rem_size);
        self.font_size = font_size;
        // Taffy measures after parent `with_text_style` is gone, so
        // `window.line_height()` is always 16×φ. Use the style captured during
        // request_layout instead.
        self.line_height = style.line_height_in_pixels(rem_size);
        let color = if is_placeholder {
            gpui::rgba(0x8f8f8fff).into()
        } else {
            style.color
        };
        let run = |len: usize, underline: bool| TextRun {
            len,
            font: style.font(),
            color,
            background_color: None,
            underline: underline.then_some(UnderlineStyle {
                color: Some(color),
                thickness: px(1.0),
                wavy: false,
            }),
            strikethrough: None,
        };
        let runs = match self.marked_range.as_ref() {
            Some(marked) if !is_placeholder => vec![
                run(marked.start, false),
                run(marked.len(), true),
                run(display.len() - marked.end, false),
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect(),
            _ => vec![run(display.len(), false)],
        };
        let wrap_width = self.multiline.then_some(width);
        let lines = window
            .text_system()
            .shape_text(display, font_size, &runs, wrap_width, None)
            .map(|lines| lines.into_vec())
            .unwrap_or_default();
        let mut line_starts = Vec::with_capacity(lines.len());
        let mut offset = 0;
        for line in &lines {
            line_starts.push(offset);
            offset += line.len() + 1;
        }
        if line_starts.is_empty() {
            line_starts.push(0);
        }
        self.content_height = lines
            .iter()
            .map(|line| f32::from(line.size(self.line_height).height))
            .sum::<f32>()
            .max(f32::from(self.line_height));
        self.content_width = lines
            .iter()
            .map(|line| f32::from(line.unwrapped_layout.width))
            .fold(0.0, f32::max);
        self.display_is_placeholder = is_placeholder;
        self.last_lines = lines;
        self.line_starts = line_starts;
        self.content_height
    }

    fn clamp_scroll(&mut self, viewport_width: f32, viewport_height: f32) {
        if self.follow_cursor
            && let Some(cursor) = self.point_for_index(self.cursor_offset())
        {
            let cursor_top = f32::from(cursor.y);
            if cursor_top < self.scroll_top {
                self.scroll_top = cursor_top;
            } else if cursor_top + f32::from(self.line_height) > self.scroll_top + viewport_height {
                self.scroll_top = cursor_top + f32::from(self.line_height) - viewport_height;
            }
            if !self.multiline {
                let cursor_left = f32::from(cursor.x);
                if cursor_left < self.scroll_left {
                    self.scroll_left = cursor_left;
                } else if cursor_left + 2.0 > self.scroll_left + viewport_width {
                    self.scroll_left = cursor_left + 2.0 - viewport_width;
                }
            }
        }
        self.scroll_top = self
            .scroll_top
            .clamp(0.0, (self.content_height - viewport_height).max(0.0));
        self.scroll_left = if self.multiline {
            0.0
        } else {
            self.scroll_left
                .clamp(0.0, (self.content_width + 2.0 - viewport_width).max(0.0))
        };
    }
}

impl EntityInputHandler for Input {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        self.content.get(range).map(str::to_string)
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked_range = None;
        self.emit_selection(cx);
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        let replacement = if self.multiline {
            new_text.to_string()
        } else {
            single_line_text(new_text)
        };
        if self.marked_range.is_none() {
            self.record_edit(&range, &replacement, cx.background_executor().now());
        }
        self.content =
            self.content[..range.start].to_owned() + &replacement + &self.content[range.end..];
        let cursor = range.start + replacement.len();
        self.selected_range = cursor..cursor;
        self.selection_reversed = false;
        self.marked_range = None;
        self.follow_cursor = true;
        self.reset_blink(cx);
        self.emit_change(cx);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        if self.marked_range.is_none() {
            let snapshot = self.snapshot();
            push_undo_snapshot(&mut self.undo_stack, snapshot);
            self.redo_stack.clear();
            self.last_edit = None;
        }
        let replacement = if self.multiline {
            new_text.to_string()
        } else {
            single_line_text(new_text)
        };
        self.content =
            self.content[..range.start].to_owned() + &replacement + &self.content[range.end..];
        self.marked_range =
            (!replacement.is_empty()).then_some(range.start..range.start + replacement.len());
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|selected| {
                range.start + utf16_offset_to_utf8(&replacement, selected.start)
                    ..range.start + utf16_offset_to_utf8(&replacement, selected.end)
            })
            .unwrap_or_else(|| range.start + replacement.len()..range.start + replacement.len());
        self.follow_cursor = true;
        self.reset_blink(cx);
        self.emit_change(cx);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range_utf16);
        let start = self.point_for_index(range.start)?;
        Some(caret_rect(
            point(
                bounds.left() + start.x - px(self.scroll_left),
                bounds.top() + start.y - px(self.scroll_top),
            ),
            self.line_height,
            self.font_size,
        ))
    }

    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.offset_to_utf16(self.index_for_mouse_position(position)))
    }

    fn set_selected_text_range(
        &mut self,
        range_utf16: Range<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selected_range = self.range_from_utf16(&range_utf16);
        self.selection_reversed = false;
        self.follow_cursor = true;
        self.reset_blink(cx);
        self.emit_selection(cx);
        cx.notify();
    }

    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        Some(self.content.encode_utf16().count())
    }

    fn accepts_text_input(&self, _: &mut Window, _: &mut Context<Self>) -> bool {
        !self.read_only
    }
}

impl gpui::Render for Input {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let editor = div()
            .id("input")
            .block_mouse_except_scroll()
            .role(if self.multiline {
                gpui::Role::MultilineTextInput
            } else {
                gpui::Role::TextInput
            })
            .aria_label(self.label.clone())
            .aria_value(self.content.clone())
            .aria_placeholder(self.placeholder.clone())
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if this.marked_range.is_none()
                    && event.keystroke.modifiers == gpui::Modifiers::default()
                    && this.capture_keys.contains(&event.keystroke.key)
                {
                    cx.emit(InputEvent::Key {
                        key: event.keystroke.key.clone(),
                    });
                    window.prevent_default();
                    cx.stop_propagation();
                }
            }))
            .key_context(if !self.multiline {
                INPUT_KEY_CONTEXT
            } else if self.submit_on_enter {
                TEXTAREA_SUBMIT_KEY_CONTEXT
            } else {
                TEXTAREA_KEY_CONTEXT
            })
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::doc_start))
            .on_action(cx.listener(Self::doc_end))
            .on_action(cx.listener(Self::select_home))
            .on_action(cx.listener(Self::select_end))
            .on_action(cx.listener(Self::select_doc_start))
            .on_action(cx.listener(Self::select_doc_end))
            .on_action(cx.listener(Self::word_left))
            .on_action(cx.listener(Self::word_right))
            .on_action(cx.listener(Self::select_word_left))
            .on_action(cx.listener(Self::select_word_right))
            .on_action(cx.listener(Self::delete_word_left))
            .on_action(cx.listener(Self::delete_word_right))
            .on_action(cx.listener(Self::delete_to_line_start))
            .on_action(cx.listener(Self::delete_to_line_end))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::newline))
            .on_action(cx.listener(Self::submit))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_scroll_wheel(cx.listener(Self::on_scroll_wheel))
            .w_full()
            .min_w_0()
            .child(EditorTextElement {
                input: cx.entity(),
                min_rows: self.min_rows,
                max_rows: self.max_rows,
            });
        self.style.apply_interactive(editor).overflow_hidden()
    }
}

struct EditorTextElement {
    input: Entity<Input>,
    min_rows: usize,
    max_rows: usize,
}

struct EditorPrepaint {
    caret: Option<PaintQuad>,
    selection: Vec<PaintQuad>,
}

impl gpui::Element for EditorTextElement {
    type RequestLayoutState = ();
    type PrepaintState = EditorPrepaint;

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        _: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        let input = self.input.clone();
        let text_style = window.text_style();
        let min_rows = self.min_rows;
        let max_rows = self.max_rows.max(min_rows);
        let layout = window.request_measured_layout(style, move |known, available, window, cx| {
            let width = known.width.unwrap_or(match available.width {
                gpui::AvailableSpace::Definite(width) => width,
                _ => px(320.0),
            });
            let (content_height, line_height) = input.update(cx, |input, _| {
                let content_height = input.layout_text(width, &text_style, window);
                (content_height, f32::from(input.line_height))
            });
            let height =
                content_height.clamp(min_rows as f32 * line_height, max_rows as f32 * line_height);
            size(width, px(height))
        });
        (layout, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut Window,
        cx: &mut App,
    ) -> EditorPrepaint {
        self.input.update(cx, |input, _| {
            input.last_bounds = Some(bounds);
            input.clamp_scroll(f32::from(bounds.size.width), f32::from(bounds.size.height));
        });
        let input = self.input.read(cx);
        let origin = point(
            bounds.left() - px(input.scroll_left),
            bounds.top() - px(input.scroll_top),
        );
        let mut selection = Vec::new();
        let mut caret = None;
        if input.selected_range.is_empty() || input.display_is_placeholder {
            let caret_point = input
                .point_for_index(input.cursor_offset())
                .unwrap_or(point(px(0.0), px(0.0)));
            caret = Some(fill(
                caret_rect(
                    point(origin.x + caret_point.x, origin.y + caret_point.y),
                    input.line_height,
                    input.font_size,
                ),
                input.caret_color,
            ));
        } else if let (Some(start), Some(end)) = (
            input.point_for_index(input.selected_range.start),
            input.point_for_index(input.selected_range.end),
        ) {
            let color = input.selection_color;
            if start.y == end.y {
                selection.push(fill(
                    Bounds::from_corners(
                        point(origin.x + start.x, origin.y + start.y),
                        point(origin.x + end.x, origin.y + start.y + input.line_height),
                    ),
                    color,
                ));
            } else {
                selection.push(fill(
                    Bounds::from_corners(
                        point(origin.x + start.x, origin.y + start.y),
                        point(bounds.right(), origin.y + start.y + input.line_height),
                    ),
                    color,
                ));
                if end.y > start.y + input.line_height {
                    selection.push(fill(
                        Bounds::from_corners(
                            point(origin.x, origin.y + start.y + input.line_height),
                            point(bounds.right(), origin.y + end.y),
                        ),
                        color,
                    ));
                }
                selection.push(fill(
                    Bounds::from_corners(
                        point(origin.x, origin.y + end.y),
                        point(origin.x + end.x, origin.y + end.y + input.line_height),
                    ),
                    color,
                ));
            }
        }
        EditorPrepaint { caret, selection }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        prepaint: &mut EditorPrepaint,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        let input = self.input.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
            if phase == DispatchPhase::Bubble && event.pressed_button == Some(MouseButton::Left) {
                input.update(cx, |input, cx| input.on_mouse_move(event, cx));
            }
        });
        window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
            for quad in prepaint.selection.drain(..) {
                window.paint_quad(quad);
            }
            let (lines, line_height, scroll_top, scroll_left) =
                self.input.update(cx, |input, _| {
                    input.painted_bounds = Some(bounds);
                    input.painted_revision = Some(input.revision);
                    (
                        std::mem::take(&mut input.last_lines),
                        input.line_height,
                        input.scroll_top,
                        input.scroll_left,
                    )
                });
            let mut y = bounds.top() - px(scroll_top);
            for line in &lines {
                let height = line.size(line_height).height;
                line.paint(
                    point(bounds.left() - px(scroll_left), y),
                    line_height,
                    gpui::TextAlign::Left,
                    Some(bounds),
                    window,
                    cx,
                )
                .ok();
                y += height;
            }
            self.input.update(cx, |input, _| input.last_lines = lines);
            let caret_shown = self
                .input
                .update(cx, |input, cx| input.caret_shown(window, cx));
            if caret_shown && let Some(caret) = prepaint.caret.take() {
                window.paint_quad(caret);
            }
        });
    }
}

impl gpui::IntoElement for EditorTextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

pub fn word_range(text: &str, ix: usize) -> Range<usize> {
    let mut ix = ix.min(text.len());
    while !text.is_char_boundary(ix) {
        ix -= 1;
    }
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let before = text[..ix].chars().next_back();
    let at = text[ix..].chars().next();
    if !at.is_some_and(is_word) && !before.is_some_and(is_word) {
        return match at {
            Some(c) if !c.is_whitespace() => ix..ix + c.len_utf8(),
            _ => ix..ix,
        };
    }
    let start = text[..ix]
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_word(*c))
        .last()
        .map(|(i, _)| i)
        .unwrap_or(ix);
    let end = text[ix..]
        .char_indices()
        .take_while(|(_, c)| is_word(*c))
        .last()
        .map(|(i, c)| ix + i + c.len_utf8())
        .unwrap_or(ix);
    start..end
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct InputProps {
    /// Read only at construction. Later props never echo text into the native buffer.
    pub initial_value: String,
    /// Editing mode is fixed for this mounted input. Remount to change it.
    pub initial_multiline: bool,
    pub placeholder: String,
    pub label: String,
    pub read_only: bool,
    pub min_rows: Option<usize>,
    pub max_rows: Option<usize>,
    pub submit_on_enter: bool,
    /// Native key interception, for example while an autocomplete menu is open.
    pub capture_keys: Vec<String>,
    pub style: ControlStyle,
    pub caret_color: Option<Color>,
    pub selection_color: Option<Color>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selection {
    /// UTF-16 offsets, as used by platform input methods and JavaScript strings.
    pub start: usize,
    pub end: usize,
    #[serde(default)]
    pub reversed: bool,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaintedBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub revision: u64,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputSnapshot {
    pub value: String,
    pub revision: u64,
    pub selection: Selection,
    pub composing: bool,
    /// Geometry from the last paint, which can be older than `revision`.
    pub painted: Option<PaintedBounds>,
}
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum InputEvent {
    Change {
        snapshot: InputSnapshot,
    },
    Selection {
        revision: u64,
        selection: Selection,
        composing: bool,
    },
    Submit {
        value: String,
        revision: u64,
    },
    Key {
        key: String,
    },
}
#[derive(Debug, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum InputCommand {
    Focus,
    Blur,
    /// Compare-and-replace: no text update if typing/selection/composition has advanced.
    Replace {
        value: String,
        expected_revision: u64,
    },
    Select {
        selection: Selection,
        expected_revision: u64,
    },
}

impl Input {
    pub fn new(props: InputProps, cx: &mut Context<Self>) -> Self {
        init(cx);
        let content = if props.initial_multiline {
            props.initial_value.clone()
        } else {
            single_line_text(&props.initial_value)
        };
        let end = content.len();
        let mut input = Self {
            revision: 0,
            submit_on_enter: false,
            style: ControlStyle::default(),
            selection_color: gpui::rgba(0x7c86ff59).into(),
            painted_bounds: None,
            painted_revision: None,
            focus_handle: cx.focus_handle(),
            content,
            placeholder: "".into(),
            label: String::new(),
            multiline: props.initial_multiline,
            read_only: false,
            min_rows: 1,
            max_rows: 1,
            capture_keys: Vec::new(),
            selected_range: end..end,
            selection_reversed: false,
            marked_range: None,
            is_selecting: false,
            drag_position: None,
            drag_generation: 0,
            drag_autoscroll_active: false,
            scroll_top: 0.,
            scroll_left: 0.,
            follow_cursor: true,
            last_lines: Vec::new(),
            line_starts: vec![0],
            last_bounds: None,
            line_height: px(0.),
            font_size: px(0.),
            content_height: 0.,
            content_width: 0.,
            display_is_placeholder: false,
            caret_color: gpui::rgb(0xffffff).into(),
            blink_anchor: cx.background_executor().now(),
            blink_task: None,
            undo_stack: VecDeque::new(),
            redo_stack: Vec::new(),
            last_edit: None,
        };
        input.update_props(props, cx);
        input
    }
    pub fn update_props(&mut self, props: InputProps, cx: &mut Context<Self>) {
        self.placeholder = props.placeholder.into();
        self.label = props.label;
        self.read_only = props.read_only;
        self.submit_on_enter = props.submit_on_enter;
        self.min_rows = if self.multiline {
            props.min_rows.unwrap_or(1).max(1)
        } else {
            1
        };
        self.max_rows = if self.multiline {
            props.max_rows.unwrap_or(10).max(self.min_rows)
        } else {
            1
        };
        self.capture_keys = props.capture_keys;
        self.style = props.style;
        self.caret_color = props
            .caret_color
            .map(|c| c.0)
            .unwrap_or(gpui::rgb(0xffffff).into());
        self.selection_color = props
            .selection_color
            .map(|c| c.0)
            .unwrap_or(gpui::rgba(0x7c86ff59).into());
        cx.notify();
    }
    fn selection(&self) -> Selection {
        let range = self.range_to_utf16(&self.selected_range);
        Selection {
            start: range.start,
            end: range.end,
            reversed: self.selection_reversed,
        }
    }
    pub fn current_snapshot(&self) -> InputSnapshot {
        InputSnapshot {
            value: self.content.clone(),
            revision: self.revision,
            selection: self.selection(),
            composing: self.marked_range.is_some(),
            painted: self
                .painted_bounds
                .zip(self.painted_revision)
                .map(|(b, revision)| PaintedBounds {
                    x: b.left().into(),
                    y: b.top().into(),
                    width: b.size.width.into(),
                    height: b.size.height.into(),
                    revision,
                }),
        }
    }
    fn check_revision(&self, expected: u64) -> anyhow::Result<()> {
        anyhow::ensure!(
            expected == self.revision,
            "stale input revision: expected {expected}, current {}",
            self.revision
        );
        anyhow::ensure!(self.marked_range.is_none(), "input composition is active");
        Ok(())
    }
    pub fn apply_command(
        &mut self,
        command: InputCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        match command {
            InputCommand::Focus => window.focus(&self.focus_handle, cx),
            InputCommand::Blur => {
                if self.focus_handle.is_focused(window) {
                    window.blur();
                }
            }
            InputCommand::Replace {
                value,
                expected_revision,
            } => {
                self.check_revision(expected_revision)?;
                let value = if self.multiline {
                    value
                } else {
                    single_line_text(&value)
                };
                if self.content != value {
                    let snapshot = self.snapshot();
                    push_undo_snapshot(&mut self.undo_stack, snapshot);
                    self.redo_stack.clear();
                    self.last_edit = None;
                    self.content = value;
                    let end = self.content.len();
                    self.selected_range = end..end;
                    self.selection_reversed = false;
                    self.follow_cursor = true;
                    self.reset_blink(cx);
                    self.emit_change(cx);
                    cx.notify();
                }
            }
            InputCommand::Select {
                selection,
                expected_revision,
            } => {
                self.check_revision(expected_revision)?;
                anyhow::ensure!(
                    selection.start <= selection.end,
                    "selection start exceeds end"
                );
                let range = self.range_from_utf16(&(selection.start..selection.end));
                anyhow::ensure!(
                    self.range_to_utf16(&range) == (selection.start..selection.end),
                    "selection is outside text or splits a UTF-16 surrogate pair"
                );
                self.selected_range = range;
                self.selection_reversed = selection.reversed;
                self.follow_cursor = true;
                self.reset_blink(cx);
                self.emit_selection(cx);
                cx.notify();
            }
        }
        Ok(())
    }
}
impl gpui::Focusable for Input {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
impl gpui::EventEmitter<InputEvent> for Input {}
impl ReactView for Input {
    type Props = InputProps;
    fn create(props: InputProps, _: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new(props, cx)
    }
    fn set_props(&mut self, props: InputProps, _: &mut Window, cx: &mut Context<Self>) {
        self.update_props(props, cx);
    }
    fn unmounting(&mut self, window: &mut Window, _: &mut Context<Self>) {
        self.blink_task = None;
        self.is_selecting = false;
        self.drag_generation = self.drag_generation.wrapping_add(1);
        if self.focus_handle.is_focused(window) {
            window.blur();
        }
    }
}
impl ReactEvents for Input {
    type Event = InputEvent;
}
impl ReactCommands for Input {
    type Command = InputCommand;
    fn command(
        &mut self,
        command: Self::Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        self.apply_command(command, window, cx)
    }
}
impl ReactQueries for Input {
    type Query = ();
    type Reply = InputSnapshot;
    fn query(
        &mut self,
        _: (),
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> anyhow::Result<InputSnapshot> {
        Ok(self.current_snapshot())
    }
}

#[cfg(test)]
#[path = "input_tests.rs"]
mod tests;
