//! A native UI loop and bounded worker channel. The worker owns no Rust UI tree.
use futures::channel::mpsc;
use gpui_react::{Decoder, Prepared, Registry};
use napi::{
    Env, Error, Result,
    bindgen_prelude::{AsyncTask, Either, Uint8Array},
};
use napi_derive::napi;
use serde_json::Value;
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Condvar, Mutex, OnceLock, Weak,
        atomic::{AtomicU32, Ordering},
    },
};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod signals;
pub use gpui_react;
#[cfg(target_os = "macos")]
pub use macos::*;

const MAX_BYTES: usize = 4 * 1024 * 1024;
const MAX_TRANSACTIONS: usize = 256;
const MAX_EVENTS: usize = 4096;

type Register = fn(&mut Registry) -> anyhow::Result<()>;
static COMPONENTS: Mutex<Vec<Register>> = Mutex::new(Vec::new());
static SESSIONS: OnceLock<Mutex<HashMap<u32, Weak<Session>>>> = OnceLock::new();
static NEXT_SESSION: AtomicU32 = AtomicU32::new(1);

/// Call from the composition's module initializer. Registrations contain only
/// functions. Each native window creates its own component registry and views.
pub fn register_components(register: Register) {
    let mut components = COMPONENTS.lock().unwrap();
    if !components
        .iter()
        .any(|old| std::ptr::fn_addr_eq(*old, register))
    {
        components.push(register);
    }
}

fn registry() -> anyhow::Result<Registry> {
    let mut registry = Registry::default();
    // The built-in controls are always present; composition registrations add
    // their own components on top.
    gpui_react::register_builtins(&mut registry)?;
    for register in COMPONENTS.lock().unwrap().iter() {
        register(&mut registry)?;
    }
    Ok(registry)
}

#[derive(Default)]
struct Queues {
    commands: VecDeque<(Prepared, usize)>,
    command_bytes: usize,
    messages: VecDeque<String>,
    message_bytes: usize,
    reason: Option<String>,
    attached: bool,
    receiving: bool,
}

struct Session {
    queues: Mutex<Queues>,
    changed: Condvar,
    wake: Mutex<mpsc::Sender<()>>,
}

impl Session {
    fn new() -> (u32, Arc<Self>, mpsc::Receiver<()>) {
        let (sender, receiver) = mpsc::channel(1);
        let session = Arc::new(Self {
            queues: Mutex::new(Queues::default()),
            changed: Condvar::new(),
            wake: Mutex::new(sender),
        });
        let id = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
        SESSIONS
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .insert(id, Arc::downgrade(&session));
        (id, session, receiver)
    }

    fn wake(&self) {
        let _ = self.wake.lock().unwrap().try_send(());
    }

    fn close(&self, reason: impl Into<String>) {
        let mut queues = self.queues.lock().unwrap();
        if queues.reason.is_none() {
            queues.reason = Some(reason.into());
        }
        drop(queues);
        self.changed.notify_all();
        self.wake();
    }

    fn closed(&self) -> bool {
        self.queues.lock().unwrap().reason.is_some()
    }

    fn emit(&self, message: Value) {
        let message = message.to_string();
        let mut queues = self.queues.lock().unwrap();
        if queues.reason.is_some() {
            return;
        }
        if queues.messages.len() >= MAX_EVENTS || queues.message_bytes + message.len() > MAX_BYTES {
            drop(queues);
            self.close("Native event queue overflow");
            return;
        }
        queues.message_bytes += message.len();
        queues.messages.push_back(message);
        drop(queues);
        self.changed.notify_all();
    }

    fn pop(&self) -> Option<Prepared> {
        let mut queues = self.queues.lock().unwrap();
        let (transaction, bytes) = queues.commands.pop_front()?;
        queues.command_bytes -= bytes;
        Some(transaction)
    }
}

#[napi]
pub struct NativeClient {
    session: Arc<Session>,
    decoder: Mutex<Decoder>,
}

#[napi]
impl NativeClient {
    #[napi(constructor)]
    pub fn new(id: u32, env: Env) -> Result<Self> {
        #[cfg(target_os = "macos")]
        if macos::is_main_thread() {
            return Err(Error::from_reason(
                "NativeClient must run on the application worker",
            ));
        }
        let session = SESSIONS
            .get()
            .and_then(|sessions| sessions.lock().unwrap().get(&id).and_then(Weak::upgrade))
            .ok_or_else(|| Error::from_reason("Unknown native session"))?;
        {
            let mut queues = session.queues.lock().unwrap();
            if queues.attached || queues.reason.is_some() {
                return Err(Error::from_reason("Native session is attached or closed"));
            }
            queues.attached = true;
        }
        env.add_env_cleanup_hook(session.clone(), |session| {
            session.close("Application worker unloaded")
        })?;
        let registry = registry().map_err(|e| Error::from_reason(e.to_string()))?;
        Ok(Self {
            session,
            decoder: Mutex::new(Decoder::new(registry)),
        })
    }

