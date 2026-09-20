//! Per-document text services over native paint, independent of the React host model.
mod geometry;
mod search;
mod selection;
use crate::{Color, Style, geometry::Rect};
use gpui::{prelude::*, *};
use gpui_react::{Children, ReactChildren, ReactCommands, ReactEvents, ReactQueries, ReactView, Shared};
pub use search::{Query as SearchQuery, Search};
use selection::{RegisteredText, SelectionState};
use serde::{Deserialize, Serialize};
use rustc_hash::FxHashMap;
use std::{fmt, ops::Range, rc::Rc, sync::Arc, time::Duration};

#[derive(Default, Deserialize, gpui_react::ComponentProps)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentProps {
    pub style: Shared<Style>,
    pub search: Option<Search>,
    pub selection_color: Option<Color>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Endpoint {
    pub key: String,
    pub offset: usize,
}
#[derive(Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum DocumentCommand {
    Clear,
    Copy,
    SelectAll,
    Select {
        start: Endpoint,
        end: Endpoint,
        expected_content_revision: u64,
    },
}
#[derive(Clone, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DocumentEvent {
    Selection {
        revision: u64,
        has_selection: bool,
    },
    Search {
        query: Option<SearchQuery>,
        frame: Option<gpui_react::FrameInfo>,
        content_revision: u64,
        count: usize,
        index_offset: usize,
    },
}
#[derive(Clone, Serialize)]
pub struct TextRange {
    pub key: String,
    pub start: usize,
    pub end: usize,
    pub rects: Vec<Rect>,
}
#[derive(Clone, Serialize)]
pub struct Highlight {
    pub index: usize,
    pub active: bool,
    #[serde(flatten)]
    pub range: TextRange,
}
#[derive(Clone, Serialize)]
pub struct PaintedText {
    pub key: String,
    pub text: String,
    pub bounds: Rect,
    pub selectable: bool,
    pub searchable: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSnapshot {
    pub text: Vec<PaintedText>,
    pub content_revision: u64,
    pub selection: Option<String>,
    pub selection_revision: u64,
    pub painted_selection_revision: u64,
    pub ranges: Vec<TextRange>,
    pub highlights: Vec<Highlight>,
    pub match_count: usize,
    pub match_index_offset: usize,
    pub query: Option<SearchQuery>,
    pub frame: Option<gpui_react::FrameInfo>,
}
/// Identity of one logical text inside a document. React text without an
/// app-supplied `textKey` is keyed by its host node id, which hashes as an
/// integer and allocates nothing. Named keys come from `textKey` props and
/// native components.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TextKey {
    Node(u32),
    Named(SharedString),
}
impl TextKey {
    /// Parse the string form used in snapshots and commands.
    pub fn parse(text: &str) -> Self {
        text.strip_prefix("text:")
            .and_then(|digits| digits.parse().ok())
            .map(TextKey::Node)
            .unwrap_or_else(|| TextKey::Named(text.to_owned().into()))
    }
}
impl fmt::Display for TextKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TextKey::Node(id) => write!(f, "text:{id}"),
            TextKey::Named(name) => f.write_str(name),
        }
    }
}
impl From<&str> for TextKey {
    fn from(name: &str) -> Self {
        TextKey::Named(name.to_owned().into())
    }
}
impl From<String> for TextKey {
    fn from(name: String) -> Self {
        TextKey::Named(name.into())
    }
}
impl From<SharedString> for TextKey {
    fn from(name: SharedString) -> Self {
        TextKey::Named(name)
    }
}
impl From<u32> for TextKey {
    fn from(id: u32) -> Self {
        TextKey::Node(id)
    }
}

