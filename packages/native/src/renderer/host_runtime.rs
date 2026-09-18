//! Native main-thread host and a bounded channel for an application worker.
//! No JS values, callbacks, or runtime locks cross this channel.
use super::*;
use futures::{StreamExt, channel::mpsc};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::io::{IsTerminal, Read, Write};
use std::sync::{
    Condvar, OnceLock, Weak,
    atomic::{AtomicBool, AtomicU32, Ordering},
};

const MAX_COMMANDS: usize = 256;
const MAX_BYTES: usize = 4 * 1024 * 1024;
const MAX_EVENTS: usize = 4096;
static NEXT_SESSION: AtomicU32 = AtomicU32::new(1);
static SESSIONS: OnceLock<Mutex<HashMap<u32, Weak<Session>>>> = OnceLock::new();
static STDIO_SESSION: OnceLock<Mutex<Weak<Session>>> = OnceLock::new();
static STDIO_READER: OnceLock<std::result::Result<(), String>> = OnceLock::new();

#[allow(unexpected_cfgs)]
fn is_main_thread() -> bool {
    use objc::{class, msg_send, sel, sel_impl};
    let main: objc::runtime::BOOL = unsafe { msg_send![class!(NSThread), isMainThread] };
    main == objc::runtime::YES
}

#[derive(Default)]
struct Queue {
    bytes: usize,
    messages: VecDeque<String>,
    last_id: u32,
}

#[derive(Default)]
struct Snapshot {
    bounds: HashMap<u64, crate::automation::ElementBounds>,
    values: HashMap<String, Value>,
    scrolls: HashMap<u64, [f64; 2]>,
    lists: HashMap<u64, [f64; 3]>,
}

impl Snapshot {
    fn capture(view: &GpuixView, window: &gpui::Window) -> Self {
        let size = window.viewport_size();
        let mut values = HashMap::new();
        values.insert(
            "getWindowSize".into(),
            json!({"width": f32::from(size.width), "height": f32::from(size.height)}),
        );
        let insets = window.insets();
        let edges = |edge: gpui::Edges<gpui::Pixels>| json!({"top": f32::from(edge.top), "right": f32::from(edge.right), "bottom": f32::from(edge.bottom), "left": f32::from(edge.left)});
        values.insert("getWindowInsets".into(), json!({"safeArea": edges(insets.safe_area), "ime": edges(insets.ime), "effective": edges(insets.effective())}));
        values.insert(
            "getFocusedElementId".into(),
            json!(view.focused_element_id(window)),
        );
        values.insert(
            "getSelectedText".into(),
            json!(view.selection.lock().selected_text()),
        );
        values.insert(
            "getSelectionInfo".into(),
            json!(crate::text::paint::selection_info(&view.selection)),
        );
        let mut scrolls: HashMap<_, _> = view
            .scroll_handles
            .iter()
            .map(|(&id, handle)| {
                let offset = handle.offset();
                (
                    id,
                    [
                        f64::from(f32::from(offset.x)),
                        f64::from(f32::from(offset.y)),
                    ],
                )
            })
            .collect();
        for (&id, entry) in &view.virtual_lists {
            let offset = entry.state.scroll_px_offset_for_scrollbar();
            scrolls.insert(
                id,
                [
                    f64::from(f32::from(offset.x)),
                    f64::from(f32::from(offset.y)),
                ],
            );
        }
        let lists = view
            .virtual_lists
            .iter()
            .map(|(&id, entry)| {
                let top = entry.state.logical_scroll_top();
                (
                    id,
                    [
                        top.item_ix as f64,
                        f64::from(f32::from(top.offset_in_item)),
                        f64::from(f32::from(entry.state.viewport_bounds().size.height)),
                    ],
                )
            })
            .collect();
        Self {
            bounds: crate::automation::all_bounds(),
            values,
            scrolls,
            lists,
        }
    }
}

pub(super) struct Session {
    commands: Mutex<Queue>,
    events: Mutex<Queue>,
    wake: Mutex<mpsc::Sender<()>>,
    attached: AtomicBool,
    closed: AtomicBool,
    reason: Mutex<String>,
    changed: Condvar,
    receiving: AtomicBool,
    text_system: Arc<gpui::TextSystem>,
    snapshot: Mutex<Arc<Snapshot>>,
}

