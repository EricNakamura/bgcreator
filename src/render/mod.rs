use std::collections::HashMap;
use std::path::Path;

use anyhow::Context;
use bytemuck::{Pod, Zeroable};
use eframe::wgpu;

use crate::model::{EXPORT_HEIGHT, EXPORT_WIDTH, Preset, SceneId, TextConfig, TextMotion};
use crate::text::{ATLAS_HEIGHT, ATLAS_WIDTH, MAX_LINES};

// Cada frame passa por dois render passes:
//   1. scene pass: o shader da cena desenha em `scene_target`;
//   2. post pass: post.wgsl lê `scene_target`, aplica cor/nitidez e escreve
//      em `target`, que é o que aparece no preview e vai para PNG/MP4.
//
// `Uniforms` abaixo precisa ter exatamente o mesmo layout do struct
// `Uniforms` em shaders/common.wgsl. O teste `uniform_layout` protege o
// tamanho total; a ordem dos campos é o contrato real.
const COPY_ALIGN: u32 = 256;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Uniforms {
    pub resolution: [f32; 2],
    pub time: f32,
    pub seed: f32,
    // x = speed, y = scale, z = intensity, w = vignette
    pub params: [f32; 4],
    // x = center dim, y = gradient style
    pub params2: [f32; 4],
    // 4 barras: x = estilo, y = ativa, z = defasagem (0..1), w = espelhada
    pub bar_params: [[f32; 4]; 4],
    // 4 paletas de 4 cores, uma por barra
    pub bar_palettes: [[f32; 4]; 16],
    // x = barras ativas, y = gap, z = suavidade
    pub bars: [f32; 4],
    // x = ativo, y = mapa, z = força, w = escala
    pub warp: [f32; 4],
    // x = velocidade, y = seed
    pub warp2: [f32; 4],
    // x = ativa, y = offset horizontal, z = y da linha, w = altura da linha
    pub text_params: [[f32; 4]; 4],
    // cor rgba da linha; alpha já inclui opacidade/fade
    pub text_colors: [[f32; 4]; 4],
    // x = exposure, y = brightness, z = contrast, w = saturation
    pub post: [f32; 4],
    // x = temperature, y = tint, z = gamma, w = sharpness
    pub post2: [f32; 4],
}

const _: () = assert!(std::mem::size_of::<Uniforms>() == 576);

impl Uniforms {
    pub fn from_preset(preset: &Preset, time: f32, width: u32, height: u32) -> Self {
        Self {
            resolution: [width as f32, height as f32],
            time: time.rem_euclid(1.0),
            seed: preset.seed,
            params: [
                preset.speed,
                preset.scale,
                preset.intensity,
                preset.vignette,
            ],
            params2: [
                preset.center_dim,
                preset.gradient_style.index() as f32,
                0.0,
                0.0,
            ],
            bar_params: std::array::from_fn(|index| {
                let bar = &preset.bars[index];
                [
                    bar.style.index() as f32,
                    if bar.enabled { 1.0 } else { 0.0 },
                    bar.phase,
                    if bar.mirrored { 1.0 } else { 0.0 },
                ]
            }),
            bar_palettes: std::array::from_fn(|index| {
                let bar = index / 4;
                let color = index % 4;
                let palette =
                    if bar == 0 && (!preset.bars_enabled || preset.scene == SceneId::Shine) {
                        &preset.palette
                    } else {
                        &preset.bars[bar].palette
                    };
                palette.rgba()[color]
            }),
            bars: [
                if preset.bars_enabled { 1.0 } else { 0.0 },
                preset.bar_gap,
                preset.bar_softness,
                0.0,
            ],
            warp: [
                if preset.warp.enabled { 1.0 } else { 0.0 },
                preset.warp.map.index() as f32,
                preset.warp.strength,
                preset.warp.scale,
            ],
            warp2: [preset.warp.speed, preset.warp.seed, 0.0, 0.0],
            text_params: std::array::from_fn(|index| {
                let line = preset.text.lines.get(index);
                let Some(line) = line else {
                    return [0.0; 4];
                };
                let active = preset.text.enabled && line.is_active();
                let t = time.rem_euclid(1.0);
                let phase = std::f32::consts::TAU * t;
                let mut y = line.y;
                let mut height = line.size / EXPORT_HEIGHT as f32 * 2.0;
                let mut offset = 0.0;
                match line.motion {
                    TextMotion::Scroll => {
                        offset = t * line.speed.round().max(1.0) * line.direction.sign();
                    }
                    TextMotion::Fade => {}
                    TextMotion::Drift => y += 0.04 * phase.sin(),
                    TextMotion::Pulse => height *= 1.0 + 0.06 * phase.sin(),
                }
                [if active { 1.0 } else { 0.0 }, offset, y, height]
            }),
            text_colors: std::array::from_fn(|index| {
                let Some(line) = preset.text.lines.get(index) else {
                    return [1.0, 1.0, 1.0, 0.0];
                };
                let [r, g, b] = crate::model::hex_to_rgb(&line.color).unwrap_or([1.0, 1.0, 1.0]);
                let t = time.rem_euclid(1.0);
                let phase = std::f32::consts::TAU * t;
                let alpha = if preset.text.enabled && line.is_active() {
                    let mut alpha = line.opacity.clamp(0.0, 1.0);
                    if line.motion == TextMotion::Fade {
                        alpha *= 0.5 - 0.5 * phase.cos();
                    }
                    alpha
                } else {
                    0.0
                };
                [r, g, b, alpha]
            }),
            post: [
                preset.grade.exposure,
                preset.grade.brightness,
                preset.grade.contrast,
                preset.grade.saturation,
            ],
            post2: [
                preset.grade.temperature,
                preset.grade.tint,
                preset.grade.gamma,
                preset.grade.sharpness,
            ],
        }
    }
}

