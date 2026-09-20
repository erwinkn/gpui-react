//! The standard controls for `gpui-react`: `Container`, `Text`, `VirtualList`,
//! `Document`, and `Input`, with the `Style` they share.
//!
//! The engine registers no component kinds. `register_kit` declares `Style`
//! as the session's shared definition type and registers the five controls;
//! a runtime or a custom composition calls it before registering its own
//! components.

pub mod container;
pub mod document;
pub mod geometry;
pub mod input;
pub mod list;
pub mod style;
pub mod text;

pub use container::{Container, ContainerProps};
pub use document::{
    Document, DocumentCommand, DocumentEvent, DocumentProps, DocumentSnapshot, TextKey,
    document_text,
};
pub use input::{Input, InputCommand, InputEvent, InputProps, InputSnapshot};
pub use list::{ListCommand, ListEvent, ListProps, ListSnapshot, VirtualList};
pub use style::{Color, Length, SharedStyle, Style};
pub use text::{Text, TextProps};

use gpui_react::{Component, HostElement, Registry};

/// Declares `Style` as the shared definition type and registers the five
/// standard controls: `document`, `list`, `container`, `text`, and `input`.
pub fn register_kit(registry: &mut Registry) -> anyhow::Result<()> {
    registry.shared::<Style>()?;
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
mod kit_schema {
    /// Every control's derived wire schema must match its serde derive.
    #[test]
    fn schemas_match_serde() {
        let mut registry = gpui_react::Registry::default();
        super::register_kit(&mut registry).unwrap();
        registry.verify_schemas().unwrap();
        let schema = registry.schema();
        assert_eq!(
            schema.iter().map(|k| k.name.as_str()).collect::<Vec<_>>(),
            ["document", "list", "container", "text", "input"]
        );
        let text = &schema[3];
        let gpui_react::Schema::Fields(fields) = text.fields else {
            panic!("text has a positional schema")
        };
        assert_eq!(
            fields.iter().map(|f| f.name).collect::<Vec<_>>(),
            [
                "text",
                "style",
                "textKey",
                "selectable",
                "searchable",
                "matchIndexOffset",
                "measure"
            ]
        );
        assert!(fields.iter().all(|f| !f.required), "every text prop has a default");
        assert_eq!(fields[1].kind, gpui_react::WireType::Style);
        assert_eq!(fields[5].kind, gpui_react::WireType::U32);
        let json = serde_json::to_value(&schema).unwrap();
        assert_eq!(
            json[3]["fields"][0],
            serde_json::json!({ "name": "text", "type": "str", "required": false })
        );
        assert_eq!(json[3]["capabilities"]["queries"], true);
    }

    /// The shared definition type is declared once per registry.
    #[test]
    fn register_kit_twice_fails() {
        let mut registry = gpui_react::Registry::default();
        super::register_kit(&mut registry).unwrap();
        assert!(super::register_kit(&mut registry).is_err());
    }
}