impl Session {
    pub(super) fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
    fn wake(&self) {
        let _ = self.wake.lock().unwrap().try_send(());
    }
    pub(super) fn close(&self, reason: &str) {
        let mut stored_reason = self.reason.lock().unwrap();
        if !self.closed.load(Ordering::Acquire) {
            *stored_reason = reason.to_owned();
            self.closed.store(true, Ordering::Release);
            drop(stored_reason);
            self.changed.notify_all();
            self.wake();
        }
    }
    fn emit(&self, value: Value) {
        if self.closed.load(Ordering::Acquire) {
            return;
        }
        let message = value.to_string();
        let mut queue = self.events.lock().unwrap();
        if queue.messages.len() == MAX_EVENTS || queue.bytes + message.len() > MAX_BYTES {
            drop(queue);
            self.close("Application event queue overflow");
            return;
        }
        queue.bytes += message.len();
        queue.messages.push_back(message);
        self.changed.notify_one();
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    id: u32,
    method: String,
    #[serde(default)]
    args: Vec<Value>,
}

#[napi]
pub struct NativeHost {
    id: u32,
    session: Arc<Session>,
    renderer: Rc<GpuixRenderer>,
    receiver: RefCell<Option<mpsc::Receiver<()>>>,
    show: bool,
    focus: bool,
}

#[napi]
impl NativeHost {
    #[napi(constructor)]
    pub fn new(options: Option<WindowOptions>) -> Result<Self> {
        if !is_main_thread() {
            return Err(Error::from_reason(
                "NativeHost must be created on the macOS main thread",
            ));
        }
        let (sender, receiver) = mpsc::channel(1);
        let mut renderer = GpuixRenderer::new(None);
        let mut options = options.unwrap_or_default();
        for (name, value, allow_zero) in [
            ("width", options.width, false),
            ("height", options.height, false),
            ("minWidth", options.min_width, true),
            ("minHeight", options.min_height, true),
        ] {
            if value.is_some_and(|value| {
                !value.is_finite() || value < 0.0 || (!allow_zero && value == 0.0)
            }) {
                return Err(Error::from_reason(format!("Invalid native window {name}")));
            }
        }
        let show = options.show.unwrap_or(true);
        let focus = options.focus.unwrap_or(true);
        options.show = Some(false);
        options.focus = Some(false);
        // Initialize without a JS event callback. The worker owns that runtime.
        renderer.init(Some(options))?;
        let text_system = GPUI_APP.with(|app| {
            app.borrow()
                .as_ref()
                .unwrap()
                .update(|cx| cx.text_system().clone())
        });
        let session = Arc::new(Session {
            commands: Mutex::new(Queue::default()),
            events: Mutex::new(Queue::default()),
            wake: Mutex::new(sender),
            attached: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            reason: Mutex::new(String::new()),
            changed: Condvar::new(),
            receiving: AtomicBool::new(false),
            text_system,
            snapshot: Mutex::new(Arc::new(Snapshot::default())),
        });
        let event_session = session.clone();
        renderer.native_event_callback = Some(Arc::new(move |event| {
            #[cfg(feature = "test-support")]
            crate::host_probe::record("input", json!({"type": event.event_type, "y": event.y}));
            event_session.emit(json!({"event": event}));
        }));
        update_window(|view, window, cx| {
            view.event_callback = renderer.native_event_callback.clone();
            *session.snapshot.lock().unwrap() = Arc::new(Snapshot::capture(view, window));
            let weak = cx.weak_entity();
            let session = session.clone();
            view.host_after_paint = Some(Rc::new(move |window, cx| {
                if let Ok(snapshot) = weak.read_with(cx, |view, _| Snapshot::capture(view, window))
                {
                    *session.snapshot.lock().unwrap() = Arc::new(snapshot);
                }
            }));
        })?;
        let id = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
        SESSIONS
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .insert(id, Arc::downgrade(&session));
        Ok(Self {
            id,
            session,
            renderer: Rc::new(renderer),
            receiver: RefCell::new(Some(receiver)),
            show,
            focus,
        })
    }

