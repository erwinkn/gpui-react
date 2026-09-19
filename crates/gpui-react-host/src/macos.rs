use super::*;
use futures::StreamExt;
use gpui_react::{
    Host,
    gpui::{self, AppContext},
};
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

thread_local! { static RUNNING: RefCell<bool> = const { RefCell::new(false) }; }

#[allow(unexpected_cfgs)]
pub(super) fn is_main_thread() -> bool {
    use objc::{class, msg_send, sel, sel_impl};
    let main: objc::runtime::BOOL = unsafe { msg_send![class!(NSThread), isMainThread] };
    main == objc::runtime::YES
}

#[napi(object)]
pub struct HostOptions {
    pub title: Option<String>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub show: Option<bool>,
}

struct NativeState {
    // GPUI entities must be released before the app that owns them.
    _root: gpui::Entity<Host>,
    window: gpui::WindowHandle<Host>,
    app: gpui::ApplicationHandle,
    platform: Rc<gpui_macos::MacPlatform>,
}

#[napi]
pub struct NativeHost {
    id: u32,
    session: Arc<Session>,
    receiver: RefCell<Option<mpsc::Receiver<()>>>,
    state: RefCell<Option<NativeState>>,
    show: bool,
}

#[napi]
impl NativeHost {
    #[napi(constructor)]
    pub fn new(options: Option<HostOptions>) -> Result<Self> {
        if !is_main_thread() {
            return Err(Error::from_reason("NativeHost requires the main thread"));
        }
        if RUNNING.with(|active| *active.borrow()) {
            return Err(Error::from_reason("A native host is already active"));
        }
        let options = options.unwrap_or(HostOptions {
            title: None,
            width: None,
            height: None,
            show: None,
        });
        let width = options.width.unwrap_or(800.0) as f32;
        let height = options.height.unwrap_or(600.0) as f32;
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return Err(Error::from_reason(
                "Window dimensions must be finite and positive",
            ));
        }
        let registry = registry().map_err(|e| Error::from_reason(e.to_string()))?;
        let (id, session, receiver) = Session::new();
        let output = session.clone();
        let events = Arc::new(move |event: gpui_react::Emission| {
            match event.payload {
                Ok(payload) => output.emit(serde_json::json!({"event":{"target":event.target,"subscription":event.subscription,"payload":payload}})),
                Err(error) => output.close(format!("Native event serialization failed: {error}")),
            }
        });
        let platform = Rc::new(gpui_macos::MacPlatform::new_embedded());
        let opened = Rc::new(RefCell::new(None));
        let opened_for_app = opened.clone();
        let app = gpui::Application::with_platform(platform.clone())
            .with_quit_mode(gpui::QuitMode::LastWindowClosed)
            .run_embedded(move |cx| {
                let bounds =
                    gpui::Bounds::centered(None, gpui::size(gpui::px(width), gpui::px(height)), cx);
                let result = cx.open_window(
                    gpui::WindowOptions {
                        window_bounds: Some(gpui::WindowBounds::Windowed(bounds)),
                        titlebar: Some(gpui::TitlebarOptions {
                            title: Some(
                                options.title.unwrap_or_else(|| "React GPUI".into()).into(),
                            ),
                            ..Default::default()
                        }),
                        show: false,
                        focus: false,
                        ..Default::default()
                    },
                    |_, cx| cx.new(|_| Host::new(registry, events)),
                );
                *opened_for_app.borrow_mut() = Some(result);
            });
        let window = opened
            .borrow_mut()
            .take()
            .ok_or_else(|| Error::from_reason("Native application did not start"))?
            .map_err(|e| Error::from_reason(e.to_string()))?;
        let root = app
            .update(|cx| window.entity(cx))
            .map_err(|e| Error::from_reason(e.to_string()))?;
        let closing_root = root.downgrade();
        app.update(|cx| {
            cx.update_window(window.into(), |_, window, _| {
                window.on_close(move |window, cx| {
                    let _ = closing_root.update(cx, |host, cx| host.clear(window, cx));
                });
            })
        })
        .map_err(|e| Error::from_reason(e.to_string()))?;
        RUNNING.with(|active| *active.borrow_mut() = true);
        Ok(Self {
            id,
            session,
            receiver: RefCell::new(Some(receiver)),
            state: RefCell::new(Some(NativeState {
                _root: root,
                window,
                app,
                platform,
            })),
            show: options.show.unwrap_or(true),
        })
    }

    #[napi(getter)]
    pub fn id(&self) -> u32 {
        self.id
    }

    #[napi]
    pub fn close(&self, reason: String) {
        self.session.close(reason);
    }

    #[napi]
    pub fn run(&self) -> Result<String> {
        let _signals = super::signals::HostSignals::new(&self.session)?;
        let state = self
            .state
            .borrow_mut()
            .take()
            .ok_or_else(|| Error::from_reason("NativeHost can run only once"))?;
        let mut receiver = self.receiver.borrow_mut().take().unwrap();
        let session = self.session.clone();
        let window_handle = state.window;
        let show = self.show;
        let task = state.app.update(|cx| {
            cx.spawn(async move |cx| {
                let mut first = true;
                loop {
                    if session.closed() {
                        cx.update(|cx| cx.quit());
                        break;
                    }
                    let start = Instant::now();
                    for _ in 0..32 {
                        let Some(transaction) = session.pop() else {
                            break;
                        };
                        let output = session.clone();
                        let result = window_handle.update(cx, |host, window, cx| {
                            let reply = host.apply_prepared(transaction, window, cx)?;
                            // This defer follows native Emit and subscription-retirement
                            // effects. JS receives earlier events before retiring callbacks.
                            cx.defer(move |_| output.emit(serde_json::json!({"reply":reply})));
                            Ok::<_, anyhow::Error>(())
                        });
                        match result {
                            Ok(Ok(())) => {}
                            Ok(Err(error)) => {
                                session.close(format!("Native transaction failed: {error:#}"));
                                break;
                            }
                            Err(error) => {
                                session.close(format!("Native window failed: {error:#}"));
                                break;
                            }
                        }
                        if first {
                            first = false;
                            let result = cx.update_window(window_handle.into(), |_, window, cx| {
                                window.draw(cx).clear(cx);
                                if show {
                                    window.show_inactive()?;
                                }
                                Ok::<_, anyhow::Error>(())
                            });
                            if let Err(error) = result.and_then(|r| r) {
                                session.close(format!("First frame failed: {error:#}"));
                            }
                        }
                        if session.closed() || start.elapsed() >= Duration::from_millis(4) {
                            break;
                        }
                    }
                    if session.closed() {
                        continue;
                    }
                    if session.queues.lock().unwrap().commands.is_empty() {
                        if receiver.next().await.is_none() {
                            break;
                        }
                    } else {
                        cx.background_executor()
                            .timer(Duration::from_millis(1))
                            .await;
                    }
                }
            })
        });
        state.platform.run_event_loop();
        self.session.close("Native window closed");
        drop(task);
        state.app.update(|cx| {
            let _ = state
                .window
                .update(cx, |host, window, cx| host.clear(window, cx));
            let _ = cx.update_window(state.window.into(), |_, window, cx| {
                window.draw(cx).clear(cx)
            });
        });
        drop(state);
        RUNNING.with(|active| *active.borrow_mut() = false);
        SESSIONS.get().unwrap().lock().unwrap().remove(&self.id);
        Ok(self.session.queues.lock().unwrap().reason.clone().unwrap())
    }
}

impl Drop for NativeHost {
    fn drop(&mut self) {
        self.session.close("Native host disposed");
        if let Some(state) = self.state.borrow_mut().take() {
            state.app.update(|cx| cx.quit());
            state.platform.run_event_loop();
            drop(state);
            RUNNING.with(|active| *active.borrow_mut() = false);
        }
        if let Some(sessions) = SESSIONS.get() {
            sessions.lock().unwrap().remove(&self.id);
        }
    }
}
