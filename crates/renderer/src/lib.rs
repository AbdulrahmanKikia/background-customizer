//! Per-monitor GPU surface with on-demand presents.
//!
//! Static images draw once and then sleep. No frame is submitted while idle.

use std::num::NonZeroIsize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use decoder::RgbaImage;
use raw_window_handle::{
    HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle, Win32WindowHandle,
    WindowsDisplayHandle,
};
/// Counts GPU presents for idle verification.
#[derive(Debug, Default)]
pub struct PresentCounter {
    frames: AtomicU64,
}

impl PresentCounter {
    pub fn new() -> Self {
        Self {
            frames: AtomicU64::new(0),
        }
    }

    pub fn frames_submitted(&self) -> u64 {
        self.frames.load(Ordering::SeqCst)
    }

    fn record_present(&self) {
        self.frames.fetch_add(1, Ordering::SeqCst);
    }
}

/// On-demand still-image wallpaper bound to one native window.
pub struct StaticImageSurface {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    presents: Arc<PresentCounter>,
    _window: Arc<NativeWindow>,
}

impl StaticImageSurface {
    /// Attach a wgpu surface to an existing desktop HWND and configure it once.
    pub fn attach(
        hwnd: isize,
        width: u32,
        height: u32,
        presents: Arc<PresentCounter>,
    ) -> Result<Self, String> {
        if hwnd == 0 {
            return Err("wallpaper window handle is null".to_string());
        }
        if width == 0 || height == 0 {
            return Err("wallpaper surface size must be non-zero".to_string());
        }

        let window = Arc::new(NativeWindow { hwnd });
        let mut instance_desc =
            wgpu::InstanceDescriptor::new_with_display_handle(Box::new(NativeDisplay));
        instance_desc.backends = wgpu::Backends::PRIMARY;
        let instance = wgpu::Instance::new(instance_desc);

        let surface = unsafe {
            let target = wgpu::SurfaceTargetUnsafe::from_window(&window)
                .map_err(|error| format!("surface target failed: {error}"))?;
            instance
                .create_surface_unsafe(target)
                .map_err(|error| format!("create_surface_unsafe failed: {error}"))?
        };

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .map_err(|error| format!("no GPU adapter: {error}"))?;

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("background-customizer"),
            ..wgpu::DeviceDescriptor::default()
        }))
        .map_err(|error| format!("request_device failed: {error}"))?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|format| format.is_srgb())
            .or_else(|| caps.formats.first().copied())
            .ok_or_else(|| "surface has no texture formats".to_string())?;

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 1,
            alpha_mode: caps
                .alpha_modes
                .first()
                .copied()
                .unwrap_or(wgpu::CompositeAlphaMode::Opaque),
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("static-image"),
            source: wgpu::ShaderSource::Wgsl(STATIC_IMAGE_SHADER.into()),
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("static-image-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("static-image-layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("static-image-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("static-image-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..wgpu::SamplerDescriptor::default()
        });

        Ok(Self {
            device,
            queue,
            surface,
            config,
            pipeline,
            bind_group_layout,
            sampler,
            presents,
            _window: window,
        })
    }

    pub fn frames_submitted(&self) -> u64 {
        self.presents.frames_submitted()
    }

    /// Draw the still image once. Callers must not present again while idle.
    pub fn present_static(&mut self, image: &RgbaImage) -> Result<(), String> {
        let fitted = image.resized_to_cover(self.config.width, self.config.height)?;
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("wallpaper"),
            size: wgpu::Extent3d {
                width: fitted.width,
                height: fitted.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &fitted.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(fitted.width * 4),
                rows_per_image: Some(fitted.height),
            },
            wgpu::Extent3d {
                width: fitted.width,
                height: fitted.height,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("wallpaper-bg"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            other => {
                return Err(format!("get_current_texture failed: {other:?}"));
            }
        };
        let frame_view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("static-image-encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("static-image-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &frame_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        self.queue.present(frame);
        self.presents.record_present();
        // Keep the texture alive through the submit.
        let _ = texture;
        Ok(())
    }
}

/// Offline present sink used to prove static images submit only once.
pub struct CountingPresenter {
    presents: Arc<PresentCounter>,
    drawn: bool,
}

impl CountingPresenter {
    pub fn new(presents: Arc<PresentCounter>) -> Self {
        Self {
            presents,
            drawn: false,
        }
    }

    pub fn present_static_once(&mut self, _image: &RgbaImage) -> Result<(), String> {
        if self.drawn {
            return Err("static wallpaper must not present again while idle".to_string());
        }
        self.presents.record_present();
        self.drawn = true;
        Ok(())
    }
}

#[derive(Debug)]
struct NativeDisplay;

impl HasDisplayHandle for NativeDisplay {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        let handle = WindowsDisplayHandle::new();
        Ok(unsafe {
            raw_window_handle::DisplayHandle::borrow_raw(RawDisplayHandle::Windows(handle))
        })
    }
}

struct NativeWindow {
    hwnd: isize,
}

impl HasWindowHandle for NativeWindow {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        let hwnd =
            NonZeroIsize::new(self.hwnd).ok_or(raw_window_handle::HandleError::Unavailable)?;
        // DX12 does not require hinstance on the window handle.
        let handle = Win32WindowHandle::new(hwnd);
        // Safety: hwnd remains valid for the life of StaticImageSurface via `_window`.
        Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(RawWindowHandle::Win32(handle)) })
    }
}

impl HasDisplayHandle for NativeWindow {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        let handle = WindowsDisplayHandle::new();
        Ok(unsafe {
            raw_window_handle::DisplayHandle::borrow_raw(RawDisplayHandle::Windows(handle))
        })
    }
}

const STATIC_IMAGE_SHADER: &str = r#"
struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VsOut {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let pos = positions[index];
    var out: VsOut;
    out.clip_position = vec4<f32>(pos, 0.0, 1.0);
    out.uv = vec2<f32>(pos.x * 0.5 + 0.5, 0.5 - pos.y * 0.5);
    return out;
}

@group(0) @binding(0) var wallpaper: texture_2d<f32>;
@group(0) @binding(1) var wallpaper_sampler: sampler;

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(wallpaper, wallpaper_sampler, in.uv);
}
"#;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use decoder::RgbaImage;

    use super::{CountingPresenter, PresentCounter};

    #[test]
    fn static_presenter_submits_only_one_frame() {
        let presents = Arc::new(PresentCounter::new());
        let mut presenter = CountingPresenter::new(Arc::clone(&presents));
        let image = RgbaImage::from_rgba8(1, 1, vec![0, 114, 178, 255]).expect("image");
        presenter.present_static_once(&image).expect("first");
        assert_eq!(presents.frames_submitted(), 1);
        assert!(presenter.present_static_once(&image).is_err());
        assert_eq!(presents.frames_submitted(), 1);
    }
}