    /// Decode to typed data on the worker. The UI thread receives typed
    /// operations and does no wire work; it applies them to the host tree.
    /// Binary payloads are borrowed for the duration of this call and never
    /// retained, so the worker may reuse the buffer afterwards.
    #[napi]
    pub fn send(&self, encoded: Either<String, Uint8Array>) -> Result<()> {
        let len = match &encoded {
            Either::A(text) => text.len(),
            Either::B(bytes) => bytes.len(),
        };
        if len > MAX_BYTES {
            return Err(Error::from_reason("Native transaction exceeds byte limit"));
        }
        let transaction = {
            let mut decoder = self.decoder.lock().unwrap();
            match &encoded {
                Either::A(text) => decoder.parse(text),
                Either::B(bytes) => decoder.parse_binary(bytes),
            }
        }
        .map_err(|e| Error::from_reason(format!("Native transaction failed: {e:#}")))?;
        let mut queues = self.session.queues.lock().unwrap();
        if let Some(reason) = &queues.reason {
            return Err(Error::from_reason(reason.clone()));
        }
        if queues.commands.len() >= MAX_TRANSACTIONS || queues.command_bytes + len > MAX_BYTES {
            return Err(Error::from_reason("Native command queue is full"));
        }
        queues.command_bytes += len;
        queues.commands.push_back((transaction, len));
        drop(queues);
        self.session.wake();
        Ok(())
    }

    /// The component kind table as JSON: the worker encodes props against it.
    #[napi]
    pub fn schema(&self) -> Result<String> {
        serde_json::to_string(&self.decoder.lock().unwrap().registry().schema())
            .map_err(|e| Error::from_reason(e.to_string()))
    }

    #[napi]
    pub fn receive(&self) -> Result<AsyncTask<Receive>> {
        let mut queues = self.session.queues.lock().unwrap();
        if queues.receiving {
            return Err(Error::from_reason("Only one native receiver is allowed"));
        }
        queues.receiving = true;
        Ok(AsyncTask::new(Receive(self.session.clone())))
    }

    #[napi]
    pub fn close(&self, reason: String) {
        self.session.close(reason);
    }
}

pub struct Receive(Arc<Session>);
impl napi::Task for Receive {
    type Output = Vec<String>;
    type JsValue = Vec<String>;
    fn compute(&mut self) -> Result<Self::Output> {
        let mut queues = self.0.queues.lock().unwrap();
        while queues.messages.is_empty() && queues.reason.is_none() {
            queues = self.0.changed.wait(queues).unwrap();
        }
        queues.receiving = false;
        if queues.messages.is_empty() {
            return Err(Error::from_reason(queues.reason.clone().unwrap()));
        }
        queues.message_bytes = 0;
        Ok(queues.messages.drain(..).collect())
    }
    fn resolve(&mut self, _: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

#[napi]
pub fn bridge_runtime_version() -> u32 {
    2
}

#[cfg(test)]
mod tests {
    use super::*;
    use napi::Task;

    #[test]
    fn admission_is_bounded_and_pop_preserves_transaction_order() {
        let (_, session, _wake) = Session::new();
        let client = NativeClient {
            session: session.clone(),
            decoder: Mutex::new(Decoder::new(Registry::default())),
        };
        for sequence in 1..=MAX_TRANSACTIONS {
            client
                .send(Either::A(
                    serde_json::json!({"version":1,"sequence":sequence,"operations":[]})
                        .to_string(),
                ))
                .unwrap();
        }
        assert!(
            client
                .send(Either::A(
                    serde_json::json!({"version":1,"sequence":MAX_TRANSACTIONS+1,"operations":[]})
                        .to_string()
                ))
                .is_err()
        );
        for sequence in 1..=MAX_TRANSACTIONS {
            assert_eq!(session.pop().unwrap().sequence(), sequence as u64);
        }
        assert_eq!(session.queues.lock().unwrap().command_bytes, 0);
        assert!(session.pop().is_none());
        assert!(client.send(Either::A("not json".into())).is_err());
        assert!(client.send(Either::A(" ".repeat(MAX_BYTES + 1))).is_err());
        assert!(session.pop().is_none());
        session.close("done");
        assert!(
            client
                .send(Either::A(serde_json::json!({"version":1,"sequence":999,"operations":[]}).to_string()))
                .is_err()
        );
    }

    #[test]
    fn output_overflow_is_terminal_and_does_not_drop_accepted_records() {
        let (_, session, _wake) = Session::new();
        for sequence in 0..MAX_EVENTS {
            session.emit(serde_json::json!({"sequence":sequence}));
        }
        session.emit(serde_json::json!({"overflow":true}));
        assert!(session.closed());
        let messages = Receive(session.clone()).compute().unwrap();
        assert_eq!(messages.len(), MAX_EVENTS);
        assert_eq!(
            serde_json::from_str::<Value>(&messages[0]).unwrap()["sequence"],
            0
        );
        assert!(Receive(session).compute().is_err());
    }

    #[test]
    fn session_close_wakes_a_waiting_receiver() {
        let (_, session, _wake) = Session::new();
        let receiver = session.clone();
        let thread = std::thread::spawn(move || Receive(receiver).compute());
        session.close("worker stopped");
        assert!(
            thread
                .join()
                .unwrap()
                .unwrap_err()
                .to_string()
                .contains("worker stopped")
        );
    }
}
