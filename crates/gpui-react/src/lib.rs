//! React bindings for ordinary GPUI views.
//!
//! A component remains a GPUI `Render` implementation. This crate owns the
//! boundary: typed props, optional capabilities, identity, and subscriptions.
//! No JavaScript runtime or worker-side native description tree is required.

mod binding;
mod frame;
mod host;
pub use frame::{FrameInfo, current_frame};
pub mod protocol;

pub use binding::{Component, Emission, EventSink, MountOptions, MountedView, Prepared, Registry};
pub use gpui;
use gpui::{AnyView, Context, EntityId, EventEmitter, Render, Window};
pub use host::Host;
use serde::{Serialize, de::DeserializeOwned};

/// The minimum interface needed to mount a GPUI view from React.
///
/// `set_props` receives a complete validated prop value, including defaults and
/// removals. Preserve native interaction state unless the props explicitly
/// request a replacement. Call `cx.notify()` when the update changes rendering.
pub trait ReactView: Render {
    type Props: DeserializeOwned + 'static;

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
    type Command: DeserializeOwned + 'static;
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
    type Query: DeserializeOwned + 'static;
    type Reply: Serialize;
    fn query(
        &mut self,
        query: Self::Query,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<Self::Reply>;
}

/// Receive ordinary native child views. Child handles do not contain copies of
/// their state. Components keep and render them using normal GPUI composition.
pub trait ReactChildren: ReactView {
    fn set_children(&mut self, children: Vec<AnyView>, window: &mut Window, cx: &mut Context<Self>);

    /// A committed prop, invoked command, or descendant-structure update can change a
    /// child's intrinsic size. Called once per affected direct child before the
    /// next command/query or transaction end. Containers with native caches can
    /// invalidate those entries; ordinary containers need no extra work.
    /// A command can change state before returning an error, so it also counts.
    /// Native changes outside bridge transactions still use GPUI's own APIs.
    fn children_changed(
        &mut self,
        _children: &[EntityId],
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }
}
