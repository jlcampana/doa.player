/// Renders the loaded character model, or a placeholder box when none is loaded.

use crate::formats::doa3::{ChestMorphGroup, JointBlend, SkinVertex};
use crate::formats::model::{ModelTexture, ModelVertex as MeshVertex};
use wgpu::util::DeviceExt;

/// One piece of a model: a range of the shared vertex buffer, moved by a node.
struct PosedPiece {
    node: usize,
    vertices: std::ops::Range<usize>,
    parent: usize,
    skin: Option<(JointBlend, Vec<SkinVertex>)>,
    bend: Option<VertexBend>,
    /// (rest vertex of a rigid piece, its node, vertex of this piece), in the shared buffer.
    welds: Vec<(usize, usize, usize)>,
}

/// Vertices blended between their node and a parent node moved by `offset`.
#[derive(Clone)]
pub struct VertexBend {
    pub parent: usize,
    pub offset: glam::Vec3,
    /// How much of each vertex follows the piece's own node.
    pub weights: Vec<f32>,
}

/// A range of the shared index buffer drawn with one texture.
struct DrawCall {
    indices: std::ops::Range<u32>,
    texture: Option<usize>,
    cull_back: bool,
    translucent: bool,
}

struct LoadedModel {
    /// Vertices in bone space. Posed copies are uploaded every frame.
    rest_vertices: Vec<MeshVertex>,
    posed_vertices: Vec<MeshVertex>,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    pieces: Vec<PosedPiece>,
    draws: Vec<DrawCall>,
    textures: Vec<wgpu::BindGroup>,
}