    #[napi(getter)]
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Blocks the main JS launcher until shutdown. Application JS runs in the worker.
    #[napi]
    pub fn run(&self) -> Result<String> {
        let _signals = super::host_signals::HostSignals::new(&self.session)?;
        let mut receiver = self
            .receiver
            .borrow_mut()
            .take()
            .ok_or_else(|| Error::from_reason("NativeHost can run only once"))?;
        let renderer = self.renderer.clone();
        let session = self.session.clone();
        let (mut first_commit, show, focus) = (true, self.show, self.focus);
        let command_task = GPUI_APP.with(|app| {
            app.borrow().as_ref().unwrap().update(|cx| {
                cx.spawn(async move |cx| {
                    loop {
                        if session.closed.load(Ordering::Acquire) {
                            cx.update(|cx| cx.quit());
                            break;
                        }
                        let started = std::time::Instant::now();
                        for _ in 0..32 {
                            let message = {
                                let mut queue = session.commands.lock().unwrap();
                                queue
                                    .messages
                                    .pop_front()
                                    .inspect(|message| queue.bytes -= message.len())
                            };
                            let Some(message) = message else {
                                break;
                            };
                            let request: Request = serde_json::from_str(&message).unwrap();
                            let commands = if request.method == "transaction" {
                                serde_json::from_value::<Vec<Request>>(request.args[0].clone())
                                    .unwrap()
                            } else {
                                vec![request]
                            };
                            // A transaction includes React's layout-effect commands. Never
                            // yield between its mutations and its focus or scroll intent.
                            for request in commands {
                                let result = execute(&renderer, &request);
                                session.emit(match result {
                                    Ok(value) => json!({"id": request.id, "value": value}),
                                    Err(error) => {
                                        json!({"id": request.id, "error": error.to_string()})
                                    }
                                });
                                if request.method == "shutdown" {
                                    session.close("Application requested shutdown");
                                    break;
                                }
                            }
                            if first_commit && renderer.tree.lock().unwrap().root_id.is_some() {
                                first_commit = false;
                                let result = update_window_without_view(|window, cx| {
                                    window.draw(cx).clear(cx);
                                    #[cfg(feature = "test-support")]
                                    crate::host_probe::record(
                                        "first-frame",
                                        json!({"text": crate::text::painted_text()}),
                                    );
                                    if show {
                                        window.show_inactive()?;
                                        if focus {
                                            window.activate_window();
                                            cx.activate(true);
                                        }
                                    }
                                    Ok::<_, anyhow::Error>(())
                                });
                                if let Err(error) = result.and_then(|result| {
                                    result.map_err(|error| Error::from_reason(error.to_string()))
                                }) {
                                    session.close(&format!(
                                        "Failed to show the first native frame: {error}"
                                    ));
                                }
                            }
                            if session.closed.load(Ordering::Acquire) {
                                break;
                            }
                            if started.elapsed() >= Duration::from_millis(4) {
                                break;
                            }
                        }
                        if session.closed.load(Ordering::Acquire) {
                            continue;
                        }
                        if session.commands.lock().unwrap().messages.is_empty() {
                            if receiver.next().await.is_none() {
                                break;
                            }
                        } else {
                            // Yield to AppKit and display work between bounded command slices.
                            cx.background_executor()
                                .timer(Duration::from_millis(1))
                                .await;
                        }
                    }
                })
            })
        });
        MAC_PLATFORM.with(|platform| platform.borrow().as_ref().unwrap().run_event_loop());
        self.session.close("Native window closed");
        if let Some(sessions) = SESSIONS.get() {
            sessions.lock().unwrap().remove(&self.id);
        }
        drop(command_task);
        #[cfg(feature = "test-support")]
        crate::host_probe::stop();
        GPUI_WINDOW.with(|window| window.borrow_mut().take());
        GPUI_APP.with(|app| app.borrow_mut().take());
        MAC_PLATFORM.with(|platform| platform.borrow_mut().take());
        SCROLL_HANDLES.with(|handles| handles.borrow_mut().clear());
        VIRTUAL_LIST_STATES.with(|states| states.borrow_mut().clear());
        PENDING_VIRTUAL_LIST_SCROLLS.with(|scrolls| scrolls.borrow_mut().clear());
        Ok(self.session.reason.lock().unwrap().clone())
    }

