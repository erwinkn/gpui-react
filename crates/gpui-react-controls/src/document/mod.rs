//! Per-document text services over native paint, independent of the React host model.
mod geometry;
mod search;
mod selection;
use crate::{Color, Style, geometry::Rect};
use gpui::{prelude::*, *};
use gpui_react::{ReactChildren, ReactCommands, ReactEvents, ReactQueries, ReactView};
pub use search::{Query as SearchQuery, Search};
use selection::{RegisteredText, SelectionState};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, ops::Range, rc::Rc, sync::Arc, time::Duration};

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentProps {
    pub style: Style,
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
#[derive(Clone, Copy, PartialEq)]
struct TextOptions {
    selectable: bool,
    searchable: bool,
    match_index_offset: Option<usize>,
}
impl Default for TextOptions {
    fn default() -> Self {
        Self {
            selectable: true,
            searchable: true,
            match_index_offset: None,
        }
    }
}
struct Entry {
    key: SharedString,
    text: SharedString,
    geometry: geometry::Geometry,
    hitbox: HitboxId,
    clip: Bounds<Pixels>,
    options: TextOptions,
}
struct Cached {
    text: SharedString,
    matcher: Option<Arc<search::Matcher>>,
    options: TextOptions,
    matches: Arc<[Range<usize>]>,
    seen: u64,
    index: usize,
}
type ScrollCallback = Rc<dyn Fn(Pixels, &mut App) -> bool>;
struct ScrollArea {
    id: EntityId,
    bounds: Bounds<Pixels>,
    scroll: ScrollCallback,
}
/// Register a vertical scroll area during paint. Return whether the callback
/// moved its native scroll state. The document uses the normal GPUI state;
/// it neither copies offsets nor routes drag frames through JavaScript.
pub fn register_scroll_area(
    id: EntityId,
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
    children: Vec<AnyView>,
    focus: FocusHandle,
    selection: SelectionState,
    selection_revision: u64,
    drag_capture: Option<HitboxId>,
    drag_position: Option<Point<Pixels>>,
    drag_scrolls: Vec<EntityId>,
    drag_task: Option<Task<()>>,
    scroll_areas: Vec<ScrollArea>,
    painted_selection_revision: u64,
    entries: Vec<Entry>,
    cache: HashMap<SharedString, Cached>,
    paint: u64,
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
            children: vec![],
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
            cache: HashMap::new(),
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
            .filter(|entry| entry.options.selectable)
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
            if !entry.options.selectable {
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
            .filter(|entry| entry.options.selectable)
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
            .filter(|entry| entry.options.selectable)
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
                .filter(|entry| entry.options.selectable)
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
                    let index = entries
                        .iter()
                        .position(|entry| entry.key.as_ref() == endpoint.key)
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
    pub fn selected_range(&self, key: &str, text: &str) -> Option<Range<usize>> {
        let range = self.selection.wash_range(key)?;
        let source = self
            .selection
            .spans()
            .iter()
            .find(|span| span.key.as_ref() == key)?;
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
                    selectable: entry.options.selectable,
                    searchable: entry.options.searchable,
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
        key: SharedString,
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
            options,
            matches: Arc::from([]),
            seen: 0,
            index: 0,
        });
        if cached.seen == self.paint {
            let previous = &self.entries[cached.index];
            assert!(
                previous.text == text && previous.geometry.bounds == geometry.bounds,
                "DocumentText keys must identify one logical text"
            );
            return;
        }
        let source_changed = cached.seen == 0 || cached.text != text;
        self.content_changed |=
            source_changed || cached.index != self.entries.len() || cached.options != options;
        let same_matcher = match (cached.matcher.as_ref(), matcher) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        };
        if source_changed || !same_matcher || cached.options.searchable != options.searchable {
            cached.matches = matcher
                .filter(|_| options.searchable)
                .map_or_else(|| Arc::from([]), |matcher| matcher.ranges(&text));
            cached.matcher = matcher.cloned();
        }
        cached.text = text.clone();
        cached.options = options;
        cached.seen = self.paint;
        cached.index = self.entries.len();
        if let Some(search) = &self.props.search {
            for (local, range) in cached.matches.iter().enumerate() {
                let index = options
                    .match_index_offset
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
        self.match_count += cached.matches.len();
        if options.selectable
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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                .children(self.children.clone())
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
    fn set_children(&mut self, children: Vec<AnyView>, _: &mut Window, cx: &mut Context<Self>) {
        self.children = children;
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
    key: SharedString,
    text: SharedString,
    styled: StyledText,
    options: TextOptions,
}
pub fn document_text(key: impl Into<SharedString>, text: impl Into<SharedString>) -> DocumentText {
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
        self.options.searchable = searchable;
        self
    }
    pub fn match_index_offset(mut self, offset: usize) -> Self {
        self.options.match_index_offset = Some(offset);
        self
    }
    pub fn selectable(mut self, selectable: bool) -> Self {
        self.options.selectable = selectable;
        self
    }
}
impl Element for DocumentText {
    type RequestLayoutState = ();
    type PrepaintState = Option<Hitbox>;
    fn id(&self) -> Option<ElementId> {
        Some(ElementId::Name(self.key.clone()))
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
mod tests {
    use super::{Document, DocumentProps, geometry};
    use crate::Color;
    use crate::{Text, TextProps};
    use gpui::{self, AppContext, TestAppContext, rgb};
    use gpui_react::{ReactChildren, ReactView};
    use serde_json::json;
    use std::sync::Arc;
    fn props(query: &str, active: usize, offset: usize) -> DocumentProps {
        serde_json::from_value(json!({"search":{"query":query,"activeIndex":active,"matchIndexOffset":offset},"style":{"width":200,"height":100}})).unwrap()
    }
    #[gpui::test]
    fn native_match_cache_survives_cursor_colors_layout_and_offset_changes(
        cx: &mut TestAppContext,
    ) {
        let window = cx.add_window(|_, cx| Document::new(props("token", 0, 0), cx));
        window
            .update(cx, |doc, window, cx| {
                let text = cx.new(|_| {
                    Text::new(TextProps {
                        text: "token token".into(),
                        text_key: Some("text".into()),
                        ..Default::default()
                    })
                });
                doc.set_children(vec![text.into()], window, cx);
            })
            .unwrap();
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        let matches = window
            .update(cx, |doc, _, _| doc.cache["text"].matches.clone())
            .unwrap();
        let revision = window.update(cx, |doc, _, _| doc.content_revision).unwrap();
        for (active, offset, width) in [(1, 0, 150.), (5, 4, 300.)] {
            window
                .update(cx, |doc, window, cx| {
                    let mut props = props("token", active, offset);
                    props.style.width = Some(crate::Length::Pixels(width));
                    props.search.as_mut().unwrap().color = Color(rgb(0x00ff00).into());
                    doc.set_props(props, window, cx);
                })
                .unwrap();
            cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
                .unwrap();
            window
                .update(cx, |doc, _, _| {
                    assert!(
                        Arc::ptr_eq(&matches, &doc.cache["text"].matches),
                        "cursor and paint changes must not rematch text"
                    );
                    assert_eq!(doc.content_revision, revision);
                    assert_eq!(doc.match_count, 2);
                })
                .unwrap();
        }
        window
            .update(cx, |doc, window, cx| doc.set_children(vec![], window, cx))
            .unwrap();
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        window
            .update(cx, |doc, _, _| {
                assert!(doc.cache.is_empty());
                assert!(doc.content_revision > revision);
            })
            .unwrap();
    }
    #[gpui::test]
    fn snapshot_keeps_the_painted_match_offset_until_next_draw(cx: &mut TestAppContext) {
        let window = cx.add_window(|_, cx| Document::new(props("token", 0, 2), cx));
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        window
            .update(cx, |doc, window, cx| {
                doc.set_props(props("different", 1, 8), window, cx);
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
        let window = cx.add_window(|_, cx| Document::new(DocumentProps::default(), cx));
        window
            .update(cx, |doc, window, cx| {
                let text = cx.new(|_| {
                    Text::new(TextProps {
                        text: (0..300).map(|_| "line\n").collect(),
                        text_key: Some("long".into()),
                        ..Default::default()
                    })
                });
                doc.set_children(vec![text.into()], window, cx);
            })
            .unwrap();
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        window
            .update(cx, |doc, _, _| {
                let entry = &doc.entries[0];
                assert_eq!(entry.geometry.range_rects(0..entry.text.len()).len(), 300);
            })
            .unwrap();
        assert_eq!(geometry::byte_offset("a😀b", 3).unwrap(), 5);
        assert!(geometry::byte_offset("a😀b", 2).is_err());
    }

    struct DeferredText(gpui::SharedString);
    impl gpui::Render for DeferredText {
        fn render(
            &mut self,
            _: &mut gpui::Window,
            _: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            use gpui::prelude::*;
            gpui::deferred(
                gpui::div()
                    .child(super::document_text("deferred", self.0.clone()))
                    .child(
                        gpui::deferred(super::document_text("nested", "nested token"))
                            .with_priority(2),
                    ),
            )
        }
    }

    #[gpui::test]
    fn deferred_document_text_keeps_selection_search_and_cache(cx: &mut TestAppContext) {
        use gpui::prelude::*;
        let window = cx.add_window(|_, cx| Document::new(props("token", 0, 0), cx));
        let reports = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let report_sink = reports.clone();
        let document = window.update(cx, |_, _, cx| cx.entity()).unwrap();
        cx.update(|cx| {
            cx.subscribe(&document, move |_, event: &super::DocumentEvent, _| {
                if let super::DocumentEvent::Search {
                    count,
                    content_revision,
                    ..
                } = event
                {
                    report_sink.borrow_mut().push((*count, *content_revision));
                }
            })
            .detach()
        });
        window
            .update(cx, |doc, window, cx| {
                let normal = cx.new(|_| {
                    Text::new(TextProps {
                        text: "normal token".into(),
                        text_key: Some("normal".into()),
                        ..Default::default()
                    })
                });
                let deferred = cx.new(|_| DeferredText("deferred token".into()));
                doc.set_children(vec![normal.into(), deferred.into()], window, cx);
            })
            .unwrap();
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        let (revision, matches) = window
            .update(cx, |doc, window, cx| {
                assert_eq!(
                    doc.entries
                        .iter()
                        .map(|entry| entry.key.as_ref())
                        .collect::<Vec<_>>(),
                    vec!["normal", "deferred", "nested"]
                );
                assert_eq!(doc.match_count, 3);
                assert_eq!(doc.cache.len(), 3);
                doc.apply_command(super::DocumentCommand::SelectAll, window, cx)
                    .unwrap();
                assert_eq!(
                    doc.snapshot().selection.as_deref(),
                    Some("normal token\ndeferred token\nnested token")
                );
                (doc.content_revision, doc.cache["deferred"].matches.clone())
            })
            .unwrap();
        assert_eq!(&*reports.borrow(), &[(3, revision)]);
        for _ in 0..2 {
            cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
                .unwrap();
            window
                .update(cx, |doc, _, _| {
                    assert_eq!(doc.content_revision, revision);
                    assert!(Arc::ptr_eq(&matches, &doc.cache["deferred"].matches));
                    assert_eq!(doc.ranges.len(), 3);
                })
                .unwrap();
        }
        assert_eq!(
            reports.borrow().len(),
            1,
            "stable deferred content must not emit repeated search results"
        );
        window
            .update(cx, |doc, window, cx| {
                doc.set_children(vec![doc.children[0].clone()], window, cx);
            })
            .unwrap();
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        window
            .update(cx, |doc, _, _| {
                assert_eq!(doc.cache.len(), 1);
                assert_eq!(doc.content_revision, revision + 1);
                assert_eq!(doc.match_count, 1);
            })
            .unwrap();
        assert_eq!(reports.borrow().last(), Some(&(1, revision + 1)));
    }

    #[gpui::test]
    fn document_revision_is_complete_when_draw_returns(cx: &mut TestAppContext) {
        let handle = cx.add_window(|_, cx| Document::new(props("token", 0, 0), cx));
        let document = handle.update(cx, |_, _, cx| cx.entity()).unwrap();
        let text = cx.update(|cx| cx.new(|_| DeferredText("first token".into())));
        cx.update_window(handle.into(), |_, window, cx| {
            document.update(cx, |doc, cx| doc.set_children(vec![text.clone().into()], window, cx));
            let initial = document.read(cx).content_revision;
            window.draw(cx).clear(cx);
            assert_eq!(document.read(cx).content_revision, initial + 1, "a completed draw must publish its complete content revision before native callers read it");
            text.update(cx, |text, cx| {
                text.0 = "second token".into();
                cx.notify();
            });
            window.draw(cx).clear(cx);
            let snapshot = document.read(cx).snapshot();
            assert_eq!(snapshot.content_revision, initial + 2);
            assert_eq!(snapshot.text[0].text, "second token");
            assert_eq!(snapshot.match_count, 2);
        }).unwrap();
    }

    struct ChangeSearchAfterPaint(gpui::WeakEntity<Document>);
    impl gpui::Render for ChangeSearchAfterPaint {
        fn render(
            &mut self,
            _: &mut gpui::Window,
            _: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            use gpui::prelude::*;
            let owner = self.0.clone();
            gpui::div()
                .child(super::document_text("late", "token"))
                .on_painted(move |_, window, _| {
                    let owner = owner.clone();
                    window.on_draw_complete(move |window, cx| {
                        owner
                            .update(cx, |doc, cx| {
                                if doc.query_revision == 0 {
                                    doc.set_props(props("different", 0, 9), window, cx);
                                }
                            })
                            .unwrap();
                    });
                })
        }
    }
    #[gpui::test]
    fn completion_uses_the_query_that_was_painted(cx: &mut TestAppContext) {
        let handle = cx.add_window(|_, cx| Document::new(props("token", 0, 2), cx));
        let document = handle.update(cx, |_, _, cx| cx.entity()).unwrap();
        cx.update_window(handle.into(), |_, window, cx| {
            let child = cx.new(|_| ChangeSearchAfterPaint(document.downgrade()));
            document.update(cx, |doc, cx| {
                doc.set_children(vec![child.into()], window, cx)
            });
            window.draw(cx).clear(cx);
            let doc = document.read(cx);
            assert_eq!(doc.query_revision, 1);
            assert_eq!(doc.reported_search, Some((doc.content_revision, 0, 2)));
            assert_eq!(doc.snapshot().query.unwrap().query, "token");
            assert_eq!(doc.snapshot().match_index_offset, 2);
            assert_eq!(doc.snapshot().match_count, 1);
        })
        .unwrap();
    }

    #[gpui::test]
    fn native_selected_range_uses_bytes_and_rejects_changed_source(cx: &mut TestAppContext) {
        let window = cx.add_window(|_, cx| Document::new(DocumentProps::default(), cx));
        window
            .update(cx, |doc, _, _| {
                doc.selection
                    .begin_with_span(&"key".into(), &"a😀b".into(), 1..5);
                assert_eq!(doc.selected_range("key", "a😀b"), Some(1..5));
                assert_eq!(doc.selected_range("key", "z😀b"), Some(1..5));
                assert_eq!(doc.selected_range("other", "a😀b"), None);
                assert_eq!(doc.selected_range("key", "axxxx"), None);
                assert_eq!(doc.selected_range("key", "a"), None);
                doc.selection.clear();
                assert_eq!(doc.selected_range("key", "a😀b"), None);
            })
            .unwrap();
    }
}