fn shader_source(scene: SceneId) -> String {
    let scene_source = match scene {
        SceneId::Shine => include_str!("../../shaders/shine.wgsl"),
        SceneId::Gradient => include_str!("../../shaders/gradient.wgsl"),
    };
    format!(
        "{}\n{}",
        include_str!("../../shaders/common.wgsl"),
        scene_source
    )
}

fn post_shader_source() -> String {
    format!(
        "{}\n{}",
        include_str!("../../shaders/common.wgsl"),
        include_str!("../../shaders/post.wgsl"),
    )
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipelines: HashMap<SceneId, wgpu::RenderPipeline>,
    bind_group: wgpu::BindGroup,
    post_pipeline: wgpu::RenderPipeline,
    post_bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    _scene_target: wgpu::Texture,
    scene_view: wgpu::TextureView,
    text_texture: wgpu::Texture,
    _text_view: wgpu::TextureView,
    _text_sampler: wgpu::Sampler,
    target: wgpu::Texture,
    view: wgpu::TextureView,
    readback: wgpu::Buffer,
    width: u32,
    height: u32,
    padded_bytes_per_row: u32,
}

impl Renderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, width: u32, height: u32) -> Self {
        assert!(width > 0 && height > 0);

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("render uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let scene_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("scene uniforms"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            std::mem::size_of::<Uniforms>() as u64
                        ),
                    },
                    count: None,
                }],
            });

        let scene_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene uniforms"),
            layout: &scene_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let scene_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("scene pipeline layout"),
                bind_group_layouts: &[Some(&scene_bind_group_layout)],
                immediate_size: 0,
            });

        let mut pipelines = HashMap::new();
        for scene in SceneId::ALL {
            let source = shader_source(scene);
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(scene.slug()),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(scene.slug()),
                layout: Some(&scene_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });
            pipelines.insert(scene, pipeline);
        }

        let scene_target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("scene target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let scene_view = scene_target.create_view(&wgpu::TextureViewDescriptor::default());

        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("render target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("post sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let text_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("text atlas"),
            size: wgpu::Extent3d {
                width: ATLAS_WIDTH,
                height: ATLAS_HEIGHT,
                depth_or_array_layers: MAX_LINES as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let text_view = text_texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let text_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("text sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let post_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("post"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(
                                std::mem::size_of::<Uniforms>() as u64,
                            ),
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let post_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post"),
            layout: &post_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&scene_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&text_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&text_sampler),
                },
            ],
        });

        let post_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("post pipeline layout"),
            bind_group_layouts: &[Some(&post_bind_group_layout)],
            immediate_size: 0,
        });

        let post_source = post_shader_source();
        let post_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("post"),
            source: wgpu::ShaderSource::Wgsl(post_source.into()),
        });
        let post_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("post"),
            layout: Some(&post_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &post_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &post_shader,
                entry_point: Some("fs_post"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let padded_bytes_per_row = (width * 4).div_ceil(COPY_ALIGN) * COPY_ALIGN;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("render readback"),
            size: (padded_bytes_per_row as u64) * (height as u64),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let renderer = Self {
            device: device.clone(),
            queue: queue.clone(),
            pipelines,
            bind_group: scene_bind_group,
            post_pipeline,
            post_bind_group,
            uniform_buffer,
            _scene_target: scene_target,
            scene_view,
            text_texture,
            _text_view: text_view,
            _text_sampler: text_sampler,
            target,
            view,
            readback,
            width,
            height,
            padded_bytes_per_row,
        };
        renderer.update_text(&TextConfig::default());
        renderer
    }

    /// Regrava o atlas de texto. Só é necessário quando texto/tamanho/fonte
    /// mudam; cor, opacidade e movimento vão pelos uniforms.
    pub fn update_text(&self, config: &TextConfig) {
        let data = crate::text::rasterize(config);
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.text_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ATLAS_WIDTH * 4),
                rows_per_image: Some(ATLAS_HEIGHT),
            },
            wgpu::Extent3d {
                width: ATLAS_WIDTH,
                height: ATLAS_HEIGHT,
                depth_or_array_layers: MAX_LINES as u32,
            },
        );
    }

    pub fn from_render_state(
        state: &eframe::egui_wgpu::RenderState,
        width: u32,
        height: u32,
    ) -> (Self, eframe::egui::TextureId) {
        let renderer = Self::new(&state.device, &state.queue, width, height);
        let texture_id = state.renderer.write().register_native_texture(
            &state.device,
            &renderer.view,
            wgpu::FilterMode::Linear,
        );
        (renderer, texture_id)
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn render(&self, scene: SceneId, uniforms: &Uniforms) {
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(uniforms));

        let pipeline = self
            .pipelines
            .get(&scene)
            .unwrap_or_else(|| panic!("pipeline ausente para {}", scene.label()));
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render encoder"),
            });

        {
            // Primeiro passe: desenha a cena na textura intermediária.
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.scene_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        {
            // Segundo passe: lê a cena, aplica a correção de cor e grava na
            // textura final usada pelo preview/export.
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("post pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.post_pipeline);
            pass.set_bind_group(0, &self.post_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        self.queue.submit(Some(encoder.finish()));
    }

    pub fn read_pixels(&self) -> anyhow::Result<Vec<u8>> {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("readback encoder"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.padded_bytes_per_row),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));

        let slice = self.readback.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .context("aguardando a GPU")?;
        receiver
            .recv()
            .context("readback cancelado")?
            .context("mapeando readback")?;

        let data = slice.get_mapped_range().context("lendo readback")?;
        let mut pixels = Vec::with_capacity((self.width * self.height * 4) as usize);
        for row in 0..self.height {
            let start = (row * self.padded_bytes_per_row) as usize;
            let end = start + (self.width * 4) as usize;
            pixels.extend_from_slice(&data[start..end]);
        }
        drop(data);
        self.readback.unmap();
        Ok(pixels)
    }

    pub fn export_png(&self, preset: &Preset, time: f32, path: &Path) -> anyhow::Result<()> {
        let renderer = Self::new(&self.device, &self.queue, EXPORT_WIDTH, EXPORT_HEIGHT);
        renderer.update_text(&preset.text);
        let uniforms = Uniforms::from_preset(preset, time, EXPORT_WIDTH, EXPORT_HEIGHT);
        renderer.render(preset.scene, &uniforms);
        let pixels = renderer.read_pixels()?;
        crate::encode::write_png(path, EXPORT_WIDTH, EXPORT_HEIGHT, &pixels)
    }

    pub fn export_mp4_async(
        &self,
        preset: Preset,
        path: std::path::PathBuf,
    ) -> std::sync::mpsc::Receiver<crate::encode::mp4::Mp4Progress> {
        crate::encode::mp4::export_async(self.device.clone(), self.queue.clone(), preset, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{GradientStyle, Palette, WarpMap};

    fn test_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            ..Default::default()
        }))
        .expect("adaptador wgpu");
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("test device"),
            ..Default::default()
        }))
        .expect("device wgpu")
    }

    #[test]
    fn uniform_layout() {
        assert_eq!(std::mem::size_of::<Uniforms>(), 576);
    }

    #[test]
    fn time_is_wrapped() {
        let preset = Preset::shine();
        assert_eq!(Uniforms::from_preset(&preset, 1.25, 10, 10).time, 0.25);
        assert_eq!(Uniforms::from_preset(&preset, -0.25, 10, 10).time, 0.75);
    }

    #[test]
    #[ignore = "requer GPU"]
    fn render_scene_pngs() {
        let (device, queue) = test_device();
        let renderer = Renderer::new(&device, &queue, EXPORT_WIDTH, EXPORT_HEIGHT);
        std::fs::create_dir_all("target").unwrap();

        let shine = Preset::shine();
        let uniforms = Uniforms::from_preset(&shine, 0.35, EXPORT_WIDTH, EXPORT_HEIGHT);
        renderer.render(SceneId::Shine, &uniforms);
        let pixels = renderer.read_pixels().expect("readback shine");
        crate::encode::write_png(
            Path::new("target/shine-test.png"),
            EXPORT_WIDTH,
            EXPORT_HEIGHT,
            &pixels,
        )
        .expect("png shine");

        for style in GradientStyle::ALL {
            let preset = Preset::gradient(style);
            let uniforms = Uniforms::from_preset(&preset, 0.35, EXPORT_WIDTH, EXPORT_HEIGHT);
            renderer.render(SceneId::Gradient, &uniforms);
            let pixels = renderer.read_pixels().expect("readback gradient");
            let path = format!("target/gradient-{}.png", style.slug());
            crate::encode::write_png(Path::new(&path), EXPORT_WIDTH, EXPORT_HEIGHT, &pixels)
                .expect("png gradient");
        }

        let mut graded = Preset::shine();
        graded.grade.exposure = 0.7;
        graded.grade.brightness = 0.1;
        graded.grade.contrast = 0.35;
        graded.grade.saturation = 1.6;
        graded.grade.temperature = 0.4;
        graded.grade.tint = -0.2;
        graded.grade.gamma = 1.2;
        graded.grade.sharpness = 0.6;
        let uniforms = Uniforms::from_preset(&graded, 0.35, EXPORT_WIDTH, EXPORT_HEIGHT);
        renderer.render(SceneId::Shine, &uniforms);
        let pixels = renderer.read_pixels().expect("readback grade");
        crate::encode::write_png(
            Path::new("target/grade-test.png"),
            EXPORT_WIDTH,
            EXPORT_HEIGHT,
            &pixels,
        )
        .expect("png grade");

        let mut bars = Preset::gradient(GradientStyle::Horizon);
        bars.bars_enabled = true;
        let uniforms = Uniforms::from_preset(&bars, 0.35, EXPORT_WIDTH, EXPORT_HEIGHT);
        renderer.render(SceneId::Gradient, &uniforms);
        let pixels = renderer.read_pixels().expect("readback bars");
        crate::encode::write_png(
            Path::new("target/bars-test.png"),
            EXPORT_WIDTH,
            EXPORT_HEIGHT,
            &pixels,
        )
        .expect("png bars");

        let mut warped = Preset::gradient(GradientStyle::Aurora);
        warped.warp.enabled = true;
        warped.warp.map = WarpMap::Swirl;
        warped.warp.strength = 0.06;
        warped.warp.scale = 1.5;
        let uniforms = Uniforms::from_preset(&warped, 0.35, EXPORT_WIDTH, EXPORT_HEIGHT);
        renderer.render(SceneId::Gradient, &uniforms);
        let pixels = renderer.read_pixels().expect("readback warp");
        crate::encode::write_png(
            Path::new("target/warp-test.png"),
            EXPORT_WIDTH,
            EXPORT_HEIGHT,
            &pixels,
        )
        .expect("png warp");

        let mut text = Preset::gradient(GradientStyle::Aurora);
        text.text.enabled = true;
        renderer.update_text(&text.text);
        let uniforms = Uniforms::from_preset(&text, 0.35, EXPORT_WIDTH, EXPORT_HEIGHT);
        renderer.render(SceneId::Gradient, &uniforms);
        let pixels = renderer.read_pixels().expect("readback text");
        crate::encode::write_png(
            Path::new("target/text-test.png"),
            EXPORT_WIDTH,
            EXPORT_HEIGHT,
            &pixels,
        )
        .expect("png text");

        let mut full = text.clone();
        full.bars_enabled = true;
        full.warp.enabled = true;
        full.warp.map = WarpMap::Fbm;
        full.warp.strength = 0.04;
        full.warp.scale = 1.2;
        let uniforms = Uniforms::from_preset(&full, 0.35, EXPORT_WIDTH, EXPORT_HEIGHT);
        renderer.render(SceneId::Gradient, &uniforms);
        let pixels = renderer.read_pixels().expect("readback full");
        crate::encode::write_png(
            Path::new("target/full-test.png"),
            EXPORT_WIDTH,
            EXPORT_HEIGHT,
            &pixels,
        )
        .expect("png full");
    }

    #[test]
    #[ignore = "requer GPU"]
    fn render_palette_samples() {
        let (device, queue) = test_device();
        let renderer = Renderer::new(&device, &queue, 480, 300);
        std::fs::create_dir_all("target/palettes").unwrap();

        for palette in Palette::all() {
            let mut preset = Preset::gradient(GradientStyle::Mesh);
            preset.palette = palette.clone();
            let uniforms = Uniforms::from_preset(&preset, 0.35, 480, 300);
            renderer.render(SceneId::Gradient, &uniforms);
            let pixels = renderer.read_pixels().expect("readback palette");
            let path = format!("target/palettes/{}.png", palette.name);
            crate::encode::write_png(Path::new(&path), 480, 300, &pixels).expect("png palette");
        }
    }
}
