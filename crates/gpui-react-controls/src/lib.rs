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

#[cfg(test)]
mod schema {
    /// Every control's derived wire schema must match its serde derive.
    #[test]
    fn schemas_match_serde() {
        let mut registry = gpui_react::Registry::default();
        super::register(&mut registry).unwrap();
        registry.verify_schemas().unwrap();
        let schema = registry.schema();
        assert_eq!(schema.iter().map(|k| k.name.as_str()).collect::<Vec<_>>(), ["document", "list", "container", "text", "input"]);
        let text = &schema[3];
        let gpui_react::Schema::Fields(fields) = text.fields else { panic!("text has a positional schema") };
        assert_eq!(fields.iter().map(|f| f.name).collect::<Vec<_>>(), ["text", "style", "textKey", "selectable", "searchable", "matchIndexOffset", "measure"]);
        assert!(fields.iter().all(|f| !f.required), "every text prop has a default");
        assert_eq!(fields[1].kind, gpui_react::WireType::Style);
        assert_eq!(fields[5].kind, gpui_react::WireType::U32);
        let json = serde_json::to_value(&schema).unwrap();
        assert_eq!(json[3]["fields"][0], serde_json::json!({ "name": "text", "type": "str", "required": false }));
        assert_eq!(json[3]["capabilities"]["queries"], true);
    }
}
