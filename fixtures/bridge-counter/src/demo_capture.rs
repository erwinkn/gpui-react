//! Optional example image capture. This is not part of the production host.
use gpui::{prelude::*, *};
use gpui_react_host::gpui_react::*;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Props {}
struct Capture {
    children: Vec<AnyView>,
    path: Option<std::path::PathBuf>,
}
impl Render for Capture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let path = self.path.take();
        div()
            .size_full()
            .children(self.children.clone())
            .on_painted(move |_, window, _| {
                if let Some(path) = path.clone() {
                    window.on_draw_complete(move |window, _| {
                        window
                            .render_to_image()
                            .expect("demo GPU capture")
                            .save(&path)
                            .expect("save demo image");
                    });
                }
            })
    }
}
impl ReactView for Capture {
    type Props = Props;
    fn create(_: Props, _: &mut Window, _: &mut Context<Self>) -> Self {
        Self {
            children: vec![],
            path: std::env::var_os("BRIDGE_DEMO_IMAGE").map(Into::into),
        }
    }
    fn set_props(&mut self, _: Props, _: &mut Window, _: &mut Context<Self>) {}
}
impl ReactChildren for Capture {
    fn set_children(&mut self, children: Vec<AnyView>, _: &mut Window, cx: &mut Context<Self>) {
        self.children = children;
        cx.notify();
    }
}
pub fn register(registry: &mut Registry) -> anyhow::Result<()> {
    registry.register(Component::<Capture>::new("demo-capture").children())
}
