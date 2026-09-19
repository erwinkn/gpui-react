//! Run each mode/scene in a separate process. Never measures physical presentation.
mod allocation;
use allocation::{Mark, mark, reset_peak};
use gpui::{prelude::*, *};
use gpui_react::{Host, Registry, protocol::Transaction};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
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
fn row_text(ix: usize) -> String {
    format!("Row {ix:05}: retained native content for the frame comparison")
}
/// Style ids as the reconciler would assign them: 0 root, 1 header, 2 rows.
fn bridge_text(text: String, style: u32) -> Value {
    json!({"text":text,"style":style})
}
/// The mount transaction. With `FRAME_BENCH_WIRE_DIR` set, both bridge modes
/// read what the JavaScript bridge sealed for this scene (see
/// `fixtures/bridge-counter/js-wire-dump.tsx`); otherwise the JSON is built here.
fn encoded(mode: &str, count: usize, virtualized: bool) -> Vec<u8> {
    if let Ok(dir) = std::env::var("FRAME_BENCH_WIRE_DIR") {
        if mode == "bridge" || mode == "binary" {
            let scene = if virtualized { "list" } else { "flow" };
            let extension = if mode == "binary" { "bin" } else { "json" };
            let path = format!("{dir}/mount-{scene}-{count}.{extension}");
            return std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        }
    } else if mode == "binary" {
        panic!("binary mode needs FRAME_BENCH_WIRE_DIR");
    }
    if mode == "bridge" {
        let mut ops = vec![
            json!({"op":"style","id":0,"style":{"width":WIDTH,"height":HEIGHT,"fontSize":14,"lineHeight":ROW,"color":"white","background":"#101010"}}),
            json!({"op":"style","id":1,"style":{"width":WIDTH,"height":HEADER,"shrink":0}}),
            json!({"op":"style","id":2,"style":{"width":WIDTH,"height":ROW,"shrink":0}}),
            json!({"op":"style","id":3,"style":{"width":WIDTH,"height":HEIGHT-HEADER,"shrink":0}}),
            // Ids follow the JavaScript bridge: the root is 0, the header 1, the list 2.
            json!({"op":"create","id":0,"component":std::env::var("BRIDGE_ROOT").unwrap_or_else(|_| "document".into()),"props":{"style":0},"parent":null}),
            json!({"op":"create","id":1,"component":"text","props":bridge_text("Status 0".into(),1),"parent":0}),
            json!({"op":"create","id":2,"component":if virtualized {"list"} else {"container"},"props":if virtualized {json!({"estimatedItemHeight":ROW,"style":3})} else {json!({"scroll":"y","style":3})},"parent":0}),
        ];
        for ix in 0..count {
            ops.push(json!({"op":"create","id":ix+3,"component":"text","props":bridge_text(row_text(ix),2),"parent":2}));
        }
        json!({"version":1,"sequence":1,"operations":ops}).to_string().into_bytes()
    } else {
        Vec::new()
    }
}
enum Engine {
    Raw(Entity<Raw>),
    Bridge {
        host: Entity<Host>,
        sequence: u64,
    },
}
impl Engine {
    fn mount(
        mode: &str,
        count: usize,
        virtualized: bool,
        source: &[u8],
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
            "bridge" | "binary" => {
                let mut registry = Registry::default();
                gpui_react::register_builtins(&mut registry).unwrap();
                let host = cx.new(|_| Host::new(registry, Arc::new(|_| {})));
                let binary = mode == "binary";
                let started = Instant::now();
                let floor = if std::env::var_os("BRIDGE_MOUNT_PHASES").is_some() && !binary {
                    let _: serde::de::IgnoredAny = serde_json::from_slice(source).unwrap();
                    Some(started.elapsed())
                } else {
                    None
                };
                let started = Instant::now();
                let prepared = host
                    .update(cx, |host, _| {
                        if binary {
                            host.decode_binary(source)
                        } else {
                            host.decode(std::str::from_utf8(source).unwrap())
                        }
                    })
                    .unwrap();
                let decoded = started.elapsed();
                host.update(cx, |host, cx| host.apply_prepared(prepared, window, cx))
                    .unwrap();
                let applied = started.elapsed();
                if let Some(floor) = floor {
                    eprintln!(
                        "mount phases: json tokenize floor {:.0} us, decode {:.0} us, apply {:.0} us",
                        floor.as_secs_f64() * 1e6,
                        decoded.as_secs_f64() * 1e6,
                        (applied - decoded).as_secs_f64() * 1e6
                    );
                }
                Self::Bridge { host, sequence: 1 }
            }
            _ => panic!("mode must be raw, bridge, or binary"),
        }
    }
    fn view(&self) -> AnyView {
        match self {
            Self::Raw(v) => v.clone().into(),
            Self::Bridge { host, .. } => host.clone().into(),
        }
    }
    fn update_wire(&mut self, text: &str) -> String {
        if let Self::Bridge { sequence, .. } = self {
            *sequence += 1;
            if std::env::var_os("BRIDGE_SKIP_UPDATE").is_some() {
                // Probe: an empty transaction leaves the tree clean, so the
                // following draw re-renders nothing.
                return json!({"version":1,"sequence":sequence,"operations":[]}).to_string();
            }
            json!({"version":1,"sequence":sequence,"operations":[{"op":"props","id":1,"component":"text","props":bridge_text(text.into(),1)}]}).to_string()
        } else {
            text.to_owned()
        }
    }
    fn update(&mut self, source: &str, window: &mut Window, cx: &mut App) {
        match self {
            Self::Raw(view) => view.update(cx, |view, cx| {
                view.header = source.to_owned().into();
                cx.notify();
            }),
            Self::Bridge { host, .. } => {
                // Decode occurs on the worker in production; this reports combined native CPU work.
                let tx = Transaction::parse(source).unwrap();
                host.update(cx, |host, cx| host.apply(tx, window, cx))
                    .unwrap();
            }
        }
    }
    fn removal_wire(&mut self) -> String {
        if let Self::Bridge { sequence, .. } = self {
            *sequence += 1;
            json!({"version":1,"sequence":sequence,"operations":[{"op":"remove","id":0}]})
                .to_string()
        } else {
            String::new()
        }
    }
    fn remove(&mut self, source: &str, window: &mut Window, cx: &mut App) {
        if let Self::Bridge { host, .. } = self {
            let transaction = Transaction::parse(source).unwrap();
            let started = Instant::now();
            host.update(cx, |host, cx| {
                host.apply(transaction, window, cx).unwrap();
                assert!(host.is_empty());
            });
            if std::env::var_os("BRIDGE_MOUNT_PHASES").is_some() {
                eprintln!("remove transaction: {:.0} us", started.elapsed().as_secs_f64() * 1e6);
            }
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
    if mode == "schema" {
        // The controls' kind table as JSON, for worker-side tools that encode
        // the binary wire without a native session.
        let mut registry = Registry::default();
        gpui_react::register_builtins(&mut registry).unwrap();
        println!("{}", serde_json::to_string(&registry.schema()).unwrap());
        return;
    }
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
    if std::env::var_os("PROBE_MOUNT_HISTOGRAM").is_some() {
        let empty = vec![0i64; allocation::histogram().len().max(1)];
        eprintln!("live allocations after mount by size: {:?}", allocation::growth(&empty, &allocation::histogram()));
    }
    let (_, first_draw) = measure(|| scene.draw());
    let after_first_draw = mark();
    let histogram_after_first_draw = allocation::histogram();
    if let Some(size) = std::env::var("PROBE_TRACK_SIZE").ok().and_then(|s| s.parse().ok()) {
        allocation::track_size(size);
    }
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
    let mut live_after_draw = Vec::new();
    for i in 0..110 {
        allocation::PHASE.store(i + 1, std::sync::atomic::Ordering::Relaxed);
        if i < 6 {
            live_after_draw.push(mark().live - baseline.live);
        }
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
    let after_text_updates = mark();
    let growth = allocation::growth(&histogram_after_first_draw, &allocation::histogram());
    for (count, trace) in allocation::tracked_live().into_iter().take(1) {
        eprintln!("=== {count} live tracked allocations from:\n{trace}\n");
    }
    for trace in allocation::late_frees() {
        eprintln!("=== late free:\n{trace}\n");
    }
    allocation::track_size(0);
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
    let histogram_after_updates = allocation::histogram();
    let remove = engine.removal_wire();
    let phases = std::env::var_os("BRIDGE_MOUNT_PHASES").is_some();
    let (_, clear) = measure(|| {
        let t = Instant::now();
        scene.update(|root, window, cx| {
            engine.remove(&remove, window, cx);
            root.child = None;
            cx.notify();
        });
        let removed = t.elapsed();
        if std::env::var_os("PROBE_LEAK_ENGINE").is_some() {
            std::mem::forget(engine);
        } else {
            drop(engine);
        }
        let dropped = t.elapsed();
        scene.draw();
        if phases {
            eprintln!(
                "clear phases: remove {:.0} us, drop engine {:.0} us, empty draw {:.0} us",
                removed.as_secs_f64() * 1e6,
                (dropped - removed).as_secs_f64() * 1e6,
                (t.elapsed() - dropped).as_secs_f64() * 1e6
            );
        }
    });
    let after_clear = mark();
    if std::env::var_os("PROBE_CLEAR_HISTOGRAM").is_some() {
        eprintln!(
            "freed by clear, by size: {:?}",
            allocation::growth(&histogram_after_updates, &allocation::histogram())
        );
    }
    let viewport=scene.with_window(|window,_|json!({"width":f32::from(window.viewport_size().width),"height":f32::from(window.viewport_size().height),"scale":window.scale_factor()}));
    println!(
        "{}",
        json!({"mode":mode,"scene":if virtualized{"list"}else{"flow"},"rows":count,"allocationCounts":cfg!(feature="allocation-counts"),"wireBytes":source.len(),"viewport":viewport,"mount":mount,"firstDraw":first_draw,"nativeUpdate":summary(&apply),"updatedDraw":summary(&frames),"wheelAndDraw":summary(&scroll),"clearAndDraw":clear,"liveGrowthBySize":growth,"liveBeforeUpdateDraws":live_after_draw,"rustLiveBytesAboveEmpty":{"afterMount":after_mount.live-baseline.live,"afterFirstDraw":after_first_draw.live-baseline.live,"afterTextUpdates":after_text_updates.live-baseline.live,"afterUpdates":after_updates.live-baseline.live,"afterClear":after_clear.live-baseline.live}})
    );
}
