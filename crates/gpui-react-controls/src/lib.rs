//! Native controls. Each control is an ordinary GPUI entity with one owner of its state.
pub mod container;
pub mod document;
pub use document::{
    Document, DocumentCommand, DocumentEvent, DocumentProps, DocumentSnapshot, TextKey,
    document_text,
};
pub mod geometry;
pub mod input;
pub mod list;
pub mod text;
pub use container::{Container, ContainerProps};
pub use list::{ListCommand, ListEvent, ListProps, ListSnapshot, VirtualList};
pub use text::{Text, TextProps};
pub mod style {
    pub use gpui_react::style::*;
}
pub use gpui;
pub use input::{Input, InputCommand, InputEvent, InputProps, InputSnapshot};
pub use style::{Color, Length, SharedStyle, Style};

pub fn register(registry: &mut gpui_react::Registry) -> anyhow::Result<()> {
    use gpui_react::{Component, HostElement};
    registry.register(
        Component::<Document>::new("document")
            .children()
            .events()
            .commands()
            .queries(),
    )?;
    registry.register(
        Component::<VirtualList>::new("list")
            .children()
            .events()
            .commands()
            .queries(),
    )?;
    registry.register(
        HostElement::<Container>::new("container")
            .children()
            .events()
            .commands()
            .queries(),
    )?;
    registry.register(HostElement::<Text>::new("text").queries())?;
    registry.register(
        Component::<Input>::new("input")
            .events()
            .commands()
            .queries(),
    )
}
