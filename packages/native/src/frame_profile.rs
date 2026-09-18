//! Opt-in diagnostics using GPUI's real draw and platform-submission timestamps.
use gpui::profiler::{FrameEvent, FrameTimingCollector};
use std::cell::RefCell;
use web_time::Instant;
const LIMIT: usize = 8192;
struct Profile {
    origin: Instant,
    enabled_by_profile: bool,
    collector: FrameTimingCollector,
    builds: Vec<[f64; 3]>,
    batches: Vec<[f64; 2]>,
}
thread_local! { static PROFILE: RefCell<Option<Profile>> = const { RefCell::new(None) }; }
pub fn start(keep_visible: bool) {
    let _ = take();
    #[cfg(target_os = "macos")]
    visibility::begin(keep_visible);
    let enabled_by_profile = gpui::profiler::set_trace_enabled(true);
    PROFILE.with(|slot| {
        *slot.borrow_mut() = Some(Profile {
            enabled_by_profile,
            origin: Instant::now(),
            collector: FrameTimingCollector::new(),
            builds: Vec::new(),
            batches: Vec::new(),
        })
    });
}
pub fn build_start() -> Option<Instant> {
    PROFILE.with(|slot| slot.borrow().as_ref().map(|_| Instant::now()))
}
pub fn build_end(start: Option<Instant>, nodes: usize) {
    if let Some(start) = start {
        PROFILE.with(|slot| {
            if let Some(p) = slot.borrow_mut().as_mut() {
                if p.builds.len() < LIMIT {
                    p.builds.push([
                        start.duration_since(p.origin).as_secs_f64() * 1000.,
                        start.elapsed().as_secs_f64() * 1000.,
                        nodes as f64,
                    ]);
                }
            }
        });
    }
}
pub fn batch(bytes: usize) {
    PROFILE.with(|slot| {
        if let Some(p) = slot.borrow_mut().as_mut() {
            if p.batches.len() < LIMIT {
                p.batches
                    .push([p.origin.elapsed().as_secs_f64() * 1000., bytes as f64]);
            }
        }
    });
}
pub fn take() -> String {
    let result = PROFILE.with(|slot| slot.borrow_mut().take().map(|mut p| {
        let events: Vec<_> = p.collector.collect_unseen().into_iter().map(|event| match event {
            FrameEvent::Draw(f) => serde_json::json!({"kind":"draw", "startMs":f.draw_start.saturating_duration_since(p.origin).as_secs_f64()*1000., "durationMs":f.draw_duration().as_secs_f64()*1000.}),
            FrameEvent::Present(f) => serde_json::json!({"kind":"submit", "startMs":f.present_start.saturating_duration_since(p.origin).as_secs_f64()*1000., "durationMs":f.present_duration().as_secs_f64()*1000.}),
        }).collect();
        let result = serde_json::json!({"version":1, "durationMs":p.origin.elapsed().as_secs_f64()*1000., "events":events, "builds":p.builds, "batches":p.batches});
        if p.enabled_by_profile { gpui::profiler::set_trace_enabled(false); }
        result
    }));
    #[cfg(target_os = "macos")]
    visibility::end();
    result
        .unwrap_or_else(|| serde_json::json!({"error":"Frame profile has not been started"}))
        .to_string()
}

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
mod visibility {
    use super::*;
    use objc::{
        class, msg_send,
        runtime::{BOOL, NO, Object, YES},
        sel, sel_impl,
    };
    thread_local! { static WINDOW: RefCell<Option<(usize, isize, BOOL, BOOL, usize)>> = const { RefCell::new(None) }; }
    pub fn begin(keep_visible: bool) {
        end();
        unsafe {
            let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
            let windows: *mut Object = msg_send![app, windows];
            let count: usize = msg_send![windows, count];
            if count == 0 {
                return;
            }
            let window: *mut Object = msg_send![windows, objectAtIndex: 0usize];
            let level: isize = msg_send![window, level];
            let hides: BOOL = msg_send![window, hidesOnDeactivate];
            let ignores_mouse: BOOL = msg_send![window, ignoresMouseEvents];
            let process: *mut Object = msg_send![class!(NSProcessInfo), processInfo];
            let reason: *mut Object = msg_send![class!(NSString), alloc];
            let reason: *mut Object =
                msg_send![reason, initWithUTF8String: c"GPUIX frame timing capture".as_ptr()];
            // NSActivityUserInitiatedAllowingIdleSystemSleep. Synthetic GPUI
            // input is not AppKit user activity, so keep this finite capture
            // out of App Nap without preventing display or system sleep.
            let options: u64 = 0x00ff_ffff & !(1 << 20);
            let activity: *mut Object =
                msg_send![process, beginActivityWithOptions: options reason: reason];
            let _: *mut Object = msg_send![activity, retain];
            let _: () = msg_send![reason, release];
            let _: *mut Object = msg_send![window, retain];
            if keep_visible {
                let _: () = msg_send![window, setHidesOnDeactivate: NO];
                let _: () = msg_send![window, setIgnoresMouseEvents: YES];
                let _: () = msg_send![window, setLevel: 3isize];
                let _: () = msg_send![window, orderFrontRegardless];
            }
            WINDOW.with(|slot| {
                *slot.borrow_mut() = Some((
                    window as usize,
                    level,
                    hides,
                    ignores_mouse,
                    activity as usize,
                ))
            });
        }
    }
    pub fn end() {
        if let Some((window, level, hides, ignores_mouse, activity)) =
            WINDOW.with(|slot| slot.borrow_mut().take())
        {
            unsafe {
                let window = window as *mut Object;
                let _: () = msg_send![window, setLevel: level];
                let _: () = msg_send![window, setHidesOnDeactivate: hides];
                let _: () = msg_send![window, setIgnoresMouseEvents: ignores_mouse];
                let process: *mut Object = msg_send![class!(NSProcessInfo), processInfo];
                let activity = activity as *mut Object;
                let _: () = msg_send![process, endActivity: activity];
                let _: () = msg_send![activity, release];
                let _: () = msg_send![window, release];
            }
        }
    }
}