    #[napi]
    pub fn request_shutdown(&self, reason: String) {
        self.session.close(&reason);
    }
}

impl Drop for NativeHost {
    fn drop(&mut self) {
        self.session.close("Native host disposed");
        if let Some(sessions) = SESSIONS.get() {
            sessions.lock().unwrap().remove(&self.id);
        }
    }
}

#[napi]
pub struct NativeClient {
    session: Arc<Session>,
    staging: RefCell<RetainedTree>,
}

#[napi]
impl NativeClient {
    #[napi(constructor)]
    pub fn new(id: u32, env: Env) -> Result<Self> {
        if is_main_thread() {
            return Err(Error::from_reason(
                "NativeClient must be created in the application worker",
            ));
        }
        let session = SESSIONS
            .get()
            .and_then(|sessions| sessions.lock().unwrap().get(&id).and_then(Weak::upgrade))
            .ok_or_else(|| Error::from_reason("Unknown or expired native session"))?;
        if session.closed.load(Ordering::Acquire) {
            return Err(Error::from_reason("Native session has closed"));
        }
        if session.attached.swap(true, Ordering::AcqRel) {
            return Err(Error::from_reason(
                "Native session already has an application runtime",
            ));
        }
        env.add_env_cleanup_hook(session.clone(), |session| {
            session.close("Application runtime unloaded")
        })?;
        Ok(Self {
            session,
            staging: RefCell::new(RetainedTree::new()),
        })
    }

    /// False means backpressure. The caller must retain and retry the complete request.
    #[napi]
    pub fn send(&self, message: String) -> Result<bool> {
        if self.session.closed.load(Ordering::Acquire) {
            return Err(Error::from_reason(
                self.session.reason.lock().unwrap().clone(),
            ));
        }
        if message.len() > MAX_BYTES {
            return Err(Error::from_reason("Native request exceeds 4 MiB"));
        }
        let request = serde_json::from_str::<Request>(&message)
            .map_err(|e| Error::from_reason(e.to_string()))?;
        let ids = if request.method == "transaction" {
            let commands = serde_json::from_value::<Vec<Request>>(
                request
                    .args
                    .first()
                    .cloned()
                    .ok_or_else(|| Error::from_reason("Missing transaction commands"))?,
            )
            .map_err(|error| Error::from_reason(error.to_string()))?;
            if commands.is_empty()
                || commands.len() > MAX_COMMANDS
                || commands
                    .iter()
                    .any(|command| command.method == "transaction")
            {
                return Err(Error::from_reason(
                    "A transaction must contain 1 to 256 non-nested commands",
                ));
            }
            commands
                .into_iter()
                .map(|command| command.id)
                .collect::<Vec<_>>()
        } else {
            vec![request.id]
        };
        let mut queue = self.session.commands.lock().unwrap();
        let mut last_id = queue.last_id;
        for id in ids {
            if id <= last_id {
                return Err(Error::from_reason(
                    "Native request IDs must increase within a session",
                ));
            }
            last_id = id;
        }
        if queue.messages.len() == MAX_COMMANDS || queue.bytes + message.len() > MAX_BYTES {
            return Ok(false);
        }
        queue.last_id = last_id;
        queue.bytes += message.len();
        queue.messages.push_back(message);
        drop(queue);
        self.session.wake();
        Ok(true)
    }

    #[napi]
    pub fn drain(&self) -> Vec<String> {
        let mut queue = self.session.events.lock().unwrap();
        queue.bytes = 0;
        queue.messages.drain(..).collect()
    }

    #[napi(getter)]
    pub fn closed(&self) -> bool {
        self.session.closed.load(Ordering::Acquire)
    }

    #[napi(getter)]
    pub fn protocol_version(&self) -> u32 {
        1
    }

    #[napi(getter)]
    pub fn close_reason(&self) -> String {
        self.session.reason.lock().unwrap().clone()
    }

    #[napi]
    pub fn close(&self, reason: String) {
        self.session.close(&reason);
    }

    /// Validate a complete commit on the application runtime, with no UI-thread wait.
    #[napi]
    pub fn prepare_batch(&self, json: String) -> Result<Vec<f64>> {
        if json.len() > MAX_BYTES {
            return Err(Error::from_reason("Native commit exceeds 4 MiB"));
        }
        apply_batch_to_tree(&mut self.staging.borrow_mut(), json.as_bytes())
            .map_err(Error::from_reason)
    }

    #[napi]
    pub fn register_fonts(&self, fonts: Vec<Buffer>) -> Result<()> {
        self.session
            .text_system
            .add_fonts(
                fonts
                    .into_iter()
                    .map(|bytes| std::borrow::Cow::Owned(bytes.to_vec()))
                    .collect(),
            )
            .map_err(|error| Error::from_reason(error.to_string()))
    }

