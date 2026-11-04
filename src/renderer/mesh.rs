/// Renders the loaded character model, or a placeholder box when none is loaded.

use crate::formats::model::MaterialLook;
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
    bind_group: wgpu::BindGroup,
    cull_back: bool,
    /// Drawn after the others, blended, without writing depth.
    translucent: bool,
    /// Blended, writing depth: the game blends all of its materials by alpha.
    blended: bool,
}

/// The lights the game uses while it draws fighters: D3D directional lights and ambient.
#[derive(Debug, Clone, Copy)]
pub struct SceneLights {
    pub ambient: [f32; 3],
    /// Direction the light travels, in the game's space, diffuse and specular.
    pub lights: [([f32; 3], [f32; 3], [f32; 3]); 4],
}

/// A stage's lights: ambient, then direction and diffuse. Specular is 0.3 of diffuse in
/// all that were read.
const fn stage_lights(ambient: u8, lights: [([f32; 3], [f32; 3]); 4]) -> SceneLights {
    let mut out = [([0.0; 3], [0.0; 3], [0.0; 3]); 4];
    let mut i = 0;
    while i < 4 {
        let c = lights[i].1;
        out[i] = (lights[i].0, c, [c[0] * 0.3, c[1] * 0.3, c[2] * 0.3]);
        i += 1;
    }
    SceneLights { ambient: [ambient as f32 / 255.0; 3], lights: out }
}

/// Lights read from the game while it draws fighters, by area of a stage. See `doa2u_lights.py`.
pub const DOA2U_LIGHTS: [(&str, SceneLights); 4] = [
    ("Desert", stage_lights(0x5E, [
        ([-0.243, -0.049, -0.969], [0.6, 0.48, 0.45]),
        ([0.962, -0.195, -0.191], [0.78, 0.62, 0.53]),
        ([-0.195, -0.049, 0.98], [0.19, 0.14, 0.28]),
        ([-0.931, 0.337, 0.138], [0.26, 0.24, 0.12]),
    ])),
    ("Area 8", stage_lights(0x3F, [
        ([0.451, -0.29, -0.844], [0.78, 0.57, 0.42]),
        ([-0.677, -0.29, -0.677], [0.62, 0.41, 0.28]),
        ([0.36, 0.337, 0.87], [0.36, 0.22, 0.37]),
        ([-0.878, 0.098, 0.469], [0.2, 0.16, 0.38]),
    ])),
    ("Area 30", stage_lights(0x54, [
        ([-0.628, -0.147, -0.765], [0.72, 0.6, 0.52]),
        ([0.451, -0.29, -0.844], [0.62, 0.54, 0.4]),
        ([0.664, 0.147, 0.733], [0.24, 0.28, 0.38]),
        ([-0.848, -0.147, 0.509], [0.36, 0.38, 0.3]),
    ])),
    // The same stage as area 8, in another part. It has three lights.
    ("Area 48", stage_lights(0x5E, [
        ([0.643, -0.29, -0.709], [0.48, 0.54, 0.63]),
        ([0.884, -0.29, 0.366], [0.35, 0.39, 0.58]),
        ([-0.773, 0.0, -0.634], [0.41, 0.29, 0.1]),
        ([0.0, 0.0, 0.0], [0.0, 0.0, 0.0]),
    ])),
];

struct LoadedModel {
    /// Vertices in bone space. Posed copies are uploaded every frame.
    rest_vertices: Vec<MeshVertex>,
    posed_vertices: Vec<MeshVertex>,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    pieces: Vec<PosedPiece>,
    draws: Vec<DrawCall>,
    /// Keeps the materials' buffers and textures alive with the draws' bind groups.
    _textures: Vec<wgpu::TextureView>,
}

/// CPU-side description of a piece passed to `set_model`.
pub struct PieceInput<'a> {
    pub node: usize,
    pub vertices: &'a [MeshVertex],
    /// Triangle lists relative to `vertices`, each with its texture and
    /// whether back faces are hidden.
    pub draws: Vec<(&'a [u32], Option<usize>, bool, Option<MaterialLook>)>,
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
    blended_pipeline: wgpu::RenderPipeline,
    blended_culling_pipeline: wgpu::RenderPipeline,
    uniform_buf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    placeholder_vb: wgpu::Buffer,
    placeholder_count: u32,
    model: Option<LoadedModel>,
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    white_view: wgpu::TextureView,
    white_bind_group: wgpu::BindGroup,
    lights: Option<SceneLights>,
}

