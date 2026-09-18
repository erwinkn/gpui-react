//! AVFoundation decode into GPUI's native CoreVideo/Metal surface, without an NSView/webview.
#![allow(unexpected_cfgs)] // objc 0.2 emits legacy cargo-clippy cfgs.
use super::{CustomElement, CustomElementFactory, CustomRenderContext};
use core_foundation::base::TCFType;
use core_video::pixel_buffer::{CVPixelBuffer, CVPixelBufferRef};
use gpui::prelude::*;
use objc::{class, msg_send, runtime::Object, sel, sel_impl};
use serde_json::Value;
use std::{ffi::CString, ptr};

#[link(name = "AVFoundation", kind = "framework")]
extern "C" {}
#[link(name = "CoreMedia", kind = "framework")]
extern "C" {
    fn CMTimeGetSeconds(time: CMTime) -> f64;
}
#[repr(C)]
#[derive(Clone, Copy)]
struct CMTime {
    value: i64,
    timescale: i32,
    flags: u32,
    epoch: i64,
}
unsafe impl objc::Encode for CMTime {
    fn encode() -> objc::Encoding {
        unsafe { objc::Encoding::from_str("{CMTime=qiIq}") }
    }
}
const ZERO: CMTime = CMTime {
    value: 0,
    timescale: 1,
    flags: 1,
    epoch: 0,
};
unsafe fn ns(s: &str) -> *mut Object {
    let s = CString::new(s).unwrap_or_default();
    msg_send![class!(NSString),stringWithUTF8String:s.as_ptr()]
}
struct Player {
    player: *mut Object,
    item: *mut Object,
    output: *mut Object,
    last: Option<CVPixelBuffer>,
}
impl Drop for Player {
    fn drop(&mut self) {
        unsafe {
            let _: () = msg_send![self.player, pause];
            let _: () = msg_send![self.player, release];
            let _: () = msg_send![self.item, release];
            let _: () = msg_send![self.output, release];
        }
    }
}
impl Player {
    unsafe fn new(src: &str) -> Option<Self> {
        let url: *mut Object = if src.starts_with("https://") || src.starts_with("http://") {
            msg_send![class!(NSURL),URLWithString:ns(src)]
        } else {
            msg_send![class!(NSURL),fileURLWithPath:ns(src)]
        };
        if url.is_null() {
            return None;
        }
        let item: *mut Object = msg_send![class!(AVPlayerItem),playerItemWithURL:url];
        if item.is_null() {
            return None;
        }
        let _: () = msg_send![item, retain];
        // GPUI's Metal surface shader accepts full-range bi-planar YUV (420f).
        let format: *mut Object = msg_send![class!(NSNumber),numberWithUnsignedInt:0x34323066u32];
        let attrs: *mut Object = msg_send![class!(NSDictionary),dictionaryWithObject:format forKey:ns("PixelFormatType")];
        let output: *mut Object = msg_send![class!(AVPlayerItemVideoOutput), alloc];
        let output: *mut Object = msg_send![output,initWithPixelBufferAttributes:attrs];
        let _: () = msg_send![item,addOutput:output];
        let player: *mut Object = msg_send![class!(AVPlayer),playerWithPlayerItem:item];
        let _: () = msg_send![player, retain];
        let _: () = msg_send![player,setMuted:true];
        Some(Self {
            player,
            item,
            output,
            last: None,
        })
    }
}
pub struct VideoFactory;
impl CustomElementFactory for VideoFactory {
    fn element_type(&self) -> &str {
        "cherry-video"
    }
    fn create(&self, _: u64) -> Box<dyn CustomElement> {
        Box::new(Video {
            src: String::new(),
            player: None,
            playing: true,
            looping: true,
            reported: String::new(),
            seek: None,
        })
    }
}
struct Video {
    src: String,
    player: Option<Player>,
    playing: bool,
    looping: bool,
    reported: String,
    seek: Option<f64>,
}
impl CustomElement for Video {
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        window: &mut gpui::Window,
        _: &mut gpui::Context<crate::renderer::GpuixView>,
    ) -> gpui::AnyElement {
        let mut root = super::custom_surface(
            gpui::div()
                .id(gpui::SharedString::from(format!("video-{}", ctx.id)))
                .overflow_hidden(),
            &ctx,
        );
        if self.player.is_none() && !self.src.is_empty() {
            self.player = unsafe { Player::new(&self.src) };
        }
        let mut status = "loading";
        let mut message = String::new();
        let mut duration = 0.0;
        let mut current = 0.0;
        let mut width = 0;
        let mut height = 0;
        if let Some(player) = self.player.as_mut() {
            unsafe {
                let state: i64 = msg_send![player.item, status];
                if state == 2 {
                    status = "error";
                    let error: *mut Object = msg_send![player.item, error];
                    let desc: *mut Object = msg_send![error, localizedDescription];
                    let bytes: *const std::os::raw::c_char = msg_send![desc, UTF8String];
                    if !bytes.is_null() {
                        message = std::ffi::CStr::from_ptr(bytes)
                            .to_string_lossy()
                            .to_string();
                    }
                } else {
                    let time: CMTime = msg_send![player.player, currentTime];
                    let length: CMTime = msg_send![player.item, duration];
                    current = CMTimeGetSeconds(time);
                    duration = CMTimeGetSeconds(length);
                    if !current.is_finite() {
                        current = 0.0;
                    }
                    if !duration.is_finite() {
                        duration = 0.0;
                    }
                    if let Some(seconds) = self.seek.take() {
                        let seek = CMTime {
                            value: (seconds.max(0.0) * 600.0) as i64,
                            timescale: 600,
                            flags: 1,
                            epoch: 0,
                        };
                        let _: () = msg_send![player.player,seekToTime:seek];
                    }
                    if self.looping && duration > 0.0 && current >= duration - 0.015 {
                        let _: () = msg_send![player.player,seekToTime:ZERO];
                    }
                    let rate: f32 = msg_send![player.player, rate];
                    if self.playing && rate == 0.0 {
                        let _: () = msg_send![player.player, play];
                    } else if !self.playing && rate != 0.0 {
                        let _: () = msg_send![player.player, pause];
                    }
                    let has: bool = msg_send![player.output,hasNewPixelBufferForItemTime:time];
                    if has {
                        let buffer: CVPixelBufferRef = msg_send![player.output,copyPixelBufferForItemTime:time itemTimeForDisplay:ptr::null_mut::<CMTime>()];
                        if !buffer.is_null() {
                            player.last = Some(CVPixelBuffer::wrap_under_create_rule(buffer));
                        }
                    }
                    if let Some(frame) = &player.last {
                        status = if self.playing { "playing" } else { "paused" };
                        width = frame.get_width();
                        height = frame.get_height();
                        root = root.child(
                            gpui::surface(frame.clone())
                                .object_fit(gpui::ObjectFit::Cover)
                                .size_full(),
                        );
                    }
                    if self.playing || player.last.is_none() {
                        window.request_animation_frame();
                    }
                }
            }
        } else {
            status = "error";
            message = "No video source is available".into();
        }
        if status == "error" {
            root = root.child(ctx.chrome_text("Video unavailable", None));
        }
        let report = format!("{status}:{width}:{height}:{message}");
        if report != self.reported {
            self.reported = report;
            crate::renderer::emit_event_full(ctx.event_callback, ctx.id, "change", |p| {
                p.value=Some(serde_json::json!({"status":status,"width":width,"height":height,"duration":duration,"time":current,"error":message}).to_string())
            });
        }
        root.into_any_element()
    }
    fn set_prop(&mut self, key: &str, value: Value) {
        match key {
            "src" => {
                let src = value.as_str().unwrap_or_default();
                if src != self.src {
                    self.src = src.into();
                    self.player = None;
                    self.reported.clear();
                }
            }
            "playing" => self.playing = value.as_bool().unwrap_or(true),
            "loop" => self.looping = value.as_bool().unwrap_or(true),
            "seek" => self.seek = value.as_f64().filter(|v| v.is_finite()),
            _ => {}
        }
    }
    fn supported_props(&self) -> &'static [&'static str] {
        &["src", "playing", "loop", "seek"]
    }
    fn supported_events(&self) -> &'static [&'static str] {
        &["change"]
    }
    fn destroy(&mut self) {
        self.player = None;
    }
}