    #[napi]
    pub fn measure_text_widths(
        &self,
        family: String,
        size: f64,
        weight: f64,
        texts: Vec<String>,
    ) -> Result<Vec<f64>> {
        if !size.is_finite() || size <= 0.0 || !weight.is_finite() || weight <= 0.0 {
            return Err(Error::from_reason(
                "Text size and weight must be finite and positive",
            ));
        }
        // A worker has no GPUI frame boundary to evict line layouts. Keep its
        // shaping cache scoped to this batch; application callers cache widths.
        let text_system = gpui::WindowTextSystem::new(self.session.text_system.clone());
        Ok(crate::text_measure::widths_with_system(
            &text_system,
            family,
            size,
            weight,
            texts,
        ))
    }

    #[napi]
    pub fn highlight_code(
        &self,
        source: String,
        path: Option<String>,
        language: Option<String>,
    ) -> Vec<Vec<crate::syntax_api::SyntaxToken>> {
        crate::syntax_api::tokens(&source, path.as_deref(), language.as_deref())
    }

    #[napi(ts_return_type = "Promise<Array<string>>")]
    pub fn receive(&self) -> Result<AsyncTask<ReceiveTask>> {
        if self.session.receiving.swap(true, Ordering::AcqRel) {
            return Err(Error::from_reason("Only one native receive may be pending"));
        }
        Ok(AsyncTask::new(ReceiveTask(self.session.clone())))
    }

    /// Read the last complete native frame. A fresh query uses the command channel.
    #[napi]
    pub fn read_snapshot(&self, method: String, element_id: Option<f64>) -> Result<String> {
        let snapshot = self.session.snapshot.lock().unwrap().clone();
        let id = element_id.map(to_element_id).transpose()?.unwrap_or(0);
        let value = match method.as_str() {
            "getElementBounds" => json!(snapshot.bounds.get(&id)),
            "getScrollOffset" => json!(snapshot.scrolls.get(&id)),
            "getListScrollTop" => json!(snapshot.lists.get(&id)),
            "getAutomationTree" => json!(
                self.staging
                    .borrow()
                    .to_automation_json(&snapshot.bounds)
                    .to_string()
            ),
            _ => snapshot
                .values
                .get(&method)
                .cloned()
                .ok_or_else(|| Error::from_reason("Unknown snapshot query"))?,
        };
        Ok(value.to_string())
    }

    /// Automation input must not depend on the launcher's blocked JS event loop.
    #[napi]
    pub fn enable_stdio(&self) -> Result<bool> {
        if std::io::stdin().is_terminal() {
            return Ok(false);
        }
        *STDIO_SESSION.get_or_init(Default::default).lock().unwrap() =
            Arc::downgrade(&self.session);
        STDIO_READER
            .get_or_init(|| {
                std::thread::Builder::new()
                    .name("gpuix-automation-input".into())
                    .spawn(|| {
                        let mut input = std::io::stdin().lock();
                        let mut bytes = [0; 4096];
                        loop {
                            match input.read(&mut bytes) {
                                Ok(0) => break,
                                Ok(count) => {
                                    if let Some(session) = STDIO_SESSION
                                        .get()
                                        .and_then(|slot| slot.lock().unwrap().upgrade())
                                    {
                                        session.emit(json!({"automationBytes": &bytes[..count]}));
                                    }
                                }
                                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {
                                    continue;
                                }
                                Err(error) => {
                                    log::error!("Native automation input failed: {error}");
                                    break;
                                }
                            }
                        }
                    })
                    .map_err(|error| error.to_string())?;
                Ok(())
            })
            .as_ref()
            .map_err(|error| Error::from_reason(error.clone()))?;
        Ok(true)
    }

    #[napi]
    pub fn write_stdio(&self, message: String) -> Result<()> {
        if message.len() > MAX_BYTES {
            return Err(Error::from_reason("Automation response exceeds 4 MiB"));
        }
        std::io::stdout()
            .lock()
            .write_all(message.as_bytes())
            .map_err(|error| Error::from_reason(error.to_string()))
    }
}

pub struct ReceiveTask(Arc<Session>);
impl napi::Task for ReceiveTask {
    type Output = Vec<String>;
    type JsValue = Vec<String>;
    fn compute(&mut self) -> Result<Self::Output> {
        let queue = self.0.events.lock().unwrap();
        // A finite native wait lets runtime teardown finish even if no event arrives.
        let (mut queue, _) = self
            .0
            .changed
            .wait_timeout_while(queue, Duration::from_millis(100), |queue| {
                queue.messages.is_empty() && !self.0.closed.load(Ordering::Acquire)
            })
            .unwrap();
        queue.bytes = 0;
        Ok(queue.messages.drain(..).collect())
    }
    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        self.0.receiving.store(false, Ordering::Release);
        Ok(output)
    }
}