impl MeshRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, surface_format: wgpu::TextureFormat) -> Self {
        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Mesh UB"),
            size: UNIFORM_FLOATS as u64 * 4, // matrices, shading options and lights
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
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

        let make_pipeline = |cull_mode: Option<wgpu::Face>, blended: bool, write_depth: bool| device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
                    blend: Some(if blended { wgpu::BlendState::ALPHA_BLENDING } else { wgpu::BlendState::REPLACE }),
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
                depth_write_enabled: write_depth,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let pipeline = make_pipeline(None, false, true);
        let culling_pipeline = make_pipeline(Some(wgpu::Face::Back), false, true);
        let translucent_pipeline = make_pipeline(None, true, false);
        let translucent_culling_pipeline = make_pipeline(Some(wgpu::Face::Back), true, false);
        let blended_pipeline = make_pipeline(None, true, true);
        let blended_culling_pipeline = make_pipeline(Some(wgpu::Face::Back), true, true);

        // Create a placeholder box to show when no model is loaded
        let placeholder_verts = build_placeholder_box(0.5, 1.8, 0.3);
        let placeholder_count = placeholder_verts.len() as u32;
        let placeholder_vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Placeholder VB"),
            contents: bytemuck::cast_slice(&placeholder_verts),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let white_view = create_texture(device, queue, 1, 1, &[255; 4]);
        let white_bind_group = draw_bind_group(device, &texture_layout, &sampler, &white_view, &white_view, None);