/// Per-text options, 8 bytes: two flags and an optional match index offset
/// (`NO_OFFSET` when absent).
#[derive(Clone, Copy, PartialEq)]
struct TextOptions {
    match_index_offset: u32,
    flags: u8,
}
const SELECTABLE: u8 = 1;
const SEARCHABLE: u8 = 2;
const NO_OFFSET: u32 = u32::MAX;
impl Default for TextOptions {
    fn default() -> Self {
        Self {
            match_index_offset: NO_OFFSET,
            flags: SELECTABLE | SEARCHABLE,
        }
    }
}
impl TextOptions {
    fn selectable(&self) -> bool {
        self.flags & SELECTABLE != 0
    }
    fn searchable(&self) -> bool {
        self.flags & SEARCHABLE != 0
    }
    fn match_index_offset(&self) -> Option<usize> {
        (self.match_index_offset != NO_OFFSET).then_some(self.match_index_offset as usize)
    }
    fn set(&mut self, flag: u8, on: bool) {
        self.flags = if on { self.flags | flag } else { self.flags & !flag };
    }
}
struct Entry {
    key: TextKey,
    text: SharedString,
    geometry: geometry::Geometry,
    hitbox: HitboxId,
    clip: Bounds<Pixels>,
    options: TextOptions,
}
/// One record per painted text, kept across paints: 64 bytes. `matches` is
/// `None` when the text has no search hits, so most texts allocate nothing.
struct Cached {
    text: SharedString,
    matcher: Option<Arc<search::Matcher>>,
    matches: Option<Arc<[Range<usize>]>>,
    options: TextOptions,
    seen: u32,
    index: u32,
}
type ScrollCallback = Rc<dyn Fn(Pixels, &mut App) -> bool>;
struct ScrollArea {
    id: u64,
    bounds: Bounds<Pixels>,
    scroll: ScrollCallback,
}
/// Register a vertical scroll area during paint. Return whether the callback
/// moved its native scroll state. The document uses the normal GPUI state;
/// it neither copies offsets nor routes drag frames through JavaScript.
pub fn register_scroll_area(
    id: u64,
    bounds: Bounds<Pixels>,
    window: &Window,
    cx: &mut App,
    scroll: impl Fn(Pixels, &mut App) -> bool + 'static,
) {
    let owner = window
        .element_context::<ActiveDocument>()
        .map(|active| active.0.clone());
    if let Some(owner) = owner {
        let bounds = bounds.intersect(&window.content_mask().bounds);
        if bounds.size.height > px(0.) && bounds.size.width > px(0.) {
            owner
                .update(cx, |doc, _| {
                    doc.scroll_areas.push(ScrollArea {
                        id,
                        bounds,
                        scroll: Rc::new(scroll),
                    })
                })
                .ok();
        }
    }
}
/// One native selection, focus owner, and bounded cache of text painted in this document.
pub struct Document {
    props: DocumentProps,
    children: Option<Children>,
    native: Vec<AnyView>,
    focus: FocusHandle,
    selection: SelectionState,
    selection_revision: u64,
    drag_capture: Option<HitboxId>,
    drag_position: Option<Point<Pixels>>,
    drag_scrolls: Vec<u64>,
    drag_task: Option<Task<()>>,
    scroll_areas: Vec<ScrollArea>,
    painted_selection_revision: u64,
    entries: Vec<Entry>,
    cache: FxHashMap<TextKey, Cached>,
    paint: u32,
    content_changed: bool,
    query_revision: u64,
    painted_query_revision: u64,
    content_revision: u64,
    reported_search: Option<(u64, u64, usize)>,
    frame: Option<gpui_react::FrameInfo>,
    painted_matcher: Option<Arc<search::Matcher>>,
    painted_index_offset: usize,
    ranges: Vec<TextRange>,
    highlights: Vec<Highlight>,
    match_count: usize,
}
impl Document {
    pub fn new(props: DocumentProps, cx: &mut Context<Self>) -> Self {
        Self {
            props,
            children: None,
            native: Vec::new(),
            focus: cx.focus_handle(),
            selection: SelectionState::default(),
            selection_revision: 0,
            drag_capture: None,
            drag_position: None,
            drag_scrolls: vec![],
            drag_task: None,
            scroll_areas: vec![],
            painted_selection_revision: 0,
            entries: vec![],
            cache: FxHashMap::default(),
            paint: 0,
            content_changed: false,
            query_revision: 0,
            painted_query_revision: 0,
            content_revision: 0,
            reported_search: None,
            frame: None,
            painted_matcher: None,
            painted_index_offset: 0,
            ranges: vec![],
            highlights: vec![],
            match_count: 0,
        }
    }
    /// Native views composed into this document ahead of its React children.
    /// Native extensions that own a Document supply their content here.
    pub fn set_native_children(&mut self, native: Vec<AnyView>, cx: &mut Context<Self>) {
        self.native = native;
        cx.notify();
    }
    fn index_offset(&self) -> usize {
        self.props
            .search
            .as_ref()
            .map_or(0, |search| search.match_index_offset)
    }
    fn emit_selection(&mut self, changed: bool, cx: &mut Context<Self>) {
        if changed {
            self.selection_revision += 1;
            cx.notify();
            cx.emit(DocumentEvent::Selection {
                revision: self.selection_revision,
                has_selection: self.selection.has_selection(),
            });
        }
    }
    fn registered(&self) -> Vec<RegisteredText> {
        self.entries
            .iter()
            .filter(|entry| entry.options.selectable())
            .map(|entry| RegisteredText {
                key: entry.key.clone(),
                text: entry.text.clone(),
            })
            .collect()
    }
    fn hit(&self, position: Point<Pixels>, start: bool, window: &Window) -> Option<(usize, usize)> {
        let mut contained = None;
        let mut nearest: Option<(usize, (f32, f32))> = None;
        let mut selectable_index = 0;
        for entry in &self.entries {
            let bounds = entry.geometry.bounds.intersect(&entry.clip);
            if !entry.options.selectable() {
                if start && bounds.contains(&position) {
                    contained = None;
                    nearest = None;
                }
                continue;
            }
            let index = selectable_index;
            selectable_index += 1;
            if bounds.size.width <= px(0.) || bounds.size.height <= px(0.) {
                continue;
            }
            if bounds.contains(&position) && (!start || entry.hitbox.is_hovered(window)) {
                contained = Some(index);
            }
            let dy = (bounds.top() - position.y)
                .max(position.y - bounds.bottom())
                .max(px(0.));
            let dx = (bounds.left() - position.x)
                .max(position.x - bounds.right())
                .max(px(0.));
            let distance = (f32::from(dy), f32::from(dx));
            if nearest.as_ref().is_none_or(|(_, old)| distance < *old) {
                nearest = Some((index, distance));
            }
        }
        let index = if start {
            contained?
        } else {
            contained.or(nearest.map(|(index, _)| index))?
        };
        let entry = self
            .entries
            .iter()
            .filter(|entry| entry.options.selectable())
            .nth(index)?;
        Some((index, entry.geometry.index_for_position(position)))
    }
    fn down(
        &mut self,
        event: &MouseDownEvent,
        hitbox: &Hitbox,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            return;
        }
        let Some((index, offset)) = self.hit(event.position, true, window) else {
            return;
        };
        let entry = self
            .entries
            .iter()
            .filter(|entry| entry.options.selectable())
            .nth(index)
            .unwrap();
        let changed = if event.click_count >= 2 {
            let range = if event.click_count >= 3 {
                0..entry.text.len()
            } else {
                selection::word_range(
                    &entry.text,
                    entry.geometry.character_for_position(event.position),
                )
            };
            let changed = self
                .selection
                .begin_with_span(&entry.key, &entry.text, range);
            self.focus.focus(window, cx);
            changed
        } else {
            self.selection.arm(&entry.key, offset)
        };
        self.drag_capture = Some(hitbox.id);
        self.drag_scrolls = self
            .scroll_areas
            .iter()
            .rev()
            .filter(|area| area.bounds.contains(&event.position))
            .map(|area| area.id)
            .collect();
        if self.selection.is_dragging() {
            window.capture_pointer(hitbox.id);
        }
        self.emit_selection(changed, cx);
    }
    fn dragging(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !event.dragging() {
            return;
        }
        let mut changed = false;
        if self.selection.promote_pending() {
            self.focus.focus(window, cx);
            if let Some(hitbox) = self.drag_capture {
                window.capture_pointer(hitbox);
            }
        }
        if self.selection.is_dragging() {
            self.drag_position = Some(event.position);
            self.start_drag_clock(window, cx);
        }
        if self.selection.is_dragging()
            && let Some(head) = self.hit(event.position, false, window)
        {
            let entries = self
                .entries
                .iter()
                .filter(|entry| entry.options.selectable())
                .map(|entry| RegisteredText {
                    key: entry.key.clone(),
                    text: entry.text.clone(),
                })
                .collect::<Vec<_>>();
            changed = self.selection.update_drag(&entries, head);
        }
        self.emit_selection(changed, cx);
    }
    fn start_drag_clock(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.drag_task.is_some() || self.scroll_areas.is_empty() {
            return;
        }
        self.drag_task = Some(cx.spawn_in(window, async move |owner, cx| {
            let mut last = cx.background_executor().now();
            let mut last_scroll_paint = None;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let now = cx.background_executor().now();
                let elapsed = now.duration_since(last).as_secs_f32();
                last = now;
                let keep = owner
                    .update(cx, |doc, cx| {
                        if !doc.selection.is_dragging() {
                            doc.drag_task = None;
                            return false;
                        }
                        if last_scroll_paint == Some(doc.paint) {
                            return true;
                        }
                        let Some(position) = doc.drag_position else {
                            return false;
                        };
                        // The press chooses the scroll chain. Crossing another
                        // pane during the drag must not scroll that pane.
                        let areas = doc
                            .drag_scrolls
                            .iter()
                            .filter_map(|id| doc.scroll_areas.iter().find(|area| area.id == *id))
                            .map(|area| (area.bounds, area.scroll.clone()))
                            .collect::<Vec<_>>();
                        for (bounds, scroll) in areas {
                            let distance = if position.y < bounds.top() {
                                position.y - bounds.top()
                            } else if position.y > bounds.bottom() {
                                position.y - bounds.bottom()
                            } else {
                                px(0.)
                            };
                            // At most half a viewport per completed paint keeps
                            // an overlap for virtualized selection. An occluded
                            // window cannot accumulate invisible scroll steps.
                            let cap = bounds.size.height / 2.;
                            let delta = (distance * elapsed * 15.).clamp(-cap, cap);
                            if delta != px(0.) && scroll(delta, cx) {
                                last_scroll_paint = Some(doc.paint);
                                break;
                            }
                        }
                        true
                    })
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
        }));
    }
    fn select(
        &mut self,
        start: (usize, usize),
        end: (usize, usize),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entries = self.registered();
        let key = entries[start.0].key.clone();
        let spans = selection::resolve_spans(&entries, start, end);
        let changed = self.selection.spans() != spans;
        self.selection.arm(&key, start.1);
        self.selection.promote_pending();
        self.selection.update_spans(spans);
        self.selection.end_active_drag();
        self.focus.focus(window, cx);
        self.emit_selection(changed, cx);
    }
    pub fn apply_command(
        &mut self,
        command: DocumentCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        match command {
            DocumentCommand::Clear => {
                let changed = self.selection.clear();
                self.emit_selection(changed, cx);
            }
            DocumentCommand::Copy => {
                if let Some(text) = self.selection.selected_text() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            DocumentCommand::SelectAll => {
                anyhow::ensure!(self.paint > 0, "document has not painted");
                let entries = self.registered();
                if let Some(last) = entries.last() {
                    let end = (entries.len() - 1, last.text.len());
                    self.select((0, 0), end, window, cx);
                }
            }
            DocumentCommand::Select {
                start,
                end,
                expected_content_revision,
            } => {
                anyhow::ensure!(self.paint > 0, "document has not painted");
                anyhow::ensure!(
                    expected_content_revision == self.content_revision,
                    "stale document content revision"
                );
                let entries = self.registered();
                let find = |endpoint: Endpoint| -> anyhow::Result<(usize, usize)> {
                    let key = TextKey::parse(&endpoint.key);
                    let index = entries
                        .iter()
                        .position(|entry| entry.key == key)
                        .ok_or_else(|| anyhow::anyhow!("selection endpoint is not painted"))?;
                    Ok((
                        index,
                        geometry::byte_offset(&entries[index].text, endpoint.offset)?,
                    ))
                };
                let start = find(start)?;
                let end = find(end)?;
                self.select(start, end, window, cx);
            }
        }
        Ok(())
    }
    /// The current native selection for one logical text, in UTF-8 byte offsets.
    /// Returns None if the key is not selected or the selected bytes no longer
    /// match `text`. This reads selection state; it does not compute layout.
    pub fn selected_range(&self, key: &TextKey, text: &str) -> Option<Range<usize>> {
        let range = self.selection.wash_range(key)?;
        let source = self
            .selection
            .spans()
            .iter()
            .find(|span| span.key == *key)?;
        (text.get(range.clone())? == source.text.get(range.clone())?).then_some(range)
    }
    pub fn snapshot(&self) -> DocumentSnapshot {
        DocumentSnapshot {
            text: self
                .entries
                .iter()
                .map(|entry| PaintedText {
                    key: entry.key.to_string(),
                    text: entry.text.to_string(),
                    bounds: entry.geometry.bounds.intersect(&entry.clip).into(),
                    selectable: entry.options.selectable(),
                    searchable: entry.options.searchable(),
                })
                .collect(),
            content_revision: self.content_revision,
            selection: self.selection.selected_text(),
            selection_revision: self.selection_revision,
            painted_selection_revision: self.painted_selection_revision,
            ranges: self.ranges.clone(),
            highlights: self.highlights.clone(),
            match_count: self.match_count,
            match_index_offset: self.painted_index_offset,
            query: self
                .painted_matcher
                .as_ref()
                .map(|matcher| matcher.query.clone()),
            frame: self.frame,
        }
    }
    fn begin_paint(&mut self, window: &Window, cx: &App) {
        self.paint += 1;
        self.content_changed = false;
        self.entries.clear();
        self.scroll_areas.clear();
        self.ranges.clear();
        self.highlights.clear();
        self.match_count = 0;
        self.painted_selection_revision = self.selection_revision;
        self.frame = gpui_react::current_frame(window, cx);
        self.painted_matcher = self
            .props
            .search
            .as_ref()
            .map(|search| search.matcher.clone());
        self.painted_index_offset = self.index_offset();
        self.painted_query_revision = self.query_revision;
    }
    fn finish_paint(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.selection.is_dragging()
            && let Some(position) = self.drag_position
        {
            cx.defer_in(window, move |doc, window, cx| {
                if doc.selection.is_dragging() && doc.drag_position == Some(position) {
                    doc.dragging(
                        &MouseMoveEvent {
                            position,
                            pressed_button: Some(MouseButton::Left),
                            modifiers: Modifiers::default(),
                        },
                        window,
                        cx,
                    );
                }
            });
        }
        self.content_changed |= self.cache.len() != self.entries.len();
        self.cache.retain(|_, entry| entry.seen == self.paint);
        if self.content_changed {
            self.content_revision += 1;
        }
        let report = (
            self.content_revision,
            self.painted_query_revision,
            self.painted_index_offset,
        );
        if self.reported_search.as_ref() != Some(&report) {
            self.reported_search = Some(report);
            let event = DocumentEvent::Search {
                query: self
                    .painted_matcher
                    .as_ref()
                    .map(|matcher| matcher.query.clone()),
                frame: self.frame,
                content_revision: self.content_revision,
                count: self.match_count,
                index_offset: self.painted_index_offset,
            };
            // Publication follows the complete native paint, not a partial text walk.
            cx.defer_in(window, move |_, _, cx| cx.emit(event));
        }
    }
    fn paint_text(
        &mut self,
        key: TextKey,
        text: SharedString,
        layout: TextLayout,
        hitbox: HitboxId,
        options: TextOptions,
        window: &mut Window,
    ) {
        let align = window.text_style().text_align;
        let clip = window.content_mask().bounds;
        let geometry = geometry::Geometry::new(&layout, align);
        let matcher = self.props.search.as_ref().map(|search| &search.matcher);
        let cached = self.cache.entry(key.clone()).or_insert_with(|| Cached {
            text: SharedString::default(),
            matcher: None,
            matches: None,
            options,
            seen: 0,
            index: 0,
        });
        if cached.seen == self.paint {
            let previous = &self.entries[cached.index as usize];
            assert!(
                previous.text == text && previous.geometry.bounds == geometry.bounds,
                "DocumentText keys must identify one logical text"
            );
            return;
        }
        let source_changed = cached.seen == 0 || cached.text != text;
        self.content_changed |=
            source_changed || cached.index as usize != self.entries.len() || cached.options != options;
        let same_matcher = match (cached.matcher.as_ref(), matcher) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        };
        if source_changed || !same_matcher || cached.options.searchable() != options.searchable() {
            cached.matches = matcher
                .filter(|_| options.searchable())
                .map(|matcher| matcher.ranges(&text))
                .filter(|ranges| !ranges.is_empty());
            cached.matcher = matcher.cloned();
        }
        cached.text = text.clone();
        cached.options = options;
        cached.seen = self.paint;
        cached.index = self.entries.len() as u32;
        let matches = cached.matches.as_deref().unwrap_or(&[]);
        if let Some(search) = &self.props.search {
            for (local, range) in matches.iter().enumerate() {
                let index = options
                    .match_index_offset()
                    .unwrap_or(search.match_index_offset + self.match_count)
                    + local;
                let active = search.active_index == Some(index);
                let rects = geometry
                    .range_rects(range.clone())
                    .into_iter()
                    .map(|r| r.intersect(&clip))
                    .filter(|r| r.size.width > px(0.) && r.size.height > px(0.))
                    .collect::<Vec<_>>();
                for rect in &rects {
                    window.paint_quad(fill(
                        *rect,
                        if active {
                            search.active_color.0
                        } else {
                            search.color.0
                        },
                    ));
                }
                self.highlights.push(Highlight {
                    index,
                    active,
                    range: TextRange {
                        key: key.to_string(),
                        start: geometry::utf16(&text, range.start),
                        end: geometry::utf16(&text, range.end),
                        rects: rects.into_iter().map(Into::into).collect(),
                    },
                });
            }
        }
        self.match_count += matches.len();
        if options.selectable()
            && let Some(range) = self.selected_range(&key, &text)
        {
            let rects = geometry
                .range_rects(range.clone())
                .into_iter()
                .map(|r| r.intersect(&clip))
                .filter(|r| r.size.width > px(0.) && r.size.height > px(0.))
                .collect::<Vec<_>>();
            let color = self
                .props
                .selection_color
                .unwrap_or(Color(rgba(0x3875d799).into()))
                .0;
            for rect in &rects {
                window.paint_quad(fill(*rect, color));
            }
            self.ranges.push(TextRange {
                key: key.to_string(),
                start: geometry::utf16(&text, range.start),
                end: geometry::utf16(&text, range.end),
                rects: rects.into_iter().map(Into::into).collect(),
            });
        }
        self.entries.push(Entry {
            key,
            text,
            geometry,
            hitbox,
            clip,
            options,
        });
    }
}
impl Render for Document {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut children: Vec<AnyElement> = self
            .native
            .iter()
            .map(|view| view.clone().into_any_element())
            .collect();
        if let Some(react) = &self.children {
            children.extend(react.render_all(window, cx));
        }
        DocumentScope {
            context: Rc::new(ActiveDocument(cx.weak_entity())),
            child: self
                .props
                .style
                .apply_interactive(
                    div()
                        .id("document")
                        .flex()
                        .flex_col()
                        .track_focus(&self.focus),
                )
                .children(children)
                .into_any_element(),
        }
    }
}
impl ReactView for Document {
    type Props = DocumentProps;
    fn create(props: Self::Props, _: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new(props, cx)
    }
    fn set_props(&mut self, mut props: Self::Props, _: &mut Window, cx: &mut Context<Self>) {
        let same_query = self.props.search.as_ref().map(|s| &s.matcher.query)
            == props.search.as_ref().map(|s| &s.matcher.query);
        if same_query {
            if let (Some(old), Some(next)) = (&self.props.search, &mut props.search) {
                next.matcher = old.matcher.clone();
            }
        } else {
            self.query_revision += 1;
        }
        self.props = props;
        cx.notify();
    }
    fn unmounting(&mut self, window: &mut Window, _: &mut Context<Self>) {
        if self.focus.is_focused(window) {
            window.blur();
        }
        self.selection.clear();
        self.drag_task = None;
    }
}
impl ReactChildren for Document {
    fn set_children(&mut self, children: Children, _: &mut Window, cx: &mut Context<Self>) {
        self.children = Some(children);
        cx.notify();
    }
}
impl EventEmitter<DocumentEvent> for Document {}
impl ReactEvents for Document {
    type Event = DocumentEvent;
}
impl ReactCommands for Document {
    type Command = DocumentCommand;
    fn command(
        &mut self,
        command: Self::Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        self.apply_command(command, window, cx)
    }
}
impl ReactQueries for Document {
    type Query = ();
    type Reply = DocumentSnapshot;
    fn query(
        &mut self,
        _: (),
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> anyhow::Result<Self::Reply> {
        Ok(self.snapshot())
    }
}
struct ActiveDocument(WeakEntity<Document>);
struct DocumentScope {
    context: Rc<ActiveDocument>,
    child: AnyElement,
}
impl Element for DocumentScope {
    type RequestLayoutState = ();
    type PrepaintState = Hitbox;
    fn id(&self) -> Option<ElementId> {
        Some("document-scope".into())
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
        (self.child.request_layout(window, cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Hitbox {
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        window.with_element_context(self.context.clone(), |window| {
            self.child.prepaint(window, cx)
        });
        hitbox
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        hitbox: &mut Hitbox,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.context
            .0
            .update(cx, |owner, cx| {
                owner.drag_capture = Some(hitbox.id);
                owner.begin_paint(window, cx);
            })
            .ok();
        let owner = self.context.0.clone();
        let hitbox = hitbox.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble {
                owner
                    .update(cx, |owner, cx| owner.down(event, &hitbox, window, cx))
                    .ok();
            }
        });
        let owner = self.context.0.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture {
                owner
                    .update(cx, |owner, cx| owner.dragging(event, window, cx))
                    .ok();
            }
        });
        let owner = self.context.0.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
            if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                owner
                    .update(cx, |owner, _| {
                        if owner.selection.is_pending() {
                            owner.selection.cancel_pending();
                        }
                        owner.selection.end_active_drag();
                        owner.drag_task = None;
                        owner.drag_position = None;
                        owner.drag_scrolls.clear();
                    })
                    .ok();
            }
        });
        let owner = self.context.0.clone();
        window.on_root_key_event(move |event: &KeyDownEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble
                || !(event.keystroke.modifiers.platform || event.keystroke.modifiers.control)
            {
                return;
            }
            owner
                .update(cx, |owner, cx| {
                    if !owner.focus.is_focused(window) {
                        return;
                    }
                    let command = match event.keystroke.key.as_str() {
                        "c" => DocumentCommand::Copy,
                        "a" => DocumentCommand::SelectAll,
                        _ => return,
                    };
                    if owner.apply_command(command, window, cx).is_ok() {
                        cx.stop_propagation();
                    }
                })
                .ok();
        });
        window.with_element_context(self.context.clone(), |window| self.child.paint(window, cx));
        let owner = self.context.0.clone();
        window.on_draw_complete(move |window, cx| {
            owner
                .update(cx, |owner, cx| owner.finish_paint(window, cx))
                .ok();
        });
    }
}
impl IntoElement for DocumentScope {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

/// One complete logical text, optionally styled with GPUI runs. Keys must be
/// stable and unique inside the nearest Document. Outside a Document it remains
/// ordinary GPUI text. Native extensions use this helper without a React tree.
pub struct DocumentText {
    key: TextKey,
    text: SharedString,
    styled: StyledText,
    options: TextOptions,
}
pub fn document_text(key: impl Into<TextKey>, text: impl Into<SharedString>) -> DocumentText {
    let text = text.into();
    DocumentText {
        key: key.into(),
        styled: StyledText::new(text.clone()),
        text,
        options: TextOptions::default(),
    }
}
impl DocumentText {
    /// GPUI's own layout handle. Clone it before moving this element into its
    /// parent. Read positions only after this text has completed prepaint.
    pub fn layout(&self) -> &TextLayout {
        self.styled.layout()
    }
    pub fn with_runs(mut self, runs: Vec<TextRun>) -> Self {
        self.styled = self.styled.with_runs(runs);
        self
    }
    pub fn searchable(mut self, searchable: bool) -> Self {
        self.options.set(SEARCHABLE, searchable);
        self
    }
    pub fn match_index_offset(mut self, offset: usize) -> Self {
        self.options.match_index_offset = u32::try_from(offset).unwrap_or(NO_OFFSET - 1);
        self
    }
    pub fn selectable(mut self, selectable: bool) -> Self {
        self.options.set(SELECTABLE, selectable);
        self
    }
}
impl Element for DocumentText {
    type RequestLayoutState = ();
    type PrepaintState = Option<Hitbox>;
    fn id(&self) -> Option<ElementId> {
        Some(match &self.key {
            TextKey::Node(id) => ElementId::Integer(*id as u64),
            TextKey::Named(name) => ElementId::Name(name.clone()),
        })
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn a11y_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Label)
    }
    fn write_a11y_info(&self, node: &mut accesskit::Node) {
        node.set_value(self.text.to_string());
    }
    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        self.styled.request_layout(id, inspector, window, cx)
    }
    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Hitbox> {
        self.styled
            .prepaint(id, inspector, bounds, state, window, cx);
        let in_document = window.element_context::<ActiveDocument>().is_some();
        in_document.then(|| window.insert_hitbox(bounds, HitboxBehavior::Normal))
    }
    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut (),
        prepaint: &mut Option<Hitbox>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(owner) = window
            .element_context::<ActiveDocument>()
            .map(|active| active.0.clone())
        {
            owner
                .update(cx, |owner, _| {
                    owner.paint_text(
                        self.key.clone(),
                        self.text.clone(),
                        self.styled.layout().clone(),
                        prepaint
                            .as_ref()
                            .expect("document text was prepainted in its scope")
                            .id,
                        self.options,
                        window,
                    )
                })
                .ok();
        }
        self.styled
            .paint(id, inspector, bounds, state, &mut (), window, cx);
    }
}
impl IntoElement for DocumentText {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

#[cfg(test)]
mod layout {
    use super::{Cached, Entry, TextOptions};
    use std::mem::size_of;
    #[test]
    fn per_text_records_stay_small() {
        assert_eq!(size_of::<TextOptions>(), 8);
        assert!(size_of::<Cached>() <= 64, "{}", size_of::<Cached>());
        assert!(size_of::<Entry>() <= 136, "{}", size_of::<Entry>());
    }
}

#[cfg(test)]
mod tests {
    use super::{Document, DocumentProps, TextKey, geometry};
    use gpui::{self, AppContext, Entity, TestAppContext, WindowHandle};
    use gpui_react::{Host, ReactView, Registry};
    use serde_json::{Value, json};
    use std::sync::Arc;

