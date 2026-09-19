use crate::{Style, geometry::Painted};
use gpui::{prelude::*, *};
use gpui_react::{ReactQueries, ReactView};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct TextProps {
    pub text: String,
    pub style: Style,
    pub text_key: Option<String>,
    pub selectable: bool,
    pub searchable: bool,
    pub match_index_offset: Option<usize>,
}
impl Default for TextProps {
    fn default() -> Self {
        Self {
            text: String::new(),
            style: Style::default(),
            text_key: None,
            selectable: true,
            searchable: true,
            match_index_offset: None,
        }
    }
}
#[derive(Debug, Serialize)]
pub struct TextSnapshot {
    pub text: String,
    pub revision: u64,
    pub painted: Option<Painted>,
}
/// Native text. The owning entity also supplies inspection of its latest paint.
pub struct Text {
    text: SharedString,
    style: Style,
    revision: u64,
    painted: Option<Painted>,
    text_key: Option<SharedString>,
    selectable: bool,
    searchable: bool,
    match_index_offset: Option<usize>,
}
impl Text {
    pub fn new(props: TextProps) -> Self {
        Self {
            text: props.text.into(),
            style: props.style,
            revision: 0,
            painted: None,
            text_key: props.text_key.map(Into::into),
            selectable: props.selectable,
            searchable: props.searchable,
            match_index_offset: props.match_index_offset,
        }
    }
}
impl Render for Text {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.weak_entity();
        let revision = self.revision;
        let key = self
            .text_key
            .get_or_insert_with(|| format!("text:{}", cx.entity_id().as_u64()).into())
            .clone();
        let mut text = crate::document_text(key, self.text.clone())
            .selectable(self.selectable)
            .searchable(self.searchable);
        if let Some(offset) = self.match_index_offset {
            text = text.match_index_offset(offset);
        }
        self.style
            .apply_interactive(div().id("text"))
            .on_painted(move |bounds, window, cx| {
                this.update(cx, |this, cx| {
                    this.painted = Some(Painted {
                        bounds: bounds.into(),
                        revision,
                        frame: gpui_react::current_frame(window, cx),
                    })
                })
                .ok();
            })
            .child(text)
    }
}
impl ReactView for Text {
    type Props = TextProps;
    fn create(props: Self::Props, _: &mut Window, _: &mut Context<Self>) -> Self {
        Self::new(props)
    }
    fn set_props(&mut self, props: Self::Props, _: &mut Window, cx: &mut Context<Self>) {
        self.text = props.text.into();
        self.style = props.style;
        self.text_key = Some(
            props
                .text_key
                .map(Into::into)
                .unwrap_or_else(|| format!("text:{}", cx.entity_id().as_u64()).into()),
        );
        self.selectable = props.selectable;
        self.searchable = props.searchable;
        self.match_index_offset = props.match_index_offset;
        self.revision += 1;
        cx.notify();
    }
}
impl ReactQueries for Text {
    type Query = ();
    type Reply = TextSnapshot;
    fn query(
        &mut self,
        _: (),
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> anyhow::Result<Self::Reply> {
        Ok(TextSnapshot {
            text: self.text.to_string(),
            revision: self.revision,
            painted: self.painted,
        })
    }
}
