slint::include_modules!();

use anyhow::{Result, anyhow, bail};
use std::cell::RefCell;
use std::rc::Rc;
use vello::kurbo::{Affine, Circle, Ellipse, Line, RoundedRect, Stroke};
use vello::peniko::color::palette;
use vello::peniko::{Color, Fill};
use vello::util::{RenderContext, block_on_wgpu};
use vello::wgpu::{
    self, BufferDescriptor, BufferUsages, CommandEncoderDescriptor, Extent3d, TexelCopyBufferInfo,
    TexelCopyBufferLayout, TextureDescriptor, TextureFormat, TextureUsages,
};
use vello::{AaConfig, RenderParams, Renderer, RendererOptions, Scene};

struct VelloRenderer {
    context: RenderContext,
    scene: Scene,
    device_id: usize,
    renderer: Renderer,
    render_texture: wgpu::Texture,
    width: u32,
    height: u32,
}

impl VelloRenderer {
    async fn new(width: u32, height: u32) -> Result<Self> {
        let mut context = RenderContext::new();
        let device_id = context
            .device(None)
            .await
            .ok_or_else(|| anyhow!("No compatible device found"))?;
        let device = &context.devices[device_id].device;
        let renderer =
            Renderer::new(device, RendererOptions::default()).expect("Couldn't create renderer");
        let render_texture = Self::create_render_texture(device, width, height);
        Ok(Self {
            context,
            scene: Scene::new(),
            device_id,
            renderer,
            render_texture,
            width,
            height,
        })
    }
    fn create_render_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
        device.create_texture(&TextureDescriptor {
            label: Some("Vello Render Texture"),
            size: Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::RENDER_ATTACHMENT
                | TextureUsages::STORAGE_BINDING
                | TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    }
    fn resize(&mut self, width: u32, height: u32) {
        if self.width == width && self.height == height {
            return;
        }
        self.width = width;
        self.height = height;
        let device = &self.context.devices[self.device_id].device;
        self.render_texture = Self::create_render_texture(device, width, height);
    }
    fn render(&mut self) -> Result<()> {
        let device_handle = &self.context.devices[self.device_id];
        let device = &device_handle.device;
        let queue = &device_handle.queue;
        let view = self
            .render_texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let render_params = RenderParams {
            base_color: palette::css::DARK_SLATE_BLUE,
            width: self.width,
            height: self.height,
            antialiasing_method: AaConfig::Area,
        };
        self.renderer
            .render_to_texture(device, queue, &self.scene, &view, &render_params)
            .map_err(|e| anyhow!("Error rendering to texture: {}", e))
    }
    fn read_texture_data(&self) -> Result<Vec<u8>> {
        let device_handle = &self.context.devices[self.device_id];
        let device = &device_handle.device;
        let queue = &device_handle.queue;
        let padded_byte_width = (self.width * 4).next_multiple_of(256);
        let buffer_size = (padded_byte_width * self.height) as u64;
        let output_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("Vello Output Buffer"),
            size: buffer_size,
            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("Vello Copy Encoder"),
        });
        encoder.copy_texture_to_buffer(
            self.render_texture.as_image_copy(),
            TexelCopyBufferInfo {
                buffer: &output_buffer,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_byte_width),
                    rows_per_image: None,
                },
            },
            self.render_texture.size(),
        );
        queue.submit(Some(encoder.finish()));
        let buf_slice = output_buffer.slice(..);
        let (sender, receiver) = futures_intrusive::channel::shared::oneshot_channel();
        buf_slice.map_async(wgpu::MapMode::Read, move |v| sender.send(v).unwrap());
        if let Some(recv_result) = block_on_wgpu(device, receiver.receive()) {
            recv_result?;
        } else {
            bail!("GPU readback channel was closed.");
        }
        let data = buf_slice.get_mapped_range();
        let mut image_data = Vec::with_capacity((self.width * self.height * 4) as usize);
        for row in data.chunks(padded_byte_width as usize) {
            image_data.extend_from_slice(&row[..(self.width * 4) as usize]);
        }
        drop(data);
        output_buffer.unmap();
        Ok(image_data)
    }
}

