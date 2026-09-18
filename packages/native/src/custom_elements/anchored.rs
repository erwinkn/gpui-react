/// Anchored custom element for deferred, trigger-relative floating layers.
use super::{CustomElement, CustomElementFactory, CustomRenderContext};

pub struct AnchoredFactory;

impl CustomElementFactory for AnchoredFactory {
    fn element_type(&self) -> &str {
        "anchored"
    }

    fn create(&self, _id: u64) -> Box<dyn CustomElement> {
        Box::new(AnchoredElement::default())
    }
}

#[derive(Debug, Clone, Copy)]
enum AnchorPoint {
    TopLeft,
    TopCenter,
    TopRight,
    RightCenter,
    BottomRight,
    BottomCenter,
    BottomLeft,
    LeftCenter,
}

impl AnchorPoint {
    fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "topLeft" => Self::TopLeft,
            "topCenter" => Self::TopCenter,
            "topRight" => Self::TopRight,
            "rightCenter" => Self::RightCenter,
            "bottomRight" => Self::BottomRight,
            "bottomCenter" => Self::BottomCenter,
            "bottomLeft" => Self::BottomLeft,
            "leftCenter" => Self::LeftCenter,
            _ => return None,
        })
    }

    fn as_gpui(self) -> gpui::Anchor {
        match self {
            Self::TopLeft => gpui::Anchor::TopLeft,
            Self::TopCenter => gpui::Anchor::TopCenter,
            Self::TopRight => gpui::Anchor::TopRight,
            Self::RightCenter => gpui::Anchor::RightCenter,
            Self::BottomRight => gpui::Anchor::BottomRight,
            Self::BottomCenter => gpui::Anchor::BottomCenter,
            Self::BottomLeft => gpui::Anchor::BottomLeft,
            Self::LeftCenter => gpui::Anchor::LeftCenter,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
enum Side {
    Top,
    Right,
    #[default]
    Bottom,
    Left,
}

impl Side {
    fn from_str(value: &str) -> Self {
        match value {
            "top" => Self::Top,
            "right" => Self::Right,
            "left" => Self::Left,
            _ => Self::Bottom,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
enum Alignment {
    #[default]
    Start,
    Center,
    End,
}

impl Alignment {
    fn from_str(value: &str) -> Self {
        match value {
            "center" => Self::Center,
            "end" => Self::End,
            _ => Self::Start,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
enum FitMode {
    Switch,
    #[default]
    Snap,
}

impl FitMode {
    fn from_str(value: &str) -> Self {
        if value == "switch" {
            Self::Switch
        } else {
            Self::Snap
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnchoredElement {
    position: Option<(f32, f32)>,
    selection_anchor: bool,
    selection_element: Option<u64>,
    match_width: bool,
    side: Side,
    align: Alignment,
    anchor: Option<AnchorPoint>,
    gap: f32,
    offset: (f32, f32),
    fit: FitMode,
    snap_margin: f32,
    deferred: bool,
    priority: usize,
    occlude: bool,
}

impl Default for AnchoredElement {
    fn default() -> Self {
        Self {
            position: None,
            selection_anchor: false,
            selection_element: None,
            match_width: false,
            side: Side::Bottom,
            align: Alignment::Start,
            anchor: None,
            gap: 0.0,
            offset: (0.0, 0.0),
            fit: FitMode::Snap,
            snap_margin: 8.0,
            deferred: true,
            priority: 1,
            occlude: true,
        }
    }
}

impl AnchoredElement {
    fn resolved_anchor(&self) -> AnchorPoint {
        if let Some(anchor) = self.anchor {
            return anchor;
        }
        match (self.side, self.align) {
            (Side::Top, Alignment::Start) => AnchorPoint::BottomLeft,
            (Side::Top, Alignment::Center) => AnchorPoint::BottomCenter,
            (Side::Top, Alignment::End) => AnchorPoint::BottomRight,
            (Side::Right, Alignment::Start) => AnchorPoint::TopLeft,
            (Side::Right, Alignment::Center) => AnchorPoint::LeftCenter,
            (Side::Right, Alignment::End) => AnchorPoint::BottomLeft,
            (Side::Bottom, Alignment::Start) => AnchorPoint::TopLeft,
            (Side::Bottom, Alignment::Center) => AnchorPoint::TopCenter,
            (Side::Bottom, Alignment::End) => AnchorPoint::TopRight,
            (Side::Left, Alignment::Start) => AnchorPoint::TopRight,
            (Side::Left, Alignment::Center) => AnchorPoint::RightCenter,
            (Side::Left, Alignment::End) => AnchorPoint::BottomRight,
        }
    }

    fn resolved_offset(&self) -> gpui::Point<gpui::Pixels> {
        let (side_x, side_y) = match self.side {
            Side::Top => (0.0, -self.gap),
            Side::Right => (self.gap, 0.0),
            Side::Bottom => (0.0, self.gap),
            Side::Left => (-self.gap, 0.0),
        };
        gpui::point(
            gpui::px(side_x + self.offset.0),
            gpui::px(side_y + self.offset.1),
        )
    }

    fn wrap_at_trigger(&self, layer: gpui::AnyElement) -> gpui::AnyElement {
        use gpui::prelude::*;

        if self.match_width {
            let wrapper = gpui::div().absolute().left_0().w_full().h_0().child(layer);
            return match self.side {
                Side::Top => wrapper.top_0().into_any_element(),
                _ => wrapper.bottom_0().into_any_element(),
            };
        }

        match (self.side, self.align) {
            (Side::Top, Alignment::Start) => gpui::div()
                .absolute()
                .top_0()
                .left_0()
                .size_0()
                .child(layer)
                .into_any_element(),
            (Side::Top, Alignment::Center) => gpui::div()
                .absolute()
                .top_0()
                .left_0()
                .h_0()
                .w_full()
                .flex()
                .justify_center()
                .child(layer)
                .into_any_element(),
            (Side::Top, Alignment::End) => gpui::div()
                .absolute()
                .top_0()
                .right_0()
                .size_0()
                .child(layer)
                .into_any_element(),
            (Side::Right, Alignment::Start) => gpui::div()
                .absolute()
                .top_0()
                .right_0()
                .size_0()
                .child(layer)
                .into_any_element(),
            (Side::Right, Alignment::Center) => gpui::div()
                .absolute()
                .top_0()
                .right_0()
                .w_0()
                .h_full()
                .flex()
                .items_center()
                .child(layer)
                .into_any_element(),
            (Side::Right, Alignment::End) => gpui::div()
                .absolute()
                .bottom_0()
                .right_0()
                .size_0()
                .child(layer)
                .into_any_element(),
            (Side::Bottom, Alignment::Start) => gpui::div()
                .absolute()
                .bottom_0()
                .left_0()
                .size_0()
                .child(layer)
                .into_any_element(),
            (Side::Bottom, Alignment::Center) => gpui::div()
                .absolute()
                .bottom_0()
                .left_0()
                .h_0()
                .w_full()
                .flex()
                .justify_center()
                .child(layer)
                .into_any_element(),
            (Side::Bottom, Alignment::End) => gpui::div()
                .absolute()
                .bottom_0()
                .right_0()
                .size_0()
                .child(layer)
                .into_any_element(),
            (Side::Left, Alignment::Start) => gpui::div()
                .absolute()
                .top_0()
                .left_0()
                .size_0()
                .child(layer)
                .into_any_element(),
            (Side::Left, Alignment::Center) => gpui::div()
                .absolute()
                .top_0()
                .left_0()
                .w_0()
                .h_full()
                .flex()
                .items_center()
                .child(layer)
                .into_any_element(),
            (Side::Left, Alignment::End) => gpui::div()
                .absolute()
                .bottom_0()
                .left_0()
                .size_0()
                .child(layer)
                .into_any_element(),
        }
    }
}

impl CustomElement for AnchoredElement {
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<crate::renderer::GpuixView>,
    ) -> gpui::AnyElement {
        use gpui::prelude::*;

        // The overlay, not the trigger, is what a user clicks, so the recorded
        // box has to be this content div after `anchored` placed it. Only gpui
        // knows where that is, so it reports its own painted box instead of
        // carrying a `bounds_tracker` child.
        let mut content = gpui::div()
            .id(gpui::SharedString::from(format!(
                "__gpuix_anchored_{}",
                ctx.id
            )))
            .flex_col();
        if self.match_width {
            content = content.w_full();
        }
        content = crate::automation::track_own_bounds(content, ctx.id);
        if let Some(style) = ctx.style {
            content = crate::renderer::apply_interactive_styles(content, style);
        }
        content = crate::accessibility::apply_accessibility(content, ctx.props, None);
        content = super::wire_standard_events(content, &ctx);
        // Deferred overlays paint over the window blur. A missing fill lets the
        // page show through the card. Force an opaque surface when JS omitted one.
        let has_fill = ctx
            .style
            .and_then(crate::style::StyleDesc::resolved_background)
            .is_some();
        if !has_fill {
            content = content.bg(gpui::rgb(0x1A1A1A));
        }
        if self.occlude {
            content = content.occlude();
        }
        for child in ctx.children {
            content = content.child(child);
        }

        let mut anchored = gpui::anchored()
            .anchor(self.resolved_anchor().as_gpui())
            .offset(self.resolved_offset());
        if self.match_width {
            anchored = anchored.match_parent_width();
        }
        if let Some((x, y)) = self.position {
            anchored = anchored.position(gpui::point(gpui::px(x), gpui::px(y)));
        }
        if matches!(self.fit, FitMode::Snap) {
            anchored = anchored.snap_to_window_with_margin(gpui::px(self.snap_margin));
        }

        if self.selection_anchor {
            content =
                content.on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation());
            return gpui::deferred(SelectionLayer {
                inner: Some(anchored.anchor(gpui::Anchor::TopCenter).child(content)),
                selection: ctx.selection.clone(),
                element: self.selection_element,
                visible: false,
            })
            .with_priority(ctx.overlay_priority)
            .into_any_element();
        }
        let anchored = anchored.child(content);
        let layer = if self.deferred {
            gpui::deferred(anchored)
                .with_priority(ctx.overlay_priority)
                .into_any_element()
        } else {
            anchored.into_any_element()
        };

        if self.position.is_some() {
            layer
        } else {
            self.wrap_at_trigger(layer)
        }
    }

    fn set_prop(&mut self, key: &str, value: serde_json::Value) {
        match key {
            "matchWidth" => self.match_width = value.as_bool().unwrap_or(false),
            "selectionAnchor" => self.selection_anchor = value.as_bool().unwrap_or(false),
            "selectionElement" => self.selection_element = value.as_u64(),
            "position" => {
                self.position = value.as_object().and_then(|position| {
                    Some((
                        position.get("x")?.as_f64()? as f32,
                        position.get("y")?.as_f64()? as f32,
                    ))
                });
            }
            "side" => self.side = value.as_str().map(Side::from_str).unwrap_or_default(),
            "align" => self.align = value.as_str().map(Alignment::from_str).unwrap_or_default(),
            "anchor" => self.anchor = value.as_str().and_then(AnchorPoint::from_str),
            "gap" => self.gap = value.as_f64().unwrap_or(0.0) as f32,
            "offset" => {
                self.offset = value
                    .as_object()
                    .map(|offset| {
                        (
                            offset
                                .get("x")
                                .and_then(|value| value.as_f64())
                                .unwrap_or(0.0) as f32,
                            offset
                                .get("y")
                                .and_then(|value| value.as_f64())
                                .unwrap_or(0.0) as f32,
                        )
                    })
                    .unwrap_or_default();
            }
            "fit" => self.fit = value.as_str().map(FitMode::from_str).unwrap_or_default(),
            "snapMargin" => self.snap_margin = value.as_f64().unwrap_or(8.0) as f32,
            "deferred" => self.deferred = value.as_bool().unwrap_or(true),
            "priority" => {
                self.priority = value
                    .as_u64()
                    .and_then(|priority| usize::try_from(priority).ok())
                    .unwrap_or(1);
            }
            "occlude" => self.occlude = value.as_bool().unwrap_or(true),
            _ => {}
        }
    }

    fn supported_props(&self) -> &'static [&'static str] {
        &[
            "position",
            "matchWidth",
            "selectionAnchor",
            "selectionElement",
            "side",
            "align",
            "anchor",
            "gap",
            "offset",
            "fit",
            "snapMargin",
            "deferred",
            "priority",
            "occlude",
        ]
    }

    fn supported_events(&self) -> &'static [&'static str] {
        &["click", "mouseEnter", "mouseLeave", "fileDrop"]
    }

    fn destroy(&mut self) {}
}

/// Resolve the selected text's last visible line after all normal text has
/// prepainted. Deferred prepaint supplies current-frame geometry, so scroll,
/// font and width changes need no JS correction or extra frame.
struct SelectionLayer {
    inner: Option<gpui::Anchored>,
    selection: crate::text::SharedSelection,
    element: Option<u64>,
    visible: bool,
}
impl gpui::IntoElement for SelectionLayer {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl gpui::Element for SelectionLayer {
    type RequestLayoutState = <gpui::Anchored as gpui::Element>::RequestLayoutState;
    type PrepaintState = ();
    fn id(&self) -> Option<gpui::ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        id: Option<&gpui::GlobalElementId>,
        inspector: Option<&gpui::InspectorElementId>,
        window: &mut gpui::Window,
        cx: &mut gpui::App,
    ) -> (gpui::LayoutId, Self::RequestLayoutState) {
        self.inner
            .as_mut()
            .unwrap()
            .request_layout(id, inspector, window, cx)
    }
    fn prepaint(
        &mut self,
        id: Option<&gpui::GlobalElementId>,
        inspector: Option<&gpui::InspectorElementId>,
        bounds: gpui::Bounds<gpui::Pixels>,
        state: &mut Self::RequestLayoutState,
        window: &mut gpui::Window,
        cx: &mut gpui::App,
    ) {
        let rects: Vec<_> = self
            .selection
            .lock()
            .geometry
            .iter()
            .filter(|r| self.element.map(|id| r.element_id == id).unwrap_or(true))
            .flat_map(|r| r.rects.iter())
            .copied()
            .collect();
        let selected = rects.last().copied().map(|(_, y, _, h)| {
            let line: Vec<_> = rects.iter().filter(|r| (r.1 - y).abs() < 1.0).collect();
            let left = line.iter().map(|r| r.0).fold(f32::INFINITY, f32::min);
            let right = line
                .iter()
                .map(|r| r.0 + r.2)
                .fold(f32::NEG_INFINITY, f32::max);
            (left, y, right - left, h)
        });
        self.visible = selected.is_some();
        if let Some((x, y, w, h)) = selected {
            let inner = self.inner.take().unwrap();
            self.inner = Some(inner.position(gpui::point(gpui::px(x + w / 2.0), gpui::px(y + h))));
            self.inner
                .as_mut()
                .unwrap()
                .prepaint(id, inspector, bounds, state, window, cx);
        }
    }
    fn paint(
        &mut self,
        id: Option<&gpui::GlobalElementId>,
        inspector: Option<&gpui::InspectorElementId>,
        bounds: gpui::Bounds<gpui::Pixels>,
        state: &mut Self::RequestLayoutState,
        prepaint: &mut (),
        window: &mut gpui::Window,
        cx: &mut gpui::App,
    ) {
        if self.visible {
            self.inner
                .as_mut()
                .unwrap()
                .paint(id, inspector, bounds, state, prepaint, window, cx);
        }
    }
}
