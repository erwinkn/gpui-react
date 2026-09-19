//! An external GPUI component. The React binding only maps typed capabilities.
use gpui_react::{
    gpui::{prelude::*, *},
    *,
};
use gpui_react_controls::{Style, document_text, geometry::Rect};
use serde::{Deserialize, Deserializer, Serialize};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

fn color<'de, D: Deserializer<'de>>(d: D) -> Result<[f64; 4], D::Error> {
    let value = <[f64; 4]>::deserialize(d)?;
    if !value.iter().all(|v| v.is_finite() && v.abs() <= 65504.) || !(0.0..=1.0).contains(&value[3])
    {
        return Err(serde::de::Error::custom(
            "color must fit RGBA16Float and alpha must be in 0..1",
        ));
    }
    Ok(value)
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct TextureProps {
    pub style: Style,
    #[serde(deserialize_with = "color")]
    pub initial_color: [f64; 4],
    pub radius: f32,
    pub label: String,
    pub reduced_motion: bool,
}
#[derive(Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum TextureCommand {
    Transition {
        #[serde(deserialize_with = "color")]
        to: [f64; 4],
        duration_ms: u32,
    },
    Cancel,
}
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TextureEvent {
    Click { generation: u64 },
    Error { message: String },
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TexturePaint {
    pub bounds: Rect,
    pub size: [u32; 2],
    pub color: [f64; 4],
    pub generation: u64,
    pub frame: Option<FrameInfo>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureSnapshot {
    pub color: [f64; 4],
    pub animating: bool,
    pub painted: Option<TexturePaint>,
    pub error: Option<String>,
}
struct Motion {
    from: [f64; 4],
    to: [f64; 4],
    start: Instant,
    duration: Duration,
}
impl Motion {
    fn sample(&self, now: Instant) -> ([f64; 4], bool) {
        let t = (now.saturating_duration_since(self.start).as_secs_f64()
            / self.duration.as_secs_f64())
        .min(1.);
        (
            std::array::from_fn(|i| self.from[i] + (self.to[i] - self.from[i]) * t),
            t < 1.,
        )
    }
}
struct Texture {
    size: [u32; 2],
    color: [f64; 4],
    handle: GpuTextureHandle,
}

pub struct TextureView {
    style: Style,
    radius: f32,
    label: SharedString,
    label_key: SharedString,
    reduced_motion: bool,
    color: [f64; 4],
    motion: Option<Motion>,
    texture: Option<Texture>,
    generation: u64,
    painted: Option<TexturePaint>,
    error: Option<String>,
}
impl TextureView {
    pub fn new(props: TextureProps, cx: &Context<Self>) -> Self {
        Self {
            style: props.style,
            radius: props.radius,
            label: props.label.into(),
            label_key: format!("texture:{}", cx.entity_id().as_u64()).into(),
            reduced_motion: props.reduced_motion,
            color: props.initial_color,
            motion: None,
            texture: None,
            generation: 0,
            painted: None,
            error: None,
        }
    }
    pub fn update_props(&mut self, props: TextureProps, cx: &mut Context<Self>) {
        self.style = props.style;
        self.radius = props.radius;
        self.label = props.label.into();
        self.reduced_motion = props.reduced_motion;
        if self.reduced_motion
            && let Some(motion) = self.motion.take()
        {
            self.color = motion.to;
        }
        cx.notify();
    }
    fn current_color(&self, now: Instant) -> ([f64; 4], bool) {
        self.motion
            .as_ref()
            .map_or((self.color, false), |motion| motion.sample(now))
    }
    pub fn apply_command(&mut self, command: TextureCommand, cx: &mut Context<Self>) {
        let now = cx.background_executor().now();
        self.color = self.current_color(now).0;
        self.motion = None;
        if let TextureCommand::Transition { to, duration_ms } = command {
            if self.reduced_motion || cx.reduce_motion() || duration_ms == 0 {
                self.color = to;
            } else {
                self.motion = Some(Motion {
                    from: self.color,
                    to,
                    start: now,
                    duration: Duration::from_millis(duration_ms.into()),
                });
            }
        }
        cx.notify();
    }
    pub fn snapshot(&self, now: Instant) -> TextureSnapshot {
        let (color, animating) = self.current_color(now);
        TextureSnapshot {
            color,
            animating,
            painted: self.painted.clone(),
            error: self.error.clone(),
        }
    }
    /// Native callers can share the current resource. GPUI frames retain it too.
    pub fn texture(&self) -> Option<&GpuTextureHandle> {
        self.texture.as_ref().map(|texture| &texture.handle)
    }
    fn paint_texture(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        let scale = window.scale_factor();
        let size = [
            (f32::from(bounds.size.width) * scale).ceil() as u32,
            (f32::from(bounds.size.height) * scale).ceil() as u32,
        ];
        if size.contains(&0) {
            return Ok(());
        }
        if !self
            .texture
            .as_ref()
            .is_some_and(|old| old.size == size && old.color == self.color)
        {
            // New contents get a new resource: already recorded frames retain
            // their own pixels, even after resize, cancellation, or removal.
            self.texture = Some(Texture {
                size,
                color: self.color,
                handle: produce(window, size, self.color)?,
            });
            self.generation += 1;
        }
        window.paint_gpu_texture(
            bounds,
            self.texture.as_ref().unwrap().handle.clone(),
            Corners::all(px(self.radius)),
        )?;
        self.painted = Some(TexturePaint {
            bounds: bounds.into(),
            size,
            color: self.color,
            generation: self.generation,
            frame: current_frame(window, cx),
        });
        Ok(())
    }
}
impl Render for TextureView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if cx.reduce_motion()
            && let Some(motion) = self.motion.take()
        {
            self.color = motion.to;
        }
        let (color, active) = self.current_color(cx.background_executor().now());
        self.color = color;
        if active {
            window.request_animation_frame();
        } else {
            self.motion = None;
        }
        let owner = cx.weak_entity();
        self.style
            .apply_interactive(div().id("texture").relative())
            .on_click(cx.listener(|view, _, _, cx| {
                cx.emit(TextureEvent::Click {
                    generation: view.generation,
                })
            }))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, cx| {
                        owner
                            .update(cx, |view, cx| {
                                match view.paint_texture(bounds, window, cx) {
                                    Ok(()) => view.error = None,
                                    Err(error) => {
                                        let message = error.to_string();
                                        if view.error.as_ref() != Some(&message) {
                                            cx.emit(TextureEvent::Error {
                                                message: message.clone(),
                                            });
                                        }
                                        view.error = Some(message);
                                        view.motion = None;
                                    }
                                }
                            })
                            .ok();
                    },
                )
                .absolute()
                .size_full(),
            )
            .when(!self.label.is_empty(), |el| {
                el.child(document_text(self.label_key.clone(), self.label.clone()))
            })
    }
}
impl EventEmitter<TextureEvent> for TextureView {}
impl ReactView for TextureView {
    type Props = TextureProps;
    fn create(props: Self::Props, _: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new(props, cx)
    }
    fn set_props(&mut self, props: Self::Props, _: &mut Window, cx: &mut Context<Self>) {
        self.update_props(props, cx);
    }
    fn unmounting(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.motion = None;
        self.texture = None;
    }
}
impl ReactEvents for TextureView {
    type Event = TextureEvent;
}
impl ReactCommands for TextureView {
    type Command = TextureCommand;
    fn command(
        &mut self,
        command: Self::Command,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        self.apply_command(command, cx);
        Ok(())
    }
}
impl ReactQueries for TextureView {
    type Query = ();
    type Reply = TextureSnapshot;
    fn query(
        &mut self,
        _: (),
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<Self::Reply> {
        Ok(self.snapshot(cx.background_executor().now()))
    }
}
pub fn register(registry: &mut Registry) -> anyhow::Result<()> {
    registry.register(
        Component::<TextureView>::new("example-texture")
            .events()
            .commands()
            .queries(),
    )
}

#[cfg(target_os = "macos")]
fn produce(window: &Window, size: [u32; 2], color: [f64; 4]) -> anyhow::Result<GpuTextureHandle> {
    anyhow::ensure!(window.gpu_device_lost() != Some(true), "GPU device lost");
    let context = window
        .gpu_context()
        .ok_or_else(|| anyhow::anyhow!("no GPU context"))?;
    let (device, queue) = *context
        .downcast::<(metal::Device, metal::CommandQueue)>()
        .map_err(|_| anyhow::anyhow!("expected Metal context"))?;
    let descriptor = metal::TextureDescriptor::new();
    descriptor.set_width(size[0].into());
    descriptor.set_height(size[1].into());
    descriptor.set_pixel_format(metal::MTLPixelFormat::RGBA16Float);
    descriptor.set_storage_mode(metal::MTLStorageMode::Private);
    descriptor.set_usage(metal::MTLTextureUsage::ShaderRead | metal::MTLTextureUsage::RenderTarget);
    let texture = device.new_texture(&descriptor);
    let pass = metal::RenderPassDescriptor::new();
    let attachment = pass.color_attachments().object_at(0).unwrap();
    attachment.set_texture(Some(&texture));
    attachment.set_load_action(metal::MTLLoadAction::Clear);
    attachment.set_store_action(metal::MTLStoreAction::Store);
    attachment.set_clear_color(metal::MTLClearColor::new(
        color[0], color[1], color[2], color[3],
    ));
    let commands = queue.new_command_buffer();
    commands.new_render_command_encoder(pass).end_encoding();
    commands.commit();
    Ok(Arc::new(texture))
}
#[cfg(not(target_os = "macos"))]
fn produce(_: &Window, _: [u32; 2], _: [f64; 4]) -> anyhow::Result<GpuTextureHandle> {
    anyhow::bail!("this fixture requires Metal")
}

#[cfg(test)]
mod tests {
    use super::{TextureCommand, TextureProps};
    use serde_json::json;
    #[test]
    fn color_validation_preserves_hdr_and_rejects_invalid_texture_values() {
        let props: TextureProps =
            serde_json::from_value(json!({"initialColor":[2,0,0,0.25]})).unwrap();
        assert_eq!(props.initial_color, [2., 0., 0., 0.25]);
        for color in [
            json!([1, 2, 3, 2]),
            json!([70000, 0, 0, 1]),
            json!([1, 2, 3]),
        ] {
            assert!(serde_json::from_value::<TextureProps>(json!({"initialColor":color})).is_err());
            assert!(
                serde_json::from_value::<TextureCommand>(
                    json!({"type":"transition","to":color,"durationMs":20})
                )
                .is_err()
            );
        }
        assert!(
            serde_json::from_value::<TextureCommand>(
                json!({"type":"transition","to":[1,0,0,1],"durationMs":-1})
            )
            .is_err()
        );
    }
}
