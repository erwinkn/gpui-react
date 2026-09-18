use super::host_runtime::Session;
use napi::{Error, Result};
use signal_hook::{
    consts::{SIGINT, SIGTERM},
    iterator::Signals,
};
use std::sync::{Arc, Mutex, OnceLock, Weak};

static ACTIVE: OnceLock<Mutex<Weak<Session>>> = OnceLock::new();
static READER: OnceLock<std::result::Result<(), String>> = OnceLock::new();

pub(super) struct HostSignals {
    session: Weak<Session>,
}

impl HostSignals {
    pub(super) fn new(session: &Arc<Session>) -> Result<Self> {
        let session = Arc::downgrade(session);
        *ACTIVE.get_or_init(Default::default).lock().unwrap() = session.clone();
        // Unregistering signal-hook's last action leaves the OS signal ignored.
        // Keep one reader for the process and retain default behavior between hosts.
        READER
            .get_or_init(|| {
                let mut signals =
                    Signals::new([SIGINT, SIGTERM]).map_err(|error| error.to_string())?;
                std::thread::Builder::new()
                    .name("gpuix-host-signals".into())
                    .spawn(move || {
                        for signal in signals.forever() {
                            let session = ACTIVE
                                .get()
                                .and_then(|active| active.lock().unwrap().upgrade());
                            if let Some(session) = session.filter(|session| !session.is_closed()) {
                                session.close(&format!("Process signal {signal}"));
                            } else if let Err(error) =
                                signal_hook::low_level::emulate_default_handler(signal)
                            {
                                log::error!("Could not restore default signal behavior: {error}");
                            }
                        }
                    })
                    .map_err(|error| error.to_string())?;
                Ok(())
            })
            .as_ref()
            .map_err(|error| Error::from_reason(error.clone()))?;
        Ok(Self { session })
    }
}

impl Drop for HostSignals {
    fn drop(&mut self) {
        if let Some(active) = ACTIVE.get() {
            let mut active = active.lock().unwrap();
            if Weak::ptr_eq(&active, &self.session) {
                *active = Weak::new();
            }
        }
    }
}