/// CPU-side description of a piece passed to `set_model`.
pub struct PieceInput<'a> {
    pub node: usize,
    pub vertices: &'a [MeshVertex],
    /// Triangle lists relative to `vertices`, each with its texture and
    /// whether back faces are hidden.
    pub draws: Vec<(&'a [u32], Option<usize>, bool)>,
    /// Node that `skin` positions are relative to.
    pub parent: usize,
    pub skin: Option<(JointBlend, &'a [SkinVertex])>,
    pub bend: Option<VertexBend>,
    /// (input index of a rigid piece, its vertex, this piece's vertex)
    pub welds: Vec<(usize, usize, usize)>,
    /// Blended over everything else, without writing depth.
    pub translucent: bool,
}

pub struct MeshRenderer {
    pipeline: wgpu::RenderPipeline,
    /// The same, hiding back faces. In this viewer's space the faces meant to be
    /// seen wind counter-clockwise.
    culling_pipeline: wgpu::RenderPipeline,
    translucent_pipeline: wgpu::RenderPipeline,
    translucent_culling_pipeline: wgpu::RenderPipeline,
    uniform_buf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    placeholder_vb: wgpu::Buffer,
    placeholder_count: u32,
    model: Option<LoadedModel>,
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    white_texture: wgpu::BindGroup,
}

impl MeshRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, surface_format: wgpu::TextureFormat) -> Self {
        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Mesh UB"),
            size: 144, // view_proj, model matrix and shading options
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Mesh BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Mesh BG"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            }],
        });

        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Mesh Texture BGL"),
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
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Mesh Sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Mesh PL"),
            bind_group_layouts: &[&bind_group_layout, &texture_layout],
            push_constant_ranges: &[],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Mesh Shader"),
            source: wgpu::ShaderSource::Wgsl(MESH_WGSL.into()),
        });

        let make_pipeline = |cull_mode: Option<wgpu::Face>, translucent: bool| device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Mesh Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<MeshVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x3,
                        1 => Float32x3,
                        2 => Float32x2,
                    ],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(if translucent { wgpu::BlendState::ALPHA_BLENDING } else { wgpu::BlendState::REPLACE }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: !translucent,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let pipeline = make_pipeline(None, false);
        let culling_pipeline = make_pipeline(Some(wgpu::Face::Back), false);
        let translucent_pipeline = make_pipeline(None, true);
        let translucent_culling_pipeline = make_pipeline(Some(wgpu::Face::Back), true);

        // Create a placeholder box to show when no model is loaded
        let placeholder_verts = build_placeholder_box(0.5, 1.8, 0.3);
        let placeholder_count = placeholder_verts.len() as u32;
        let placeholder_vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Placeholder VB"),
            contents: bytemuck::cast_slice(&placeholder_verts),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let white_texture = create_texture(device, queue, &texture_layout, &sampler, 1, 1, &[255; 4]);

        Self {
            pipeline,
            culling_pipeline,
            translucent_pipeline,
            translucent_culling_pipeline,
            uniform_buf,
            bind_group,
            placeholder_vb,
            placeholder_count,
            model: None,
            texture_layout,
            sampler,
            white_texture,
        }
    }

    pub fn update(&self, queue: &wgpu::Queue, view_proj: &glam::Mat4, anime_shading: bool) {
        let model = glam::Mat4::IDENTITY;
        let mut buf = [0f32; 36];
        buf[..16].copy_from_slice(&view_proj.to_cols_array());
        buf[16..32].copy_from_slice(&model.to_cols_array());
        buf[32] = if anime_shading { 1.0 } else { 0.0 };
        queue.write_buffer(&self.uniform_buf, 0, bytemuck::cast_slice(&buf));
    }

    pub fn set_model(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        inputs: &[PieceInput],
        textures: &[ModelTexture],
    ) {
        let mut rest_vertices = Vec::new();
        let mut indices = Vec::new();
        let mut pieces = Vec::new();
        let mut draws = Vec::new();
        let bases: Vec<usize> = inputs
            .iter()
            .scan(0, |next, input| {
                let base = *next;
                *next += input.vertices.len();
                Some(base)
            })
            .collect();
        for input in inputs {
            let base = rest_vertices.len();
            rest_vertices.extend_from_slice(input.vertices);
            pieces.push(PosedPiece {
                node: input.node,
                vertices: base..rest_vertices.len(),
                parent: input.parent,
                skin: input.skin.map(|(blend, vertices)| (blend, vertices.to_vec())),
                bend: input.bend.clone(),
                welds: input
                    .welds
                    .iter()
                    .map(|&(piece, source, target)| (bases[piece] + source, inputs[piece].node, base + target))
                    .collect(),
            });
            for (triangles, texture, cull_back) in &input.draws {
                let first_index = indices.len() as u32;
                indices.extend(triangles.iter().map(|i| i + base as u32));
                draws.push(DrawCall { indices: first_index..indices.len() as u32, texture: *texture, cull_back: *cull_back, translucent: input.translucent });
            }
        }

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Model VB"),
            contents: bytemuck::cast_slice(&rest_vertices),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Model IB"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let textures = textures
            .iter()
            .map(|t| create_texture(device, queue, &self.texture_layout, &self.sampler, t.width, t.height, &t.rgba))
            .collect();

        self.model = Some(LoadedModel {
            posed_vertices: rest_vertices.clone(),
            rest_vertices,
            vertex_buffer,
            index_buffer,
            pieces,
            draws,
            textures,
        });
    }

    pub fn clear_model(&mut self) {
        self.model = None;
    }

    /// Replaces rest positions of one input piece's vertices.
    pub fn deform_piece(&mut self, piece: usize, groups: &[ChestMorphGroup], value: f32) {
        let Some(model) = &mut self.model else { return };
        let Some(range) = model.pieces.get(piece).map(|p| p.vertices.clone()) else { return };
        let weight = value.abs();
        let rest = &mut model.rest_vertices[range];
        for group in groups {
            let offset = if value >= 0.0 { group.positive } else { group.negative };
            let position = (group.base + offset * weight).to_array();
            for &vertex in &group.vertices {
                if let Some(out) = rest.get_mut(vertex as usize) {
                    out.position = position;
                }
            }
        }
    }

    /// Replaces rest positions of a piece, in the space of its node.
    pub fn set_rest_positions(&mut self, piece: usize, positions: impl Iterator<Item = (usize, glam::Vec3)>) {
        let Some(model) = &mut self.model else { return };
        let Some(range) = model.pieces.get(piece).map(|p| p.vertices.clone()) else { return };
        let rest = &mut model.rest_vertices[range];
        for (vertex, position) in positions {
            if let Some(out) = rest.get_mut(vertex) {
                out.position = position.to_array();
            }
        }
    }

    /// Replaces rest normals of a piece, in the space of its node.
    pub fn set_rest_normals(&mut self, piece: usize, normals: impl Iterator<Item = (usize, [f32; 3])>) {
        let Some(model) = &mut self.model else { return };
        let Some(range) = model.pieces.get(piece).map(|p| p.vertices.clone()) else { return };
        let rest = &mut model.rest_vertices[range];
        for (vertex, normal) in normals {
            if let Some(out) = rest.get_mut(vertex) {
                out.normal = normal;
            }
        }
    }

    /// Moves the vertices of every piece by its node matrix and uploads them.
    pub fn apply_pose(&mut self, queue: &wgpu::Queue, nodes: &[glam::Mat4]) {
        let Some(model) = &mut self.model else { return };
        for piece in &model.pieces {
            let Some(matrix) = nodes.get(piece.node) else { continue };
            let normal_matrix = glam::Mat3::from_mat4(*matrix);
            let rest = &model.rest_vertices[piece.vertices.clone()];
            let posed = &mut model.posed_vertices[piece.vertices.clone()];
            for (out, vertex) in posed.iter_mut().zip(rest) {
                out.position = matrix.transform_point3(vertex.position.into()).into();
                out.normal = (normal_matrix * glam::Vec3::from(vertex.normal)).into();
            }

            if let Some(bend) = &piece.bend {
                if let Some(parent) = nodes.get(bend.parent) {
                    let moved = *parent * glam::Mat4::from_translation(bend.offset);
                    let moved_normals = glam::Mat3::from_mat4(moved);
                    for ((out, vertex), &weight) in posed.iter_mut().zip(rest).zip(&bend.weights) {
                        let position = glam::Vec3::from(vertex.position);
                        let normal = glam::Vec3::from(vertex.normal);
                        let own = matrix.transform_point3(position);
                        out.position = own.lerp(moved.transform_point3(position), 1.0 - weight).into();
                        let own_normal = normal_matrix * normal;
                        out.normal = own_normal.lerp(moved_normals * normal, 1.0 - weight).normalize_or_zero().into();
                    }
                }
            }

            // Bend the joint piece between its node and the parent node.
            let (Some((blend, skin)), Some(parent)) = (&piece.skin, nodes.get(piece.parent)) else { continue };
            for vertex in skin {
                let Some(out) = posed.get_mut(vertex.vertex) else { continue };
                let (position, normal) = if vertex.weight >= 1.0 {
                    (parent.transform_point3(vertex.position), parent.transform_vector3(vertex.normal))
                } else {
                    let (rotation, origin) = blend.frame(matrix, parent, vertex.weight);
                    (rotation * vertex.position + origin, rotation * vertex.normal)
                };
                out.position = position.into();
                out.normal = normal.normalize_or_zero().into();
            }
        }

        // Joint vertices that sit on a rigid piece stay on it.
        for piece in &model.pieces {
            for &(source, node, target) in &piece.welds {
                let Some(matrix) = nodes.get(node) else { continue };
                let rest = model.rest_vertices[source];
                let out = &mut model.posed_vertices[target];
                out.position = matrix.transform_point3(rest.position.into()).into();
                out.normal = matrix.transform_vector3(rest.normal.into()).normalize_or_zero().into();
            }
        }
        queue.write_buffer(&model.vertex_buffer, 0, bytemuck::cast_slice(&model.posed_vertices));
    }

    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        match &self.model {
            Some(model) => {
                pass.set_vertex_buffer(0, model.vertex_buffer.slice(..));
                pass.set_index_buffer(model.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                let (solid, translucent): (Vec<_>, Vec<_>) = model.draws.iter().partition(|draw| !draw.translucent);
                for draw in solid.into_iter().chain(translucent) {
                    let texture = draw.texture.and_then(|t| model.textures.get(t)).unwrap_or(&self.white_texture);
                    pass.set_pipeline(match (draw.translucent, draw.cull_back) {
                        (false, false) => &self.pipeline,
                        (false, true) => &self.culling_pipeline,
                        (true, false) => &self.translucent_pipeline,
                        (true, true) => &self.translucent_culling_pipeline,
                    });
                    pass.set_bind_group(1, texture, &[]);
                    pass.draw_indexed(draw.indices.clone(), 0, 0..1);
                }
            }
            None => {
                pass.set_bind_group(1, &self.white_texture, &[]);
                pass.set_vertex_buffer(0, self.placeholder_vb.slice(..));
                pass.draw(0..self.placeholder_count, 0..1);
            }
        }
    }
}