fn add_shapes_to_scene(scene: &mut Scene) {
    let stroke = Stroke::new(6.0);
    let rect = RoundedRect::new(10.0, 10.0, 240.0, 240.0, 20.0);
    scene.stroke(
        &stroke,
        Affine::IDENTITY,
        Color::new([0.9804, 0.702, 0.5294, 1.0]),
        None,
        &rect,
    );
    let circle = Circle::new((420.0, 200.0), 120.0);
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::new([0.9529, 0.5451, 0.6588, 1.0]),
        None,
        &circle,
    );
    let ellipse = Ellipse::new((250.0, 420.0), (100.0, 160.0), -90.0);
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::new([0.7961, 0.651, 0.9686, 1.0]),
        None,
        &ellipse,
    );
    let line = Line::new((260.0, 20.0), (620.0, 100.0));
    scene.stroke(
        &stroke,
        Affine::IDENTITY,
        Color::new([0.5373, 0.7059, 0.9804, 1.0]),
        None,
        &line,
    );
}
use slint::wgpu_26::{WGPUConfiguration, WGPUSettings};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut wgpu_settings = WGPUSettings::default();
    wgpu_settings.device_required_features = slint::wgpu_26::wgpu::Features::PUSH_CONSTANTS;
    wgpu_settings.device_required_limits.max_push_constant_size = 16;

    slint::BackendSelector::new()
        .require_wgpu_26(WGPUConfiguration::Automatic(wgpu_settings))
        .select()
        .expect("Unable to create Slint backend with WGPU based renderer");

    let app_window = AppWindow::new()?;
    let app_weak = app_window.as_weak();

    let mut width = app_window.get_requested_texture_width() as u32;
    let mut height = app_window.get_requested_texture_height() as u32;
    if width == 0 || height == 0 {
        width = 1;
        height = 1;
    }

    let vello_renderer = Rc::new(RefCell::new(VelloRenderer::new(width, height).await?));

    let mut frame_count = 0;
    let mut last_sec = std::time::Instant::now();

    {
        let mut renderer = vello_renderer.borrow_mut();
        renderer.scene.reset();
        add_shapes_to_scene(&mut renderer.scene);
    }

    app_window
        .window()
        .set_rendering_notifier(move |state, _| match state {
            slint::RenderingState::BeforeRendering => {
                if let Some(app) = app_weak.upgrade() {
                    let mut renderer = vello_renderer.borrow_mut();

                    let new_width = app.get_requested_texture_width() as u32;
                    let new_height = app.get_requested_texture_height() as u32;

                    if new_width > 0 && new_height > 0 {
                        renderer.resize(new_width, new_height);
                    } else {
                        return;
                    }

                    renderer.render().expect("Vello rendering failed");

                    // Temporary hack texture GPU -> CPU -> GPU
                    // use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
                    // let image_data = renderer.read_texture_data().unwrap();
                    // let mut pixel_buffer =
                    //     SharedPixelBuffer::<Rgba8Pixel>::new(renderer.width, renderer.height);
                    // let source_slice: &[Rgba8Pixel] = bytemuck::cast_slice(&image_data);
                    // pixel_buffer.make_mut_slice().copy_from_slice(source_slice);
                    // let slint_image = Image::from_rgba8(pixel_buffer);

                    let texture = &renderer.render_texture;
                    let slint_image = slint::Image::try_from(texture).unwrap();

                    app.set_texture(slint_image);
                    app.window().request_redraw();
                }

                frame_count += 1;
                let now = std::time::Instant::now();
                if now.duration_since(last_sec).as_secs() >= 1 {
                    println!("FPS: {}", frame_count);
                    frame_count = 0;
                    last_sec = now;
                }
            }
            _ => {}
        })?;

    app_window.window().request_redraw();

    app_window.run()?;

    Ok(())
}
