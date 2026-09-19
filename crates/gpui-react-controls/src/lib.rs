//! Native controls. Each control is an ordinary GPUI entity with one owner of its state.
pub mod container;
pub mod geometry;
pub mod input;
pub mod list;
pub mod text;
pub use container::{Container, ContainerProps};
pub use list::{ListCommand, ListEvent, ListProps, ListSnapshot, VirtualList};
pub use text::{Text, TextProps};
pub mod style;
pub use gpui;
pub use input::{Input, InputCommand, InputEvent, InputProps, InputSnapshot};
pub use style::{Color, Length, Style};

pub fn register(registry: &mut gpui_react::Registry) -> anyhow::Result<()> {
    registry.register(
        gpui_react::Component::<VirtualList>::new("list")
            .children()
            .events()
            .commands()
            .queries(),
    )?;
    registry.register(
        gpui_react::Component::<Container>::new("container")
            .children()
            .events()
            .commands()
            .queries(),
    )?;
    registry.register(gpui_react::Component::<Text>::new("text").queries())?;
    registry.register(
        gpui_react::Component::<Input>::new("input")
            .events()
            .commands()
            .queries(),
    )
}
