use gpui::{
    AnyElement, App, Bounds, Element, ElementId, Global, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, Pixels, Window, WindowId,
};
use serde::Serialize;

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
#[derive(Clone, Copy, Default)]
struct ActiveFrame(Option<(WindowId, FrameInfo)>);
impl Global for ActiveFrame {}

/// Available during paint below a Host. Outside that scope, returns None.
/// A native component can attach this to its own painted geometry or text data.
pub fn current_frame(window: &Window, cx: &App) -> Option<FrameInfo> {
    cx.try_global::<ActiveFrame>()?
        .0
        .filter(|(id, _)| *id == window.window_handle().window_id())
        .map(|(_, frame)| frame)
}

/// Delegates the element lifecycle without adding a layout box. The paint scope
/// nests correctly if an application embeds more than one Host in a window.
pub(crate) struct FrameScope {
    pub child: AnyElement,
    pub info: FrameInfo,
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
        (self.child.request_layout(window, cx), ())
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
        self.child.prepaint(window, cx);
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
        if !cx.has_global::<ActiveFrame>() {
            cx.set_global(ActiveFrame::default());
        }
        let previous = cx
            .global_mut::<ActiveFrame>()
            .0
            .replace((window.window_handle().window_id(), self.info));
        self.child.paint(window, cx);
        cx.global_mut::<ActiveFrame>().0 = previous;
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