        Self {
            pipeline,
            culling_pipeline,
            translucent_pipeline,
            translucent_culling_pipeline,
            blended_pipeline,
            blended_culling_pipeline,
            uniform_buf,
            bind_group,
            placeholder_vb,
            placeholder_count,
            model: None,
            texture_layout,
            sampler,
            white_view,
            white_bind_group,
            lights: None,
        }
    }

    /// The lights for models whose materials have a look. None keeps the viewer's own shading.
    pub fn set_lights(&mut self, lights: Option<SceneLights>) {
        self.lights = lights;
    }

    /// Whether the current model uses the game's lighting.
    pub fn is_lit(&self) -> bool {
        self.lights.is_some()
    }

    pub fn update(&self, queue: &wgpu::Queue, view_proj: &glam::Mat4, view: &glam::Mat4, anime_shading: bool) {
        let model = glam::Mat4::IDENTITY;
        let mut buf = [0f32; UNIFORM_FLOATS];
        buf[..16].copy_from_slice(&view_proj.to_cols_array());
        buf[16..32].copy_from_slice(&model.to_cols_array());
        buf[32] = if anime_shading { 1.0 } else { 0.0 };
        buf[36..52].copy_from_slice(&view.to_cols_array());
        if let Some(lights) = &self.lights {
            buf[52..55].copy_from_slice(&lights.ambient);
            for (i, (direction, diffuse, specular)) in lights.lights.iter().enumerate() {
                // Toward the light, in the viewer's space, whose Z is the game's flipped.
                buf[56 + i * 4..59 + i * 4].copy_from_slice(&[-direction[0], -direction[1], direction[2]]);
                buf[72 + i * 4..75 + i * 4].copy_from_slice(diffuse);
                buf[88 + i * 4..91 + i * 4].copy_from_slice(specular);
            }
        }
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
        let views: Vec<wgpu::TextureView> =
            textures.iter().map(|t| create_texture(device, queue, t.width, t.height, &t.rgba)).collect();
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
            for (triangles, texture, cull_back, look) in &input.draws {
                let first_index = indices.len() as u32;
                indices.extend(triangles.iter().map(|i| i + base as u32));
                let view = texture.and_then(|t| views.get(t)).unwrap_or(&self.white_view);
                let layer = look.and_then(|l| l.layer).and_then(|t| views.get(t));
                let bind_group = draw_bind_group(device, &self.texture_layout, &self.sampler, view, layer.unwrap_or(&self.white_view), look.as_ref().map(|l| (l, layer.is_some())));
                let translucent = input.translucent || look.is_some_and(|l| l.no_depth_write);
                let blended = look.is_some_and(|l| l.alpha_from_texture || l.layer.is_some() || l.diffuse[3] < 1.0);
                draws.push(DrawCall { indices: first_index..indices.len() as u32, bind_group, cull_back: *cull_back, translucent, blended });
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
        self.model = Some(LoadedModel {
            posed_vertices: rest_vertices.clone(),
            rest_vertices,
            vertex_buffer,
            index_buffer,
            pieces,
            draws,
            _textures: views,
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

    /// Copies rest positions between pieces, as they are.
    pub fn copy_rest_positions(&mut self, from: usize, to: usize, links: &[(usize, usize)]) {
        let Some(model) = &mut self.model else { return };
        let (Some(source), Some(target)) = (model.pieces.get(from).map(|p| p.vertices.clone()), model.pieces.get(to).map(|p| p.vertices.clone())) else {
            return;
        };
        for &(a, b) in links {
            if a < source.len() && b < target.len() {
                model.rest_vertices[target.start + b].position = model.rest_vertices[source.start + a].position;
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
                    pass.set_pipeline(match (draw.translucent, draw.blended, draw.cull_back) {
                        (false, false, false) => &self.pipeline,
                        (false, false, true) => &self.culling_pipeline,
                        (false, true, false) => &self.blended_pipeline,
                        (false, true, true) => &self.blended_culling_pipeline,
                        (true, _, false) => &self.translucent_pipeline,
                        (true, _, true) => &self.translucent_culling_pipeline,
                    });
                    pass.set_bind_group(1, &draw.bind_group, &[]);
                    pass.draw_indexed(draw.indices.clone(), 0, 0..1);
                }
            }
            None => {
                pass.set_bind_group(1, &self.white_bind_group, &[]);
                pass.set_vertex_buffer(0, self.placeholder_vb.slice(..));
                pass.draw(0..self.placeholder_count, 0..1);
            }
        }
    }
}

/// Floats in the uniform buffer. The material holds 6 vec4 in its own buffer.
const UNIFORM_FLOATS: usize = 104;

/// A draw's textures and material. Without a look it keeps the viewer's own shading.
fn draw_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    base: &wgpu::TextureView,
    layer: &wgpu::TextureView,
    look: Option<(&MaterialLook, bool)>,
) -> wgpu::BindGroup {
    let mut words = [0f32; 24];
    if let Some((look, has_layer)) = look {
        words[..4].copy_from_slice(&look.diffuse);
        words[4..7].copy_from_slice(&look.ambient);
        words[8..11].copy_from_slice(&look.specular);
        words[11] = look.power;
        words[12..15].copy_from_slice(&look.emissive);
        let on = |value: bool| if value { 1.0 } else { 0.0 };
        words[16..20].copy_from_slice(&[1.0, on(look.specular_on), on(look.alpha_from_texture), on(has_layer)]);
        words[20] = on(look.base_reflects);
        words[21] = on(look.layer_reflects);
    }
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Material UB"),
        contents: bytemuck::cast_slice(&words),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Draw BG"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(base) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(layer) },
            wgpu::BindGroupEntry { binding: 3, resource: buffer.as_entire_binding() },
        ],
    })
}

fn create_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> wgpu::TextureView {
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
    texture.create_view(&wgpu::TextureViewDescriptor::default())
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
    view: mat4x4<f32>,
    ambient: vec4<f32>,
    light_to: array<vec4<f32>, 4>,
    light_diffuse: array<vec4<f32>, 4>,
    light_specular: array<vec4<f32>, 4>,
}
@group(0) @binding(0) var<uniform> u: Uniforms;

struct Material {
    diffuse: vec4<f32>,
    ambient: vec4<f32>,
    /// Power in w.
    specular: vec4<f32>,
    emissive: vec4<f32>,
    /// x: the game's lighting, y: specular, z: alpha from the texture, w: has a layer.
    flags: vec4<f32>,
    /// x: base reflects, y: layer reflects.
    coords: vec4<f32>,
}
@group(1) @binding(0) var color_texture: texture_2d<f32>;
@group(1) @binding(1) var color_sampler: sampler;
@group(1) @binding(2) var layer_texture: texture_2d<f32>;
@group(1) @binding(3) var<uniform> material: Material;

struct VertIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
}
struct VertOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) layer_uv: vec2<f32>,
    @location(3) lit: vec3<f32>,
    @location(4) specular: vec3<f32>,
}

