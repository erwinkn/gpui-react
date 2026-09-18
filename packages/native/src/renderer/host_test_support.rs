//! Keep feature-gated N-API methods in separately gated impl blocks.
//! napi-rs registers every method it sees in an impl, even a cfg-disabled one.
use super::*;

#[cfg(target_os = "macos")]
#[napi]
impl GpuixRenderer {
    /// Test support: resize the actual macOS content view through GPUI.
    #[napi]
    pub fn resize_window_for_test(&self, width: f64, height: f64) -> Result<()> {
        if !width.is_finite()
            || !height.is_finite()
            || width < 100.0
            || height < 100.0
            || width > 10000.0
            || height > 10000.0
        {
            return Err(Error::from_reason(
                "Test window dimensions must be between 100 and 10,000 pixels",
            ));
        }
        update_window_without_view(|window, _| {
            window.resize(gpui::size(gpui::px(width as f32), gpui::px(height as f32)))
        })
    }

    /// Test only: force native CPU draws while the real window stays in the background.
    /// This is not a display cadence or physical presentation measurement.
    #[napi]
    pub fn start_host_probe(
        &self,
        duration_ms: u32,
        element_id: f64,
        test_id: Option<String>,
    ) -> Result<()> {
        if duration_ms > 10_000 {
            return Err(Error::from_reason("Probe limit is 10000 ms"));
        }
        let id = to_element_id(element_id)?;
        let tree = self.tree.clone();
        crate::host_probe::start();
        GPUI_APP.with(|app| {
            let app = app.borrow();
            let app = app.as_ref().ok_or_else(|| Error::from_reason("Renderer not initialized"))?;
            let task = app.update(|cx| {
                cx.spawn(async move |cx| {
                    let start = std::time::Instant::now();
                    while start.elapsed() < Duration::from_millis(duration_ms as u64) {
                        cx.background_executor().timer(Duration::from_millis(16)).await;
                        if update_window_without_view(|window, cx| window.draw(cx).clear(cx)).is_err() { break; }
                        let target = test_id.as_ref().and_then(|test_id| {
                            tree.lock().unwrap().elements.values().find(|element| element.test_id.as_ref() == Some(test_id)).map(|element| element.id)
                        }).unwrap_or(id);
                        let bounds = crate::automation::get_bounds(target);
                        crate::host_probe::record("frame", serde_json::json!({"bounds": bounds, "width": bounds.map(|b| b.width), "text": crate::text::painted_text()}));
                    }
                })
            });
            crate::host_probe::keep(task);
            Ok(())
        })
    }

    #[napi]
    pub fn mark_host_probe(&self, label: String) {
        crate::host_probe::record("mark", serde_json::json!(label));
    }

    #[napi]
    pub fn take_host_probe(&self) -> String {
        crate::host_probe::take()
    }
}

#[napi]
impl GpuixRenderer {
    /// Test support: enqueue mouse motion in this process's AppKit queue.
    /// This does not move the system pointer or post events to another app.
    #[napi]
    pub fn queue_app_kit_mouse_moves(
        &self,
        count: u32,
        x: f64,
        y: f64,
        delta_y: f64,
    ) -> Result<()> {
        if count > 10_000 {
            return Err(Error::from_reason(
                "At most 10,000 test events may be queued",
            ));
        }
        if !x.is_finite() || !y.is_finite() || !delta_y.is_finite() {
            return Err(Error::from_reason(
                "Test pointer coordinates must be finite",
            ));
        }
        #[cfg(target_os = "macos")]
        {
            return MAC_PLATFORM.with(|platform| {
                let platform = platform.borrow();
                let platform = platform
                    .as_ref()
                    .ok_or_else(|| Error::from_reason("Renderer not initialized"))?;
                platform.queue_test_mouse_moves(count, x, y, delta_y);
                Ok(())
            });
        }
        #[cfg(not(target_os = "macos"))]
        Err(Error::from_reason("AppKit event testing requires macOS"))
    }
}
