use crate::{SharedStyle, Style, geometry::Painted};
use gpui::{prelude::*, *};
use gpui_react::{ElementContext, ElementQueries, ReactElement, RenderContext, Shared};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, gpui_react::ComponentProps)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct TextProps {
    /// Decoded straight into GPUI's string type: one copy from the wire.
    pub text: SharedString,
    pub style: Shared<Style>,
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
const HAS_KEY: u8 = 8;
const NO_OFFSET: u32 = u32::MAX;

/// What only some text nodes carry, keyed by row slot: an app-defined
/// selection key and the last painted bounds.
#[derive(Default)]
pub struct TextExtras {
    keys: FxHashMap<u32, SharedString>,
    painted: FxHashMap<u32, Painted>,
}

/// Native text as a host-owned row, 48 bytes. Inside a Document it takes part
/// in selection and search, keyed by the app's `textKey` or by the node id.
pub struct Text {
    text: SharedString,
    style: SharedStyle,
    match_index_offset: u32,
    revision: u32,
    flags: u8,
}
impl Text {
    fn flags(props: &TextProps, extras: &mut TextExtras, slot: u32) -> u8 {
        let key = match &props.text_key {
            Some(key) => {
                extras.keys.insert(slot, key.clone());
                HAS_KEY
            }
            None => {
                extras.keys.remove(&slot);
                0
            }
        };
        (props.selectable as u8 * SELECTABLE)
            | (props.searchable as u8 * SEARCHABLE)
            | (props.measure as u8 * MEASURE)
            | key
    }
}
impl ReactElement for Text {
    type Props = TextProps;
    type Extras = TextExtras;
    fn create(props: TextProps, extras: &mut TextExtras, cx: &mut ElementContext) -> Self {
        Self {
            flags: Self::flags(&props, extras, cx.slot),
            match_index_offset: props.match_index_offset.unwrap_or(NO_OFFSET),
            text: props.text,
            style: props.style,
            revision: 0,
        }
    }
    fn set_props(&mut self, props: TextProps, extras: &mut TextExtras, cx: &mut ElementContext) {
        self.flags = Self::flags(&props, extras, cx.slot);
        self.match_index_offset = props.match_index_offset.unwrap_or(NO_OFFSET);
        self.text = props.text;
        self.style = props.style;
        self.revision += 1;
    }
    fn render(&self, extras: &TextExtras, cx: &mut RenderContext) -> AnyElement {
        let key = if self.flags & HAS_KEY != 0 {
            crate::TextKey::Named(extras.keys[&cx.slot].clone())
        } else {
            crate::TextKey::Node(cx.id)
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
                        host.update_element::<Text, _>(id, |_, extras, slot| {
                            extras.painted.insert(slot, painted);
                        });
                    })
                    .ok();
                })
                .child(element)
                .into_any_element();
        }
        element
    }
    fn unmount(&mut self, extras: &mut TextExtras, cx: &mut ElementContext) {
        extras.keys.remove(&cx.slot);
        extras.painted.remove(&cx.slot);
    }
}
impl ElementQueries for Text {
    type Query = ();
    type Reply = TextSnapshot;
    fn query(&mut self, _: (), extras: &mut TextExtras, cx: &mut ElementContext) -> anyhow::Result<Self::Reply> {
        Ok(TextSnapshot {
            text: self.text.to_string(),
            revision: self.revision,
            painted: extras.painted.get(&cx.slot).copied(),
        })
    }
}

#[cfg(test)]
mod layout {
    #[test]
    fn text_row_stays_small() {
        assert!(std::mem::size_of::<super::Text>() <= 48, "{}", std::mem::size_of::<super::Text>());
    }
}