    fn props(query: &str, active: usize, offset: usize) -> Value {
        json!({"search":{"query":query,"activeIndex":active,"matchIndexOffset":offset},"style":{"width":200,"height":100}})
    }
    fn host(cx: &mut TestAppContext, operations: Value) -> (WindowHandle<Host>, Entity<Document>) {
        let window = cx.add_window(|_, _| {
            let mut registry = Registry::default();
            crate::register_kit(&mut registry).unwrap();
            Host::new(registry, Arc::new(|_| {}))
        });
        let document = window
            .update(cx, |host, window, cx| {
                apply(host, 1, operations, window, cx);
                host.view(1).unwrap().downcast::<Document>().unwrap()
            })
            .unwrap();
        (window, document)
    }
    fn apply(host: &mut Host, sequence: u64, operations: Value, window: &mut gpui::Window, cx: &mut gpui::Context<Host>) {
        host.apply(
            gpui_react::protocol::Transaction::from_json(&json!({"version":1,"sequence":sequence,"operations":operations})).unwrap(),
            window,
            cx,
        )
        .unwrap();
    }
    fn draw(cx: &mut TestAppContext, window: WindowHandle<Host>) {
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
    }

    #[gpui::test]
    fn native_match_cache_survives_cursor_colors_layout_and_offset_changes(
        cx: &mut TestAppContext,
    ) {
        let (window, document) = host(cx, json!([
            {"op":"create","id":1,"component":"document","props":props("token",0,0)},
            {"op":"place","parent":null,"child":1,"before":null},
            {"op":"create","id":2,"component":"text","props":{"text":"token token","textKey":"text"}},
            {"op":"place","parent":1,"child":2,"before":null}
        ]));
        draw(cx, window);
        let matches = document.read_with(cx, |doc, _| doc.cache[&TextKey::from("text")].matches.clone().unwrap());
        let revision = document.read_with(cx, |doc, _| doc.content_revision);
        for (sequence, (active, offset, width)) in [(1, 0, 150.), (5, 4, 300.)].into_iter().enumerate() {
            window
                .update(cx, |host, window, cx| {
                    let mut props = props("token", active, offset);
                    props["style"]["width"] = json!(width);
                    props["search"]["color"] = json!("#00ff00");
                    apply(host, sequence as u64 + 2, json!([{"op":"props","id":1,"component":"document","props":props}]), window, cx);
                })
                .unwrap();
            draw(cx, window);
            document.read_with(cx, |doc, _| {
                assert!(
                    doc.cache[&TextKey::from("text")]
                        .matches
                        .as_ref()
                        .is_some_and(|current| Arc::ptr_eq(&matches, current)),
                    "cursor and paint changes must not rematch text"
                );
                assert_eq!(doc.content_revision, revision);
                assert_eq!(doc.match_count, 2);
            });
        }
        window
            .update(cx, |host, window, cx| {
                apply(host, 4, json!([{"op":"remove","id":2}]), window, cx)
            })
            .unwrap();
        draw(cx, window);
        document.read_with(cx, |doc, _| {
            assert!(doc.cache.is_empty());
            assert!(doc.content_revision > revision);
        });
    }