fn create_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> wgpu::BindGroup {
    let size = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
    let texture = device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: Some("Model Texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        rgba,
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Model Texture BG"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
        ],
    })
}

fn build_placeholder_box(w: f32, h: f32, d: f32) -> Vec<MeshVertex> {
    let hw = w / 2.0;
    let hd = d / 2.0;
    // Simple box (12 triangles)
    let faces = [
        // front
        ([-hw, 0.0, hd], [-hw, h, hd], [hw, h, hd], [hw, 0.0, hd], [0.0, 0.0, 1.0f32]),
        // back
        ([hw, 0.0, -hd], [hw, h, -hd], [-hw, h, -hd], [-hw, 0.0, -hd], [0.0, 0.0, -1.0]),
        // left
        ([-hw, 0.0, -hd], [-hw, h, -hd], [-hw, h, hd], [-hw, 0.0, hd], [-1.0, 0.0, 0.0]),
        // right
        ([hw, 0.0, hd], [hw, h, hd], [hw, h, -hd], [hw, 0.0, -hd], [1.0, 0.0, 0.0]),
        // top
        ([-hw, h, hd], [-hw, h, -hd], [hw, h, -hd], [hw, h, hd], [0.0, 1.0, 0.0]),
        // bottom
        ([-hw, 0.0, -hd], [-hw, 0.0, hd], [hw, 0.0, hd], [hw, 0.0, -hd], [0.0, -1.0, 0.0]),
    ];

    let mut verts = Vec::new();
    for (a, b, c, dd, n) in &faces {
        let quad = [*a, *b, *c, *a, *c, *dd];
        for (i, p) in quad.iter().enumerate() {
            let u = if i % 3 == 0 || i == 3 { 0.0 } else if i == 1 || i == 4 { 0.0 } else { 1.0 };
            let v = if i < 3 { 0.0 } else { 1.0 };
            verts.push(MeshVertex {
                position: *p,
                normal: *n,
                uv: [u, v],
            });
        }
    }
    verts
}

