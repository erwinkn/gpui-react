//! Run each mode/scene in a separate process. Never measures physical presentation.
mod allocation;
use allocation::{Mark, mark, reset_peak};
use gpui::{prelude::*, *};
use gpui_react::{Host, ReactChildren, ReactView, Registry, protocol::Transaction};
use gpui_react_controls::{
    Color, Container, ContainerProps, Document, DocumentProps, Length, ListProps, Style, Text,
    TextProps, VirtualList,
};
use gpuix_native::{GpuixView, apply_batch_to_tree, retained_tree::RetainedTree};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{Arc, Mutex},
    time::Instant,
};

const WIDTH: f32 = 800.;
const HEIGHT: f32 = 600.;
const HEADER: f32 = 32.;
const ROW: f32 = 20.;

struct Root {
    child: Option<AnyView>,
    draws: Rc<Cell<u64>>,
}
impl Render for Root {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        div()
            .size_full()
            .bg(rgb(0x101010))
            .children(self.child.clone())
    }
}
struct Raw {
    header: SharedString,
    rows: Vec<SharedString>,
    state: ListState,
    scroll: ScrollHandle,
    virtualized: bool,
}
fn raw_text(text: SharedString, height: f32) -> Div {
    div().w(px(WIDTH)).h(px(height)).flex_shrink_0().child(text)
}
impl Render for Raw {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = if self.virtualized {
            list(
                self.state.clone(),
                cx.processor(|this, ix: usize, _, _| {
                    raw_text(this.rows[ix].clone(), ROW).into_any_element()
                }),
            )
            .with_sizing_behavior(ListSizingBehavior::Auto)
            .w(px(WIDTH))
            .h(px(HEIGHT - HEADER))
            .into_any_element()
        } else {
            div()
                .id("raw-scroll")
                .w(px(WIDTH))
                .h(px(HEIGHT - HEADER))
                .flex()
                .flex_col()
                .overflow_y_scroll()
                .restrict_scroll_to_axis()
                .track_scroll(&self.scroll)
                .children(self.rows.iter().cloned().map(|text| raw_text(text, ROW)))
                .into_any_element()
        };
        div()
            .w(px(WIDTH))
            .h(px(HEIGHT))
            .flex()
            .flex_col()
            .text_size(px(14.))
            .line_height(px(ROW))
            .text_color(rgb(0xffffff))
            .child(raw_text(self.header.clone(), HEADER))
            .child(content)
    }
}
fn box_style(height: f32) -> Style {
    Style {
        width: Some(Length::Pixels(WIDTH)),
        height: Some(Length::Pixels(height)),
        shrink: Some(0.),
        ..Default::default()
    }
}
fn root_style() -> Style {
    Style {
        color: Some(Color(rgb(0xffffff).into())),
        background: Some(Color(rgb(0x101010).into())),
        font_size: Some(14.),
        line_height: Some(ROW),
        ..box_style(HEIGHT)
    }
}
fn text_props(text: String, height: f32) -> TextProps {
    TextProps {
        text,
        style: box_style(height),
        ..Default::default()
    }
}
fn row_text(ix: usize) -> String {
    format!("Row {ix:05}: retained native content for the frame comparison")
}
fn bridge_text(text: String, height: f32) -> Value {
    json!({"text":text,"style":{"width":WIDTH,"height":height,"shrink":0}})
}
fn encoded(mode: &str, count: usize, virtualized: bool) -> String {
    if mode == "bridge" {
        let mut ops = vec![
            json!({"op":"create","id":1,"component":"document","props":{"style":{"width":WIDTH,"height":HEIGHT,"fontSize":14,"lineHeight":ROW,"color":"white","background":"#101010"}}}),
            json!({"op":"place","child":1,"parent":null,"before":null}),
            json!({"op":"create","id":2,"component":"text","props":bridge_text("Status 0".into(),HEADER)}),
            json!({"op":"place","child":2,"parent":1,"before":null}),
            json!({"op":"create","id":3,"component":if virtualized {"list"} else {"container"},"props":if virtualized {json!({"estimatedItemHeight":ROW,"style":{"width":WIDTH,"height":HEIGHT-HEADER,"shrink":0}})} else {json!({"scroll":"y","style":{"width":WIDTH,"height":HEIGHT-HEADER,"shrink":0}})}}),
            json!({"op":"place","child":3,"parent":1,"before":null}),
        ];
        for ix in 0..count {
            ops.push(json!({"op":"create","id":ix+4,"component":"text","props":bridge_text(row_text(ix),ROW)}));
            ops.push(json!({"op":"place","child":ix+4,"parent":3,"before":null}));
        }
        json!({"version":1,"sequence":1,"operations":ops}).to_string()
    } else if mode == "legacy" {
        let mut ops = vec![
            json!(["createElement", 1, "div"]),
            json!(["setRoot", 1]),
            json!(["setStyle",1,{"display":"flex","flexDirection":"column","width":WIDTH,"height":HEIGHT,"fontSize":14,"lineHeight":ROW,"color":"white","backgroundColor":"#101010"}]),
            json!(["createElement", 2, "text"]),
            json!(["setText", 2, "Status 0"]),
            json!(["setStyle",2,{"width":WIDTH,"height":HEADER,"flexShrink":0}]),
            json!(["appendChild", 1, 2]),
            json!([
                "createElement",
                3,
                if virtualized { "virtual-list" } else { "div" }
            ]),
            json!(["setStyle",3,{"display":"flex","flexDirection":"column","width":WIDTH,"height":HEIGHT-HEADER,"flexShrink":0,"overflowY":"scroll"}]),
            json!(["appendChild", 1, 3]),
        ];
        if virtualized {
            ops.push(json!(["setCustomProp", 3, "estimatedItemHeight", ROW]));
        }
        for ix in 0..count {
            ops.push(json!(["createElement", ix + 4, "text"]));
            ops.push(json!(["setText", ix + 4, row_text(ix)]));
            ops.push(json!(["setStyle",ix+4,{"width":WIDTH,"height":ROW,"flexShrink":0}]));
            ops.push(json!(["appendChild", 3, ix + 4]));
        }
        Value::Array(ops).to_string()
    } else {
        String::new()
    }
}
enum Engine {
    Raw(Entity<Raw>),
    Controls {
        document: Entity<Document>,
        header: Entity<Text>,
    },
    Bridge {
        host: Entity<Host>,
        sequence: u64,
    },
    Legacy {
        view: Entity<GpuixView>,
        tree: Arc<Mutex<RetainedTree>>,
    },
}
impl Engine {
    fn mount(
        mode: &str,
        count: usize,
        virtualized: bool,
        source: &str,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        match mode {
            "raw" => Self::Raw(cx.new(|_| {
                Raw {
                    header: "Status 0".into(),
                    rows: (0..count).map(|ix| row_text(ix).into()).collect(),
                    state: ListState::new(count, ListAlignment::Top, px(0.))
                        .with_uniform_item_height(px(ROW)),
                    scroll: ScrollHandle::new(),
                    virtualized,
                }
            })),
            "controls" => {
                let header = cx.new(|_| Text::new(text_props("Status 0".into(), HEADER)));
                let rows: Vec<AnyView> = (0..count)
                    .map(|ix| cx.new(|_| Text::new(text_props(row_text(ix), ROW))).into())
                    .collect();
                let pane: AnyView = if virtualized {
                    let pane = cx.new(|cx| {
                        VirtualList::new(
                            ListProps {
                                style: box_style(HEIGHT - HEADER),
                                estimated_item_height: Some(ROW),
                                ..Default::default()
                            },
                            window,
                            cx,
                        )
                    });
                    pane.update(cx, |pane, cx| pane.set_children(rows, window, cx));
                    pane.into()
                } else {
                    let pane = cx.new(|cx| {
                        Container::new(
                            ContainerProps {
                                style: box_style(HEIGHT - HEADER),
                                scroll: gpui_react_controls::container::Scroll::Y,
                                ..Default::default()
                            },
                            window,
                            cx,
                        )
                    });
                    pane.update(cx, |pane, cx| pane.set_children(rows, window, cx));
                    pane.into()
                };
                let document = cx.new(|cx| {
                    Document::new(
                        DocumentProps {
                            style: root_style(),
                            ..Default::default()
                        },
                        cx,
                    )
                });
                document.update(cx, |doc, cx| {
                    doc.set_children(vec![header.clone().into(), pane], window, cx)
                });
                Self::Controls { document, header }
            }
            "bridge" => {
                let mut registry = Registry::default();
                gpui_react_controls::register(&mut registry).unwrap();
                let host = cx.new(|_| Host::new(registry, Arc::new(|_| {})));
                let tx = serde_json::from_str(source).unwrap();
                host.update(cx, |host, cx| host.apply(tx, window, cx))
                    .unwrap();
                Self::Bridge { host, sequence: 1 }
            }
            "legacy" => {
                let tree = Arc::new(Mutex::new(RetainedTree::new()));
                apply_batch_to_tree(&mut tree.lock().unwrap(), source.as_bytes()).unwrap();
                Self::Legacy {
                    view: cx.new(|_| GpuixView::for_benchmark(tree.clone())),
                    tree,
                }
            }
            _ => panic!("mode must be raw, controls, bridge, or legacy"),
        }
    }
    fn view(&self) -> AnyView {
        match self {
            Self::Raw(v) => v.clone().into(),
            Self::Controls { document, .. } => document.clone().into(),
            Self::Bridge { host, .. } => host.clone().into(),
            Self::Legacy { view, .. } => view.clone().into(),
        }
    }
    fn update_wire(&mut self, text: &str) -> String {
        match self {
            Self::Bridge { sequence, .. } => {
                *sequence += 1;
                json!({"version":1,"sequence":sequence,"operations":[{"op":"props","id":2,"props":bridge_text(text.into(),HEADER)}]}).to_string()
            }
            Self::Legacy { .. } => json!([["setText", 2, text]]).to_string(),
            _ => text.to_owned(),
        }
    }
    fn update(&mut self, source: &str, window: &mut Window, cx: &mut App) {
        match self {
            Self::Raw(view) => view.update(cx, |view, cx| {
                view.header = source.to_owned().into();
                cx.notify();
            }),
            Self::Controls { header, .. } => header.update(cx, |header, cx| {
                header.set_props(text_props(source.into(), HEADER), window, cx)
            }),
            Self::Bridge { host, .. } => {
                // Decode occurs on the worker in production; this reports combined native CPU work.
                let tx: Transaction = serde_json::from_str(source).unwrap();
                host.update(cx, |host, cx| host.apply(tx, window, cx))
                    .unwrap();
            }
            Self::Legacy { view, tree } => {
                apply_batch_to_tree(&mut tree.lock().unwrap(), source.as_bytes()).unwrap();
                view.update(cx, |_, cx| cx.notify());
            }
        }
    }
    fn removal_wire(&mut self) -> String {
        match self {
            Self::Bridge { sequence, .. } => {
                *sequence += 1;
                json!({"version":1,"sequence":sequence,"operations":[{"op":"remove","id":1}]})
                    .to_string()
            }
            Self::Legacy { .. } => "[[\"destroyElement\",1]]".into(),
            _ => String::new(),
        }
    }
    fn remove(&mut self, source: &str, window: &mut Window, cx: &mut App) {
        match self {
            Self::Bridge { host, .. } => {
                let transaction = serde_json::from_str(source).unwrap();
                host.update(cx, |host, cx| {
                    host.apply(transaction, window, cx).unwrap();
                    assert!(host.is_empty());
                });
            }
            Self::Legacy { tree, .. } => {
                let mut tree = tree.lock().unwrap();
                apply_batch_to_tree(&mut tree, source.as_bytes()).unwrap();
                assert!(tree.elements.is_empty());
            }
            _ => {}
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Sample {
    micros: f64,
    heap: Mark,
}
fn measure<T>(operation: impl FnOnce() -> T) -> (T, Sample) {
    reset_peak();
    let before = mark();
    let start = Instant::now();
    let result = operation();
    let micros = start.elapsed().as_secs_f64() * 1e6;
    let heap = mark().since(before);
    (result, Sample { micros, heap })
}
fn summary(samples: &[Sample]) -> Value {
    let mut times: Vec<_> = samples.iter().map(|s| s.micros).collect();
    times.sort_by(f64::total_cmp);
    let n = times.len();
    json!({"samples":n,"p50Us":times[n/2],"p95Us":times[(n*95/100).min(n-1)],"maxUs":times[n-1],"meanAllocatedBytes":samples.iter().map(|s|s.heap.allocated).sum::<u64>()/n as u64,"meanAllocations":samples.iter().map(|s|s.heap.calls).sum::<u64>()/n as u64,"maxAdditionalLiveBytes":samples.iter().map(|s|s.heap.peak).max()})
}
struct NativeWindow {
    window: WindowHandle<Root>,
    draws: Rc<Cell<u64>>,
    app: ApplicationHandle,
    platform: Rc<gpui_macos::MacPlatform>,
}
impl NativeWindow {
    fn new() -> Self {
        let platform = Rc::new(gpui_macos::MacPlatform::new_embedded());
        let opened = Rc::new(RefCell::new(None));
        let slot = opened.clone();
        let draws = Rc::new(Cell::new(0));
        let counter = draws.clone();
        let app = Application::with_platform(platform.clone()).run_embedded(move |cx| {
            *slot.borrow_mut() = Some(
                cx.open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                            point(px(-10000.), px(-10000.)),
                            size(px(WIDTH), px(HEIGHT)),
                        ))),
                        show: false,
                        focus: false,
                        ..Default::default()
                    },
                    |_, cx| {
                        cx.new(|_| Root {
                            child: None,
                            draws: counter,
                        })
                    },
                )
                .unwrap(),
            );
        });
        let window = opened.borrow_mut().take().unwrap();
        Self {
            window,
            draws,
            app,
            platform,
        }
    }
    fn update<R>(&self, f: impl FnOnce(&mut Root, &mut Window, &mut Context<Root>) -> R) -> R {
        self.app.update(|cx| self.window.update(cx, f).unwrap())
    }
    fn with_window<R>(&self, f: impl FnOnce(&mut Window, &mut App) -> R) -> R {
        self.app.update(|cx| {
            cx.update_window(self.window.into(), |_, window, cx| f(window, cx))
                .unwrap()
        })
    }
    fn draw(&self) {
        let before = self.draws.get();
        self.with_window(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(
            self.draws.get(),
            before + 1,
            "one explicit draw must render exactly once"
        );
    }
}
impl Drop for NativeWindow {
    fn drop(&mut self) {
        self.with_window(|window, _| window.remove_window());
        self.app.update(|cx| cx.quit());
        self.platform.run_event_loop();
    }
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let mode = args.get(1).expect("mode");
    let count: usize = args.get(3).expect("count").parse().unwrap();
    let virtualized = args.get(2).expect("flow or list") == "list";
    let source = encoded(mode, count, virtualized);
    let scene = NativeWindow::new();
    scene.draw();
    let mut apply = Vec::with_capacity(100);
    let mut frames = Vec::with_capacity(100);
    let mut scroll = Vec::with_capacity(100);
    let baseline = mark();
    let draws_before_mount = scene.draws.get();
    let (mut engine, mount) = measure(|| {
        scene.update(|root, window, cx| {
            let engine = Engine::mount(mode, count, virtualized, &source, window, cx);
            root.child = Some(engine.view());
            cx.notify();
            engine
        })
    });
    assert_eq!(scene.draws.get(), draws_before_mount, "mount must not draw");
    let after_mount = mark();
    let (_, first_draw) = measure(|| scene.draw());
    let after_first_draw = mark();
    #[cfg(feature = "scene-checks")]
    if let Some(dir) = std::env::var_os("FRAME_BENCH_IMAGES").filter(|path| !path.is_empty()) {
        std::fs::create_dir_all(&dir).unwrap();
        let image = scene
            .with_window(|window, _| window.render_to_image())
            .unwrap();
        assert!(
            image
                .pixels()
                .filter(|p| p[0] > 200 && p[1] > 200 && p[2] > 200)
                .count()
                > 1000,
            "the row scene must contain painted text"
        );
        image
            .save(std::path::Path::new(&dir).join(format!(
                "{mode}-{}-{count}.png",
                if virtualized { "list" } else { "flow" }
            )))
            .unwrap();
    }
    for i in 0..110 {
        let text = if i % 2 == 0 { "Status 1" } else { "Status 0" };
        let update = engine.update_wire(text);
        let draws_before_update = scene.draws.get();
        let (_, sample) =
            measure(|| scene.update(|_, window, cx| engine.update(&update, window, cx)));
        assert_eq!(
            scene.draws.get(),
            draws_before_update,
            "update must not draw"
        );
        let (_, frame) = measure(|| scene.draw());
        if i >= 10 {
            apply.push(sample);
            frames.push(frame);
        }
    }
    for i in 0..110 {
        let (_, sample) = measure(|| {
            scene.with_window(|window, cx| {
                let result = window.dispatch_event(
                    ScrollWheelEvent {
                        position: point(px(100.), px(100.)),
                        delta: ScrollDelta::Pixels(point(
                            px(0.),
                            px(if i % 2 == 0 { -20. } else { 20. }),
                        )),
                        ..Default::default()
                    }
                    .to_platform_input(),
                    cx,
                );
                assert!(
                    result.default_prevented,
                    "wheel must move a native scroller"
                );
            });
            scene.draw();
        });
        if i >= 10 {
            scroll.push(sample);
        }
    }
    let after_updates = mark();
    let remove = engine.removal_wire();
    let (_, clear) = measure(|| {
        scene.update(|root, window, cx| {
            engine.remove(&remove, window, cx);
            root.child = None;
            cx.notify();
        });
        drop(engine);
        scene.draw();
    });
    let after_clear = mark();
    let viewport=scene.with_window(|window,_|json!({"width":f32::from(window.viewport_size().width),"height":f32::from(window.viewport_size().height),"scale":window.scale_factor()}));
    println!(
        "{}",
        json!({"mode":mode,"scene":if virtualized{"list"}else{"flow"},"rows":count,"allocationCounts":cfg!(feature="allocation-counts"),"wireBytes":source.len(),"viewport":viewport,"mount":mount,"firstDraw":first_draw,"nativeUpdate":summary(&apply),"updatedDraw":summary(&frames),"wheelAndDraw":summary(&scroll),"clearAndDraw":clear,"rustLiveBytesAboveEmpty":{"afterMount":after_mount.live-baseline.live,"afterFirstDraw":after_first_draw.live-baseline.live,"afterUpdates":after_updates.live-baseline.live,"afterClear":after_clear.live-baseline.live}})
    );
}
