use gpui::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, Pixels, Window,
};
use serde::Serialize;
use std::rc::Rc;

/// Identifies the native draw that produced a measurement. This is a GPUI draw,
/// not evidence that the OS presented the pixels on a physical display.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameInfo {
    #[serde(serialize_with = "serialize_root")]
    pub root: u64,
    pub frame: u64,
    pub commit: u64,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub scale_factor: f32,
}
/// Available while drawing elements below a Host, including deferred elements.
/// Outside that scope, returns None. Attach it to geometry during paint; earlier
/// lifecycle phases do not prove that an element will reach the painted frame.
pub fn current_frame(window: &Window, _cx: &App) -> Option<FrameInfo> {
    window.element_context::<FrameInfo>().copied()
}

/// Delegates the element lifecycle without adding a layout box. The draw scope
/// nests correctly if an application embeds more than one Host in a window.
pub(crate) struct FrameScope {
    pub child: AnyElement,
    pub info: Rc<FrameInfo>,
}
impl Element for FrameScope {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        None
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
        let layout = window.with_element_context(self.info.clone(), |window| {
            self.child.request_layout(window, cx)
        });
        (layout, ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_element_context(self.info.clone(), |window| {
            self.child.prepaint(window, cx);
        });
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_element_context(self.info.clone(), |window| {
            self.child.paint(window, cx);
        });
    }
}
impl IntoElement for FrameScope {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

fn serialize_root<S: serde::Serializer>(root: &u64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_str(root)
}
