slint::include_modules!();

use anyhow::{Result, anyhow};
use slint::wgpu_27::{WGPUConfiguration, WGPUSettings, wgpu};
use std::time::Instant;
use vello::kurbo::{Affine, Arc, Circle, Point, Stroke};
use vello::peniko::{Color, Fill};
use vello::{AaConfig, RenderParams, Renderer, RendererOptions, Scene};

struct VelloRenderer {
    scene: Scene,
    renderer: Renderer,
    render_textures: [vello::wgpu::Texture; 2],
    current_idx: usize,
    width: u32,
    height: u32,
}

impl VelloRenderer {
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, width: u32, height: u32) -> Result<Self> {
        Ok(Self {
            scene: Scene::new(),
            renderer: Renderer::new(device, RendererOptions::default()).expect("Couldn't create renderer"),
            render_textures: [Self::create_render_texture(device, width, height), Self::create_render_texture(device, width, height)],
            current_idx: 0,
            width,
            height,
        })
    }

    fn create_render_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Vello Render Texture"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
    }

    fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if self.width == width && self.height == height {
            return;
        }
        self.width = width;
        self.height = height;
        self.render_textures = [Self::create_render_texture(device, width, height), Self::create_render_texture(device, width, height)];
    }

    fn render(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> Result<wgpu::Texture> {
        self.current_idx = 1 - self.current_idx;

        let texture_to_render_to = &self.render_textures[self.current_idx];
        let view = texture_to_render_to.create_view(&wgpu::TextureViewDescriptor::default());

        let render_params =
            RenderParams { base_color: Color::from_rgb8(20, 25, 40), width: texture_to_render_to.size().width, height: texture_to_render_to.size().height, antialiasing_method: AaConfig::Area };

        self.renderer.render_to_texture(device, queue, &self.scene, &view, &render_params).map_err(|e| anyhow!("Error rendering to texture: {}", e))?;

        Ok(texture_to_render_to.clone())
    }
}

fn update_scene(scene: &mut Scene, time: f64, width: u32, height: u32) {
    if width == 0 || height == 0 {
        return;
    }

    const SLINT_BLUE: Color = Color::new([0.137, 0.475, 0.957, 1.0]);
    const GRID_WHITE: Color = Color::new([0.94, 0.94, 0.94, 0.5]);

    let center = Point::new(width as f64 / 2.0, height as f64 / 2.0);
    let stroke_width = 8.0;

    let grid_spacing = 40.0;
    let grid_range_x = (width as f64 / grid_spacing / 2.0).ceil() as i32 + 2;
    let grid_range_y = (height as f64 / grid_spacing / 2.0).ceil() as i32 + 2;
    for i in -grid_range_x..=grid_range_x {
        for j in -grid_range_y..=grid_range_y {
            let pos = center + (i as f64 * grid_spacing, j as f64 * grid_spacing);
            let dist = pos.distance(center);
            let ripple = (dist * 0.05 - time * 2.0).sin();
            let radius = (ripple * 1.5).max(0.0);
            if radius > 0.1 {
                scene.fill(Fill::NonZero, Affine::IDENTITY, GRID_WHITE, None, &Circle::new(pos, radius));
            }
        }
    }

    let arc_sweep_angle = std::f64::consts::PI * 1.5;

    let radius1 = 60.0 + (time * 2.0).sin() * 5.0;
    let arc1 = Arc::new(Point::ZERO, (radius1, radius1), 0.0, arc_sweep_angle, 0.0);
    let transform1 = Affine::translate(center.to_vec2()) * Affine::rotate(time * 1.2);
    scene.stroke(&Stroke::new(stroke_width), transform1, SLINT_BLUE, None, &arc1);

    let radius2 = 90.0 + (time * 2.0 + 1.0).sin() * 8.0;
    let arc2 = Arc::new(Point::ZERO, (radius2, radius2), 0.0, arc_sweep_angle, 0.0);
    let transform2 = Affine::translate(center.to_vec2()) * Affine::rotate(-time * 0.8);
    scene.stroke(&Stroke::new(stroke_width), transform2, SLINT_BLUE.with_alpha(0.7), None, &arc2);

    let radius3 = 120.0 + (time * 2.0 + 2.0).sin() * 10.0;
    let arc3 = Arc::new(Point::ZERO, (radius3, radius3), 0.0, arc_sweep_angle, 0.0);
    let transform3 = Affine::translate(center.to_vec2()) * Affine::rotate(time * 0.5);
    scene.stroke(&Stroke::new(stroke_width), transform3, SLINT_BLUE.with_alpha(0.4), None, &arc3);
}

fn main() -> anyhow::Result<()> {
    let mut wgpu_settings = WGPUSettings::default();
    wgpu_settings.device_required_features = slint::wgpu_27::wgpu::Features::PUSH_CONSTANTS | slint::wgpu_27::wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;
    wgpu_settings.device_required_limits = slint::wgpu_27::wgpu::Limits { max_push_constant_size: 128, ..Default::default() };
    wgpu_settings.power_preference = wgpu::PowerPreference::HighPerformance;
    wgpu_settings.device_required_limits.max_storage_buffers_per_shader_stage = 8;
    wgpu_settings.device_required_limits.max_compute_workgroup_size_x = 256;
    wgpu_settings.device_required_limits.max_compute_workgroup_size_y = 256;
    wgpu_settings.device_required_limits.max_compute_workgroup_size_z = 64;
    wgpu_settings.device_required_limits.max_compute_invocations_per_workgroup = 256;
    wgpu_settings.device_required_limits.max_storage_textures_per_shader_stage = 4;
    wgpu_settings.device_required_limits.max_storage_buffer_binding_size = 134_217_728;

    slint::BackendSelector::new().require_wgpu_27(WGPUConfiguration::Automatic(wgpu_settings)).select().expect("Unable to create Slint backend with WGPU based renderer");

    let app_window = AppWindow::new()?;
    let app_weak = app_window.as_weak();

    let mut underlay: Option<VelloRenderer> = None;

    let start_time = Instant::now();

    app_window.window().set_rendering_notifier(move |state, graphics_api| match state {
        slint::RenderingState::RenderingSetup => {
            let slint::GraphicsAPI::WGPU27 { device, queue, .. } = graphics_api else {
                return;
            };
            let renderer = VelloRenderer::new(device, queue, 1, 1).unwrap();
            underlay = Some(renderer);
        }
        slint::RenderingState::BeforeRendering => {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let slint::GraphicsAPI::WGPU27 { device, queue, .. } = graphics_api else {
                return;
            };
            let Some(renderer) = &mut underlay else {
                return;
            };

            let new_width = app.get_requested_texture_width() as u32;
            let new_height = app.get_requested_texture_height() as u32;

            if new_width > 0 && new_height > 0 {
                renderer.resize(device, new_width, new_height);
            } else {
                return;
            }

            let time = start_time.elapsed().as_secs_f64();

            renderer.scene.reset();

            update_scene(&mut renderer.scene, time, new_width, new_height);

            let texture_for_slint = renderer.render(device, queue).expect("Vello rendering failed");

            let slint_image = slint::Image::try_from(texture_for_slint).expect("Failed to create Slint image from texture");

            app.set_texture(slint_image);
            app.window().request_redraw();
        }
        slint::RenderingState::RenderingTeardown => {
            underlay = None;
        }
        _ => {}
    })?;

    app_window.run()?;

    Ok(())
}