const MESH_WGSL: &str = r#"
struct Uniforms {
    view_proj: mat4x4<f32>,
    model: mat4x4<f32>,
    anime_shading: vec4<f32>,
}
@group(0) @binding(0) var<uniform> u: Uniforms;

struct VertIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
}
struct VertOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
}

@group(1) @binding(0) var color_texture: texture_2d<f32>;
@group(1) @binding(1) var color_sampler: sampler;

@vertex
fn vs_main(v: VertIn) -> VertOut {
    var out: VertOut;
    let world_pos = u.model * vec4<f32>(v.pos, 1.0);
    out.clip = u.view_proj * world_pos;
    out.normal = v.normal;
    out.uv = v.uv;
    return out;
}

@fragment
fn fs_main(v: VertOut) -> @location(0) vec4<f32> {
    let light = normalize(vec3<f32>(0.5, 1.0, 0.7));
    let diff = abs(dot(normalize(v.normal), light));
    let ambient = 0.3;
    let texel = textureSample(color_texture, color_sampler, v.uv);
    if texel.a < 0.1 {
        discard;
    }
    if u.anime_shading.x > 0.5 {
        // Three flat bands instead of a smooth gradient.
        var band = 0.55;
        if diff > 0.75 {
            band = 1.1;
        } else if diff > 0.3 {
            band = 0.85;
        }
        return vec4<f32>(texel.rgb * band, texel.a);
    }
    return vec4<f32>(texel.rgb * (ambient + diff * 0.7), texel.a);
}
"#;
