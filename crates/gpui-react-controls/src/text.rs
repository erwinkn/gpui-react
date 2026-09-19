use crate::{SharedStyle, geometry::Painted};
use gpui::{prelude::*, *};
use gpui_react::{ElementContext, ElementQueries, ReactElement, RenderContext};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct TextProps {
    /// Decoded straight into GPUI's string type: one copy from the wire.
    pub text: SharedString,
    pub style: SharedStyle,
    pub text_key: Option<SharedString>,
    pub selectable: bool,
    pub searchable: bool,
    pub match_index_offset: Option<u32>,
    /// Record painted bounds for queries. Off by default: it costs a paint
    /// callback per frame.
    pub measure: bool,
}
impl Default for TextProps {
    fn default() -> Self {
        Self {
            text: SharedString::default(),
            style: SharedStyle::default(),
            text_key: None,
            selectable: true,
            searchable: true,
            match_index_offset: None,
            measure: false,
        }
    }
}
#[derive(Debug, Serialize)]
pub struct TextSnapshot {
    pub text: String,
    pub revision: u32,
    pub painted: Option<Painted>,
}

const SELECTABLE: u8 = 1;
const SEARCHABLE: u8 = 2;
const MEASURE: u8 = 4;
const NO_OFFSET: u32 = u32::MAX;

/// Native text as a host-owned row, 80 bytes. Inside a Document it takes part
/// in selection and search, keyed by the app's `textKey` or by the node id.
pub struct Text {
    text: SharedString,
    key: Option<SharedString>,
    style: SharedStyle,
    painted: Option<Box<Painted>>,
    match_index_offset: u32,
    revision: u32,
    flags: u8,
}
impl Text {
    fn key(props: &TextProps) -> Option<SharedString> {
        props.text_key.clone()
    }
    fn flags(props: &TextProps) -> u8 {
        (props.selectable as u8 * SELECTABLE)
            | (props.searchable as u8 * SEARCHABLE)
            | (props.measure as u8 * MEASURE)
    }
}
impl ReactElement for Text {
    type Props = TextProps;
    fn create(props: TextProps, _: &mut ElementContext) -> Self {
        Self {
            key: Self::key(&props),
            flags: Self::flags(&props),
            match_index_offset: props.match_index_offset.unwrap_or(NO_OFFSET),
            text: props.text,
            style: props.style,
            painted: None,
            revision: 0,
        }
    }
    fn set_props(&mut self, props: TextProps, _: &mut ElementContext) {
        self.key = Self::key(&props);
        self.flags = Self::flags(&props);
        self.match_index_offset = props.match_index_offset.unwrap_or(NO_OFFSET);
        self.text = props.text;
        self.style = props.style;
        self.revision += 1;
    }
    fn render(&self, cx: &mut RenderContext) -> AnyElement {
        let key = match &self.key {
            Some(name) => crate::TextKey::Named(name.clone()),
            None => crate::TextKey::Node(cx.id),
        };
        let mut text = crate::document_text(key, self.text.clone())
            .selectable(self.flags & SELECTABLE != 0)
            .searchable(self.flags & SEARCHABLE != 0);
        if self.match_index_offset != NO_OFFSET {
            text = text.match_index_offset(self.match_index_offset as usize);
        }
        // Plain text needs no GPUI element state; skip the identified path
        // unless the style has hover, active, or focus variants.
        let mut element = if self.style.is_interactive() {
            self.style
                .apply_interactive(div().id(cx.element_id()))
                .child(text)
                .into_any_element()
        } else {
            self.style.apply(div()).child(text).into_any_element()
        };
        if self.flags & MEASURE != 0 {
            let host = cx.host();
            let id = cx.id;
            let revision = self.revision;
            element = div()
                .on_painted(move |bounds, window, cx| {
                    let painted = Painted {
                        bounds: bounds.into(),
                        revision: revision as u64,
                        frame: gpui_react::current_frame(window, cx),
                    };
                    host.update(cx, |host, _| {
                        host.update_element::<Text, _>(id, |text| {
                            text.painted = Some(Box::new(painted))
                        });
                    })
                    .ok();
                })
                .child(element)
                .into_any_element();
        }
        element
    }
}
impl ElementQueries for Text {
    type Query = ();
    type Reply = TextSnapshot;
    fn query(&mut self, _: (), _: &mut ElementContext) -> anyhow::Result<Self::Reply> {
        Ok(TextSnapshot {
            text: self.text.to_string(),
            revision: self.revision,
            painted: self.painted.as_deref().copied(),
        })
    }
}

#[cfg(test)]
mod layout {
    #[test]
    fn text_row_stays_small() {
        assert!(std::mem::size_of::<super::Text>() <= 80, "{}", std::mem::size_of::<super::Text>());
    }
}
