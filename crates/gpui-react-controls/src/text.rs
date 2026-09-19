use crate::{Style, geometry::Painted};
use gpui::{prelude::*, *};
use gpui_react::{ReactQueries, ReactView};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct TextProps {
    pub text: String,
    pub style: Style,
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
}
impl Text {
    pub fn new(props: TextProps) -> Self {
        Self {
            text: props.text.into(),
            style: props.style,
            revision: 0,
            painted: None,
        }
    }
}
impl Render for Text {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.weak_entity();
        let revision = self.revision;
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
            .child(self.text.clone())
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
