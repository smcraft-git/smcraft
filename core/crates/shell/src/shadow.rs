//! The 2D styles' drop shadow (`mmk_host::frame::FieldShadow`): the play field's sprites, moved
//! by the shadow offset, are drawn into a coverage mask (the most opaque texel wins, so
//! overlapping sprites darken once), then the mask is laid over the backdrop in the shadow colour
//! before the play field itself is drawn.

use crate::render::Gpu;

const SHADER: &str = r#"
struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@location(0) v: vec4<f32>) -> VOut {
    var o: VOut;
    o.pos = vec4<f32>(v.xy, 0.0, 1.0);
    o.uv = v.zw;
    return o;
}

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

// coverage: the sprite's alpha
@fragment
fn fs_mask(i: VOut) -> @location(0) vec4<f32> {
    let a = textureSample(tex, samp, i.uv).a;
    if (a < 0.01) {
        discard;
    }
    return vec4<f32>(a, 0.0, 0.0, 1.0);
}

struct Shade {
    color: vec4<f32>,
};

@group(0) @binding(0) var mask: texture_2d<f32>;
@group(0) @binding(1) var<uniform> shade: Shade;

// one triangle over the viewport
@vertex
fn vs_full(@builtin(vertex_index) k: u32) -> @builtin(position) vec4<f32> {
    let x = f32((k << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(k & 2u) * 2.0 - 1.0;
    return vec4<f32>(x, y, 0.0, 1.0);
}

@fragment
fn fs_shade(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let m = textureLoad(mask, vec2<i32>(pos.xy), 0).r;
    if (m <= 0.0) {
        discard;
    }
    return vec4<f32>(shade.color.rgb, shade.color.a * m);
}
"#;

const MASK: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

pub struct ShadowPass {
    mask_pipeline: wgpu::RenderPipeline,
    shade_pipeline: wgpu::RenderPipeline,
    shade_layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    /// The mask (target-sized) and its bind group for the shading draw.
    mask: Option<(wgpu::Texture, wgpu::TextureView, wgpu::BindGroup)>,
    vbuf: wgpu::Buffer,
    verts: Vec<[f32; 4]>,
    /// (first vertex, count, cache texture)
    batches: Vec<(u32, u32, u32)>,
    /// The frame's shadow colour (None: no shadow this frame).
    color: Option<[f32; 4]>,
}

impl ShadowPass {
    pub fn new(
        gpu: &Gpu,
        format: wgpu::TextureFormat,
        sprite_layout: &wgpu::BindGroupLayout,
        depth: wgpu::DepthStencilState,
    ) -> Self {
        let device = &gpu.device;
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shadow"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let mask_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shadow mask"),
            bind_group_layouts: &[Some(sprite_layout)],
            immediate_size: 0,
        });
        let max = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Max,
        };
        let mask_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow mask"),
            layout: Some(&mask_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 16,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x4],
                })],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_mask"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: MASK,
                    blend: Some(wgpu::BlendState {
                        color: max,
                        alpha: max,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let shade_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow shade"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shadow shade"),
            bind_group_layouts: &[Some(&shade_layout)],
            immediate_size: 0,
        });
        let shade_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow shade"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_full"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(depth),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_shade"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow colour"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let vbuf = Self::vertex_buffer(device, 1024);
        Self {
            mask_pipeline,
            shade_pipeline,
            shade_layout,
            uniform,
            mask: None,
            vbuf,
            verts: Vec::new(),
            batches: Vec::new(),
            color: None,
        }
    }

    fn vertex_buffer(device: &wgpu::Device, verts: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow vertices"),
            size: (verts * 16) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Start a frame: `color` is the shadow's colour and strength (None: no shadow).
    pub fn begin(&mut self, color: Option<[f32; 4]>) {
        self.color = color;
        self.verts.clear();
        self.batches.clear();
    }

    pub fn active(&self) -> bool {
        self.color.is_some()
    }

    /// One sprite's quad (already moved by the offset), textured by cache texture `tex`.
    pub fn push(&mut self, tex: u32, quad: [[f32; 4]; 6]) {
        let start = self.verts.len() as u32;
        self.verts.extend_from_slice(&quad);
        match self.batches.last_mut() {
            Some((s, n, t)) if *t == tex && *s + *n == start => *n += 6,
            _ => self.batches.push((start, 6, tex)),
        }
    }

    /// Draw the frame's mask (before the frame's passes). `bind` gives a cache texture's sprite
    /// bind group; `viewport` is the frame's.
    pub fn encode_mask<'a>(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        size: (u32, u32),
        viewport: (f32, f32, f32, f32),
        bind: impl Fn(u32) -> Option<&'a wgpu::BindGroup>,
    ) {
        let Some(color) = self.color else {
            return;
        };
        let (w, h) = (size.0.max(1), size.1.max(1));
        if self
            .mask
            .as_ref()
            .is_none_or(|(t, ..)| t.width() != w || t.height() != h)
        {
            let t = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("shadow mask"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: MASK,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let v = t.create_view(&wgpu::TextureViewDescriptor::default());
            let g = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("shadow shade"),
                layout: &self.shade_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&v),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.uniform.as_entire_binding(),
                    },
                ],
            });
            self.mask = Some((t, v, g));
        }
        gpu.queue
            .write_buffer(&self.uniform, 0, bytemuck::cast_slice(&color));
        let bytes: &[u8] = bytemuck::cast_slice(&self.verts);
        if bytes.len() as u64 > self.vbuf.size() {
            self.vbuf = Self::vertex_buffer(&gpu.device, self.verts.len().next_power_of_two());
        }
        gpu.queue.write_buffer(&self.vbuf, 0, bytes);
        let (_, view, _) = self.mask.as_ref().expect("made above");
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("shadow mask"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        let (vx, vy, vw, vh) = viewport;
        if vw < 1.0 || vh < 1.0 {
            return;
        }
        pass.set_viewport(vx, vy, vw, vh, 0.0, 1.0);
        pass.set_pipeline(&self.mask_pipeline);
        pass.set_vertex_buffer(0, self.vbuf.slice(..));
        for &(start, n, tex) in &self.batches {
            if let Some(g) = bind(tex) {
                pass.set_bind_group(0, g, &[]);
                pass.draw(start..start + n, 0..1);
            }
        }
    }

    /// Lay the mask over what the pass has drawn so far (the backdrop). The caller restores its
    /// own pipeline and buffers afterwards.
    pub fn shade(&self, pass: &mut wgpu::RenderPass<'_>) {
        let (Some(_), Some((.., group))) = (self.color, &self.mask) else {
            return;
        };
        pass.set_pipeline(&self.shade_pipeline);
        pass.set_bind_group(0, group, &[]);
        pass.draw(0..3, 0..1);
    }
}
