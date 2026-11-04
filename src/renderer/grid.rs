/// Renders a simple ground grid as reference.

use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct GridVertex {
    position: [f32; 3],
    color: [f32; 4],
}

pub struct GridRenderer {
    pipeline: wgpu::RenderPipeline,
    vertex_buf: wgpu::Buffer,
    uniform_buf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    vertex_count: u32,
}

impl GridRenderer {
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        let verts = build_grid(20, 1.0);
        let vertex_count = verts.len() as u32;

        let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Grid VB"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Grid UB"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Grid BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Grid BG"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Grid PL"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Grid Shader"),
            source: wgpu::ShaderSource::Wgsl(GRID_WGSL.into()),
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Grid Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<GridVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self { pipeline, vertex_buf, uniform_buf, bind_group, vertex_count }
    }

    pub fn update(&self, queue: &wgpu::Queue, view_proj: &glam::Mat4) {
        queue.write_buffer(&self.uniform_buf, 0, bytemuck::cast_slice(&view_proj.to_cols_array()));
    }

    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
        pass.draw(0..self.vertex_count, 0..1);
    }
}

fn build_grid(half_count: i32, spacing: f32) -> Vec<GridVertex> {
    let mut verts = Vec::new();
    let dim = half_count as f32 * spacing;
    let grid_color = [0.3, 0.3, 0.3, 0.6f32];
    let axis_x = [0.7, 0.2, 0.2, 1.0f32];
    let axis_z = [0.2, 0.2, 0.7, 1.0f32];

    for i in -half_count..=half_count {
        let p = i as f32 * spacing;
        let color = if i == 0 { axis_z } else { grid_color };
        verts.push(GridVertex { position: [-dim, 0.0, p], color });
        verts.push(GridVertex { position: [dim, 0.0, p], color });
        let color = if i == 0 { axis_x } else { grid_color };
        verts.push(GridVertex { position: [p, 0.0, -dim], color });
        verts.push(GridVertex { position: [p, 0.0, dim], color });
    }
    verts
}

const GRID_WGSL: &str = r#"
struct Uniforms { view_proj: mat4x4<f32>, }
@group(0) @binding(0) var<uniform> u: Uniforms;

struct VertIn {
    @location(0) pos: vec3<f32>,
    @location(1) col: vec4<f32>,
}
struct VertOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) col: vec4<f32>,
}

@vertex
fn vs_main(v: VertIn) -> VertOut {
    var out: VertOut;
    out.clip = u.view_proj * vec4<f32>(v.pos, 1.0);
    out.col = v.col;
    return out;
}

@fragment
fn fs_main(v: VertOut) -> @location(0) vec4<f32> {
    return v.col;
}
"#;