fn execute(renderer: &GpuixRenderer, request: &Request) -> Result<Value> {
    let arg = |index: usize| {
        request
            .args
            .get(index)
            .cloned()
            .ok_or_else(|| Error::from_reason("Missing argument"))
    };
    let number = |index| {
        arg(index)?
            .as_f64()
            .ok_or_else(|| Error::from_reason("Expected number"))
    };
    let string = |index| {
        arg(index)?
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| Error::from_reason("Expected string"))
    };
    let boolean = |index| {
        arg(index)?
            .as_bool()
            .ok_or_else(|| Error::from_reason("Expected boolean"))
    };
    match request.method.as_str() {
        "barrier" => Ok(Value::Null),
        "applyBatch" => Ok(json!(renderer.apply_batch(string(0)?)?)),
        "getAutomationTree" => Ok(json!(renderer.get_automation_tree()?)),
        "getElementBounds" => Ok(json!(renderer.element_bounds(to_element_id(number(0)?)?)?)),
        "getFocusedElementId" => Ok(json!(renderer.get_focused_element_id()?)),
        "getScrollOffset" => Ok(json!(renderer.get_scroll_offset(number(0)?)?)),
        "getListScrollTop" => Ok(json!(renderer.get_list_scroll_top(number(0)?)?)),
        "getSelectedText" => Ok(json!(renderer.get_selected_text())),
        "getSelectionInfo" => Ok(json!(renderer.get_selection_info())),
        "getAllText" => Ok(json!(renderer.get_all_text())),
        "getPaintedText" => Ok(json!(renderer.get_painted_text()?)),
        "getPaintedHighlights" => Ok(json!(renderer.get_painted_highlights()?)),
        "getWindowSize" => {
            let size = renderer.get_window_size()?;
            Ok(json!({"width": size.width, "height": size.height}))
        }
        "focusElement" => {
            renderer.focus_element(number(0)?)?;
            Ok(Value::Null)
        }
        "focusNext" => {
            renderer.focus_next()?;
            Ok(Value::Null)
        }
        "focusPrevious" => {
            renderer.focus_previous()?;
            Ok(Value::Null)
        }
        "focusNextWithin" => {
            renderer.focus_next_within(number(0)?)?;
            Ok(Value::Null)
        }
        "focusPreviousWithin" => {
            renderer.focus_previous_within(number(0)?)?;
            Ok(Value::Null)
        }
        "blur" => {
            renderer.blur()?;
            Ok(Value::Null)
        }
        "clearSelection" => {
            renderer.clear_selection()?;
            Ok(Value::Null)
        }
        "setWindowTitle" => {
            renderer.set_window_title(string(0)?)?;
            Ok(Value::Null)
        }
        "setWindowKeyEvents" => {
            renderer.set_window_key_events(boolean(0)?, boolean(1)?, number(2)?)?;
            Ok(Value::Null)
        }
        "setWindowSelectionChange" => {
            renderer.set_window_selection_change(boolean(0)?, number(1)?)?;
            Ok(Value::Null)
        }
        "scrollTo" => {
            renderer.scroll_to(number(0)?, number(1)?, number(2)?)?;
            Ok(Value::Null)
        }
        "scrollToItem" => {
            renderer.scroll_to_item(number(0)?, number(1)?, Some(number(2)?))?;
            Ok(Value::Null)
        }
        "simulateKeystrokes" => {
            renderer.simulate_keystrokes(string(0)?)?;
            Ok(Value::Null)
        }
        "simulateKeyDown" => {
            renderer.simulate_key_down(string(0)?, request.args.get(1).and_then(Value::as_bool))?;
            Ok(Value::Null)
        }
        "simulateKeyUp" => {
            renderer.simulate_key_up(string(0)?)?;
            Ok(Value::Null)
        }
        "simulateClick" => {
            renderer.simulate_click(
                number(0)?,
                number(1)?,
                request
                    .args
                    .get(2)
                    .and_then(Value::as_u64)
                    .map(|v| v as u32),
                request
                    .args
                    .get(3)
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            )?;
            Ok(Value::Null)
        }
        "simulateMouseDown" => {
            renderer.simulate_mouse_down(
                number(0)?,
                number(1)?,
                request
                    .args
                    .get(2)
                    .and_then(Value::as_u64)
                    .map(|v| v as u32),
                request
                    .args
                    .get(3)
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            )?;
            Ok(Value::Null)
        }
        "simulateMouseUp" => {
            renderer.simulate_mouse_up(
                number(0)?,
                number(1)?,
                request
                    .args
                    .get(2)
                    .and_then(Value::as_u64)
                    .map(|v| v as u32),
                request
                    .args
                    .get(3)
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            )?;
            Ok(Value::Null)
        }
        "simulateMouseMove" => {
            renderer.simulate_mouse_move(
                number(0)?,
                number(1)?,
                request
                    .args
                    .get(2)
                    .and_then(Value::as_u64)
                    .map(|v| v as u32),
                request
                    .args
                    .get(3)
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            )?;
            Ok(Value::Null)
        }
        "simulateScrollWheel" => {
            renderer.simulate_scroll_wheel(
                number(0)?,
                number(1)?,
                number(2)?,
                number(3)?,
                request
                    .args
                    .get(4)
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            )?;
            Ok(Value::Null)
        }
        "closeWindow" => {
            update_window_without_view(|window, _| window.remove_window())?;
            Ok(Value::Null)
        }
        #[cfg(feature = "test-support")]
        "terminateAppForTest" => {
            MAC_PLATFORM.with(|platform| {
                platform
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .request_test_termination()
            });
            Ok(Value::Null)
        }
        #[cfg(feature = "test-support")]
        "closeWindowAfterForTest" => {
            let delay = number(0)?;
            if !(0.0..=10_000.0).contains(&delay) {
                return Err(Error::from_reason(
                    "Close delay must be between 0 and 10000 ms",
                ));
            }
            let window = GPUI_WINDOW.with(|window| window.borrow().unwrap());
            GPUI_APP.with(|app| {
                app.borrow().as_ref().unwrap().update(|cx| {
                    cx.spawn(async move |cx| {
                        cx.background_executor()
                            .timer(Duration::from_millis(delay as u64))
                            .await;
                        if let Err(error) = window.update(cx, |_, window, _| window.remove_window())
                        {
                            log::debug!("Scheduled test close found a disposed window: {error}");
                        }
                    })
                    .detach();
                })
            });
            Ok(Value::Null)
        }
        #[cfg(feature = "test-support")]
        "dispatchKeysAfterForTest" => {
            let delay = number(0)?;
            let keys = string(1)?;
            if !(0.0..=10_000.0).contains(&delay) {
                return Err(Error::from_reason(
                    "Input delay must be between 0 and 10000 ms",
                ));
            }
            let window =
                gpui::AnyWindowHandle::from(GPUI_WINDOW.with(|window| window.borrow().unwrap()));
            GPUI_APP.with(|app| {
                app.borrow().as_ref().unwrap().update(|cx| {
                    cx.spawn(async move |cx| {
                        cx.background_executor()
                            .timer(Duration::from_millis(delay as u64))
                            .await;
                        match window.update(cx, |_, window, cx| {
                            crate::automation::dispatch_keystrokes(window, cx, &keys)
                        }) {
                            Ok(Ok(())) => (),
                            result => log::error!("Scheduled test input failed: {result:?}"),
                        }
                    })
                    .detach();
                })
            });
            Ok(Value::Null)
        }
        "captureScreenshot" => {
            renderer.capture_screenshot(string(0)?)?;
            Ok(Value::Null)
        }
        "shutdown" => Ok(Value::Null),
        "clockPause" => Ok(json!(renderer.clock_pause()?)),
        "clockResume" => Ok(json!(renderer.clock_resume()?)),
        "clockSet" => Ok(json!(renderer.clock_set(number(0)?)?)),
        "clockFastForward" => Ok(json!(renderer.clock_fast_forward(number(0)?)?)),
        #[cfg(feature = "test-support")]
        "resizeWindowForTest" => {
            renderer.resize_window_for_test(number(0)?, number(1)?)?;
            Ok(Value::Null)
        }
        #[cfg(feature = "test-support")]
        "startHostProbe" => {
            renderer.start_host_probe(
                number(0)? as u32,
                number(1)?,
                request
                    .args
                    .get(2)
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            )?;
            Ok(Value::Null)
        }
        #[cfg(feature = "test-support")]
        "markHostProbe" => {
            renderer.mark_host_probe(string(0)?);
            Ok(Value::Null)
        }
        #[cfg(feature = "test-support")]
        "takeHostProbe" => Ok(json!(renderer.take_host_probe())),
        "startFrameProfile" => {
            renderer.start_frame_profile(Some(false));
            Ok(Value::Null)
        }
        "takeFrameProfile" => Ok(json!(renderer.take_frame_profile())),
        #[cfg(feature = "test-support")]
        "queueAppKitMouseMoves" => {
            renderer.queue_app_kit_mouse_moves(
                number(0)? as u32,
                number(1)?,
                number(2)?,
                number(3)?,
            )?;
            Ok(Value::Null)
        }
        _ => Err(Error::from_reason(format!(
            "Unknown native command: {}",
            request.method
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> NativeClient {
        let (wake, _) = mpsc::channel(1);
        NativeClient {
            session: Arc::new(Session {
                commands: Mutex::new(Queue::default()),
                events: Mutex::new(Queue::default()),
                wake: Mutex::new(wake),
                attached: AtomicBool::new(true),
                closed: AtomicBool::new(false),
                reason: Mutex::new(String::new()),
                changed: Condvar::new(),
                receiving: AtomicBool::new(false),
                text_system: Arc::new(gpui::TextSystem::new(Arc::new(gpui::NoopTextSystem::new()))),
                snapshot: Mutex::new(Arc::new(Snapshot::default())),
            }),
            staging: RefCell::new(RetainedTree::new()),
        }
    }

    fn request(id: u32) -> String {
        json!({"id": id, "method": "barrier", "args": []}).to_string()
    }

    #[test]
    fn backpressure_preserves_order_and_allows_retry_without_reusing_an_accepted_id() {
        let client = client();
        for id in 1..=MAX_COMMANDS as u32 {
            assert!(client.send(request(id)).unwrap());
        }
        assert!(!client.send(request(257)).unwrap());
        {
            let mut queue = client.session.commands.lock().unwrap();
            let first = queue.messages.pop_front().unwrap();
            queue.bytes -= first.len();
            assert_eq!(serde_json::from_str::<Request>(&first).unwrap().id, 1);
            assert_eq!(queue.last_id, 256);
        }
        assert!(client.send(request(257)).unwrap());
        assert!(client.send(request(257)).is_err());
        let queue = client.session.commands.lock().unwrap();
        let ids: Vec<_> = queue
            .messages
            .iter()
            .map(|message| serde_json::from_str::<Request>(message).unwrap().id)
            .collect();
        assert_eq!(ids, (2..=257).collect::<Vec<_>>());
    }

    #[test]
    fn malformed_or_nested_transactions_do_not_enter_the_queue() {
        let client = client();
        for message in [
            json!({"id": 0, "method": "transaction", "args": []}),
            json!({"id": 0, "method": "transaction", "args": [[{"id": 1, "method": "transaction", "args": []}]]}),
            json!({"id": 0, "method": "transaction", "args": [[{"id": 2, "method": "barrier"}, {"id": 1, "method": "barrier"}]]}),
        ] {
            assert!(client.send(message.to_string()).is_err());
        }
        assert!(client.session.commands.lock().unwrap().messages.is_empty());
        assert!(client.send(request(1)).unwrap());
    }

    #[test]
    fn byte_limit_and_closed_sessions_reject_new_work() {
        let client = client();
        assert!(client.send("x".repeat(MAX_BYTES + 1)).is_err());
        assert!(client.session.commands.lock().unwrap().messages.is_empty());
        client.session.close("test shutdown");
        assert_eq!(client.close_reason(), "test shutdown");
        assert!(client.send(request(1)).is_err());
        client.session.close("later error");
        assert_eq!(client.close_reason(), "test shutdown");
    }

    #[test]
    fn event_overflow_has_an_explicit_terminal_reason() {
        let client = client();
        for index in 0..MAX_EVENTS {
            client.session.emit(json!({"event": index}));
        }
        client.session.emit(json!({"event": MAX_EVENTS}));
        assert!(client.closed());
        assert_eq!(client.close_reason(), "Application event queue overflow");
        assert_eq!(client.drain().len(), MAX_EVENTS);
    }
}
