//! React bindings for GPUI.
//!
//! One `Host` entity per window root owns the committed React tree as plain
//! data and rebuilds ephemeral GPUI elements from it every frame. Nodes with
//! native state (inputs, lists, custom components) are ordinary GPUI entities
//! listed in that tree. No JavaScript runtime or second native tree exists.

mod decode;
mod frame;
mod host;
pub mod protocol;
mod registry;
pub mod wire;
pub mod style;

pub use decode::Decoder;
pub use frame::{FrameInfo, current_frame};
pub use gpui;
pub use host::{Children, ElementContext, Host, RenderContext};
pub use registry::{
    Capabilities, Component, Emission, Emitter, EventSink, HostElement, KindSchema, Prepared,
    Registry,
};
pub use style::{Color, Length, SharedStyle, Style};
pub use gpui_react_macros::ComponentProps;
pub use wire::{Field, Schema, WireType};

use gpui::{AnyElement, Context, EventEmitter, Render, Window};
use serde::{Serialize, de::DeserializeOwned};

/// A React component backed by a GPUI entity. Use this for anything with native
/// state: editors, lists, animations, GPU resources, or code that needs layout.
///
/// `set_props` receives a complete validated prop value, including defaults and
/// removals. Preserve native interaction state unless the props explicitly
/// request a replacement. Call `cx.notify()` when the update changes rendering.
pub trait ReactView: Render {
    type Props: DeserializeOwned + wire::ComponentProps + Send + 'static;

    fn create(props: Self::Props, window: &mut Window, cx: &mut Context<Self>) -> Self;
    fn set_props(&mut self, props: Self::Props, window: &mut Window, cx: &mut Context<Self>);

    /// Runs after creation and event subscription, before the first layout.
    fn mounted(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}

    /// Release resources that require access to the live window or app.
    /// Ordinary owned resources still use Rust's normal drop behavior.
    fn unmounting(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}
}

/// One native event type, usually an enum, delivered asynchronously to `onEvent`.
pub trait ReactEvents: ReactView + EventEmitter<Self::Event> {
    type Event: Serialize + 'static;
}

pub trait ReactCommands: ReactView {
    type Command: DeserializeOwned + Send + 'static;
    fn command(
        &mut self,
        command: Self::Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()>;
}

/// A query observes native state when it executes on the UI thread. It does not
/// implicitly perform layout. Painted measurements should include `current_frame`
/// metadata recorded during paint.
pub trait ReactQueries: ReactView {
    type Query: DeserializeOwned + Send + 'static;
    type Reply: Serialize;
    fn query(
        &mut self,
        query: Self::Query,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<Self::Reply>;
}

/// Receive React children. The handle renders children on demand from the host
/// tree, so a list can build only its visible rows. It is delivered at mount
/// and again whenever the ordered list of visible children changes.
pub trait ReactChildren: ReactView {
    fn set_children(&mut self, children: Children, window: &mut Window, cx: &mut Context<Self>);

    /// A committed prop change or command touched a node inside the given
    /// direct child. Containers that cache row geometry can invalidate it.
    fn child_changed(&mut self, _child: u32, _window: &mut Window, _cx: &mut Context<Self>) {}
}

/// A React component that is plain data owned by the host. It has no entity and
/// no persistent GPUI element; the host renders it into elements each frame.
/// Interaction state that GPUI keeps by element id (hover, scroll offsets)
/// persists because `RenderContext::element_id` is stable for the node.
pub trait ReactElement: 'static {
    type Props: DeserializeOwned + wire::ComponentProps + Send + 'static;
    /// Sparse per-kind storage for what only some nodes have, keyed by the
    /// row's slot (`cx.slot`): handles, labels, measured geometry. The row
    /// itself stays small and dense. Use `()` when nothing is rare.
    type Extras: Default + 'static;

    fn create(props: Self::Props, extras: &mut Self::Extras, cx: &mut ElementContext) -> Self;
    fn set_props(&mut self, props: Self::Props, extras: &mut Self::Extras, cx: &mut ElementContext);
    fn render(&self, extras: &Self::Extras, cx: &mut RenderContext) -> AnyElement;
    fn unmount(&mut self, _extras: &mut Self::Extras, _cx: &mut ElementContext) {}
}

pub trait ElementCommands: ReactElement {
    type Command: DeserializeOwned + Send + 'static;
    fn command(
        &mut self,
        command: Self::Command,
        extras: &mut Self::Extras,
        cx: &mut ElementContext,
    ) -> anyhow::Result<()>;
}

pub trait ElementQueries: ReactElement {
    type Query: DeserializeOwned + Send + 'static;
    type Reply: Serialize;
    fn query(
        &mut self,
        query: Self::Query,
        extras: &mut Self::Extras,
        cx: &mut ElementContext,
    ) -> anyhow::Result<Self::Reply>;
}
