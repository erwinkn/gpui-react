//! Native controls. Each control is an ordinary GPUI entity with one owner of its state.
pub mod input;
pub mod style;
pub use gpui;
pub use input::{Input, InputCommand, InputEvent, InputProps, InputSnapshot};
pub use style::{Color, Length, Style};

pub fn register(registry: &mut gpui_react::Registry) -> anyhow::Result<()> {
    registry.register(
        gpui_react::Component::<Input>::new("input")
            .events()
            .commands()
            .queries(),
    )
}