@vertex
fn vs_main(v: VertIn) -> VertOut {
    var out: VertOut;
    let world_pos = u.model * vec4<f32>(v.pos, 1.0);
    out.clip = u.view_proj * world_pos;
    out.normal = v.normal;
    out.uv = v.uv;
    out.layer_uv = v.uv;
    if material.flags.x > 0.5 {
        // Per vertex, as the game's fixed function lighting (d3d_draw_shader.cpp in its port).
        let n = normalize(v.normal);
        let view_n = (u.view * vec4<f32>(n, 0.0)).xyz;
        let view_pos = (u.view * world_pos).xyz;
        // A camera space reflection. The game's matrix for stage 0 is a 0.5, -0.5 sphere
        // map, and for stage 1 the identity (read during fighter draws).
        let eye = normalize(-view_pos);
        let reflected = 2.0 * dot(eye, view_n) * view_n - eye;
        if material.coords.x > 0.5 {
            out.uv = vec2<f32>(0.5 * reflected.x + 0.5, -0.5 * reflected.y + 0.5);
        }
        if material.coords.y > 0.5 {
            out.layer_uv = reflected.xy;
        }
        // The game's specular takes an infinite viewer: +Z in camera space.
        let to_eye = (vec4<f32>(0.0, 0.0, 1.0, 0.0) * u.view).xyz;
        var rgb = material.emissive.rgb + material.ambient.rgb * u.ambient.rgb;
        var specular = vec3<f32>(0.0);
        for (var i = 0; i < 4; i += 1) {
            let l = u.light_to[i].xyz;
            let d = dot(n, l);
            if d > 0.0 {
                rgb += material.diffuse.rgb * u.light_diffuse[i].rgb * d;
                let h = normalize(l + to_eye);
                specular += material.specular.rgb * u.light_specular[i].rgb * pow(max(dot(n, h), 1e-5), material.specular.w);
            }
        }
        out.lit = clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0));
        out.specular = clamp(specular, vec3<f32>(0.0), vec3<f32>(1.0)) * material.flags.y;
    }
    return out;
}

// The game works on stored texture values, with no gamma. The viewer's textures and
// window are sRGB, so colors go out to those values and back.
fn to_stored(c: vec3<f32>) -> vec3<f32> {
    let low = c * 12.92;
    let high = 1.055 * pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(high, low, c <= vec3<f32>(0.0031308));
}
fn from_stored(c: vec3<f32>) -> vec3<f32> {
    let low = c / 12.92;
    let high = pow((max(c, vec3<f32>(0.0)) + 0.055) / 1.055, vec3<f32>(2.4));
    return select(high, low, c <= vec3<f32>(0.04045));
}

@fragment
fn fs_main(v: VertOut) -> @location(0) vec4<f32> {
    let light = normalize(vec3<f32>(0.5, 1.0, 0.7));
    let diff = abs(dot(normalize(v.normal), light));
    let ambient = 0.3;
    let texel = textureSample(color_texture, color_sampler, v.uv);
    let layer = textureSample(layer_texture, color_sampler, v.layer_uv);
    if u.anime_shading.x > 0.5 {
        if texel.a < 0.1 {
            discard;
        }
        // Three flat bands instead of a smooth gradient.
        var band = 0.55;
        if diff > 0.75 {
            band = 1.1;
        } else if diff > 0.3 {
            band = 0.85;
        }
        return vec4<f32>(texel.rgb * band, texel.a);
    }
    if material.flags.x > 0.5 {
        var color = to_stored(texel.rgb);
        var alpha = material.diffuse.a;
        if material.flags.z > 0.5 {
            alpha *= texel.a;
            if alpha < 0.1 {
                discard;
            }
        }
        if material.flags.w > 0.5 {
            color = mix(color, to_stored(layer.rgb), layer.a);
            alpha = material.diffuse.a * mix(alpha, layer.a, layer.a);
        }
        let lit = clamp(color * v.lit + v.specular, vec3<f32>(0.0), vec3<f32>(1.0));
        return vec4<f32>(from_stored(lit), alpha);
    }
    if texel.a < 0.1 {
        discard;
    }
    return vec4<f32>(texel.rgb * (ambient + diff * 0.7), texel.a);
}
"#;

