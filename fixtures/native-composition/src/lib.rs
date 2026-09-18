//! A complete composition that uses only the public GPUiX extension API.
use gpui::{canvas, div, prelude::*, px, Corners, SharedString};
use gpuix_native::extension::*;
pub use gpuix_native::*;
use std::{cell::RefCell, rc::Rc, sync::Arc};

#[cfg(not(target_family = "wasm"))]
#[napi_derive::module_init]
fn initialize() {
    register_probe().expect("register framework texture probe");
}

#[cfg_attr(target_family = "wasm", wasm_bindgen::prelude::wasm_bindgen(js_name = registerProbe))]
pub fn register_probe() -> Result<(), String> {
    register_extension(NativeExtension {
        id: "gpuix.example",
        version: env!("CARGO_PKG_VERSION"),
        api_version: NATIVE_EXTENSION_API_VERSION,
        elements: &[NativeElementRegistration {
            name: "example-gpu-texture",
            factory: || Box::new(Factory),
        }],
    })
}

struct Factory;
impl NativeElementFactory for Factory {
    fn element_type(&self) -> &str {
        "example-gpu-texture"
    }
    fn create(&self, _: u64) -> Box<dyn NativeElement> {
        Box::new(Probe::default())
    }
}

struct CachedTexture {
    size: (u32, u32),
    color: [f64; 4],
    texture: gpui::GpuTextureHandle,
}

#[derive(Default)]
struct Probe {
    color: [f64; 4],
    radius: f32,
    label: String,
    cache: Rc<RefCell<Option<CachedTexture>>>,
}

impl NativeElement for Probe {
    fn render(
        &mut self,
        ctx: NativeRenderContext,
        _: &mut gpui::Window,
        _: &mut gpui::Context<NativeView>,
    ) -> gpui::AnyElement {
        let cache = self.cache.clone();
        let color = self.color;
        let radius = self.radius;
        let label = ctx.text(0, self.label.clone(), None);
        custom_surface(
            div().id(SharedString::from(format!("__gpuix_example_{}", ctx.id))),
            &ctx,
        )
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    let scale = window.scale_factor();
                    let size = (
                        (f32::from(bounds.size.width) * scale).ceil() as u32,
                        (f32::from(bounds.size.height) * scale).ceil() as u32,
                    );
                    if size.0 == 0 || size.1 == 0 {
                        return;
                    }
                    let mut cache = cache.borrow_mut();
                    if !cache
                        .as_ref()
                        .is_some_and(|old| old.size == size && old.color == color)
                    {
                        *cache = Some(CachedTexture {
                            size,
                            color,
                            texture: produce(window, size, color).expect("produce GPU texture"),
                        });
                    }
                    window
                        .paint_gpu_texture(
                            bounds,
                            cache.as_ref().unwrap().texture.clone(),
                            Corners::all(px(radius)),
                        )
                        .expect("paint GPU texture");
                },
            )
            .absolute()
            .size_full(),
        )
        .child(label)
        .into_any_element()
    }
    fn set_prop(&mut self, key: &str, value: serde_json::Value) {
        match key {
            "color" => self.color = serde_json::from_value(value).unwrap_or([0.; 4]),
            "radius" => self.radius = value.as_f64().unwrap_or(0.) as f32,
            "label" => self.label = value.as_str().unwrap_or("").to_owned(),
            _ => {}
        }
    }
    fn supported_props(&self) -> &'static [&'static str] {
        &["color", "radius", "label"]
    }
    fn supported_events(&self) -> &'static [&'static str] {
        &["click"]
    }
    fn destroy(&mut self) {
        self.cache.borrow_mut().take();
    }
}

#[cfg(target_os = "macos")]
fn produce(
    window: &gpui::Window,
    size: (u32, u32),
    color: [f64; 4],
) -> anyhow::Result<gpui::GpuTextureHandle> {
    let context = window
        .gpu_context()
        .ok_or_else(|| anyhow::anyhow!("no GPU context"))?;
    let (device, queue) = *context
        .downcast::<(metal::Device, metal::CommandQueue)>()
        .map_err(|_| anyhow::anyhow!("expected Metal context"))?;
    let descriptor = metal::TextureDescriptor::new();
    descriptor.set_width(size.0.into());
    descriptor.set_height(size.1.into());
    descriptor.set_pixel_format(metal::MTLPixelFormat::RGBA16Float);
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

#[cfg(target_family = "wasm")]
fn produce(
    window: &gpui::Window,
    size: (u32, u32),
    color: [f64; 4],
) -> anyhow::Result<gpui::GpuTextureHandle> {
    let context = window
        .gpu_context()
        .ok_or_else(|| anyhow::anyhow!("no GPU context"))?;
    let (device, queue) = *context
        .downcast::<(Arc<wgpu::Device>, Arc<wgpu::Queue>)>()
        .map_err(|_| anyhow::anyhow!("expected wgpu context"))?;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("extension example"),
        size: wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let mut commands = device.create_command_encoder(&Default::default());
    {
        let _pass = commands.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: color[0],
                        g: color[1],
                        b: color[2],
                        a: color[3],
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
    }
    queue.submit([commands.finish()]);
    Ok(Arc::new(texture))
}

#[cfg(not(any(target_os = "macos", target_family = "wasm")))]
fn produce(_: &gpui::Window, _: (u32, u32), _: [f64; 4]) -> anyhow::Result<gpui::GpuTextureHandle> {
    anyhow::bail!("This example tests macOS and browser compositions")
}