    #[gpui::test]
    fn snapshot_keeps_the_painted_match_offset_until_next_draw(cx: &mut TestAppContext) {
        let window = cx.add_window(|_, cx| Document::new(serde_json::from_value(props("token", 0, 2)).unwrap(), cx));
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        window
            .update(cx, |doc, window, cx| {
                doc.set_props(serde_json::from_value(props("different", 1, 8)).unwrap(), window, cx);
                assert_eq!(
                    doc.snapshot().match_index_offset,
                    2,
                    "a query must not combine new props with the old painted matches"
                );
                assert_eq!(doc.snapshot().query.unwrap().query, "token");
            })
            .unwrap();
    }

    #[gpui::test]
    fn geometry_has_no_fixed_line_limit_and_checks_surrogate_boundaries(cx: &mut TestAppContext) {
        let text: String = (0..300).map(|_| "line\n").collect();
        let (window, document) = host(cx, json!([
            {"op":"create","id":1,"component":"document","props":{}},
            {"op":"place","parent":null,"child":1,"before":null},
            {"op":"create","id":2,"component":"text","props":{"text":text,"textKey":"long"}},
            {"op":"place","parent":1,"child":2,"before":null}
        ]));
        draw(cx, window);
        document.read_with(cx, |doc, _| {
            let entry = &doc.entries[0];
            assert_eq!(entry.geometry.range_rects(0..entry.text.len()).len(), 300);
        });
        assert_eq!(geometry::byte_offset("a😀b", 3).unwrap(), 5);
        assert!(geometry::byte_offset("a😀b", 2).is_err());
        let _ = DocumentProps::default();
    }
}
