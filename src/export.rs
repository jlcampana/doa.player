//! Writes a costume as a binary glTF (`.glb`), which Blender imports directly: one object per
//! piece, an armature, skin weights, embedded PNG textures and baked animations.
//!
//! What it does not hold: soft bodies stay on their bone, in the shape of the file, because
//! they are simulated at run time. DOA3 hand poses and faces are not exported either.
//! Joint pieces bend by quaternion in the game and by weights here, so they differ slightly
//! from the viewer away from the rest pose. Rigid pieces are exact.

use anyhow::{bail, Context, Result};
use glam::{Mat3, Mat4, Quat, Vec3};
use serde_json::{json, Value};
use std::path::Path;

use crate::formats::doa3::{JointBlend, SkinVertex, NODE_COUNT, NODE_EYES};
use crate::formats::doa3_morphs::PoseHand;
use crate::formats::model::{MaterialLook, ModelTexture, ModelVertex};
use crate::renderer::mesh::{MultiBend, PieceInput, VertexBend};

/// Each node's parent, from how the pose solves build them. Nodes with none hang from the
/// armature. See `solve_pose` in `doa3.rs` and `doa2u.rs`.
const BONE_PARENTS: [Option<usize>; NODE_COUNT] = {
    let mut parents = [None; NODE_COUNT];
    parents[0] = Some(2);
    parents[1] = Some(0);
    // Legs from the pelvis, arms from the chest, each as upper, middle and end.
    parents[6] = Some(2);
    parents[7] = Some(6);
    parents[3] = Some(7);
    parents[12] = Some(2);
    parents[13] = Some(12);
    parents[9] = Some(13);
    parents[8] = Some(0);
    parents[4] = Some(8);
    parents[5] = Some(4);
    parents[14] = Some(0);
    parents[10] = Some(14);
    parents[11] = Some(10);
    // Forearm twists.
    parents[19] = Some(4);
    parents[20] = Some(10);
    // DOA2U's ends of those twists, and the eyes, in `doa2u::NODE_TWIST_ENDS` and `NODE_EYES`.
    parents[23] = Some(19);
    parents[24] = Some(20);
    parents[NODE_EYES[0]] = Some(1);
    parents[NODE_EYES[1]] = Some(1);
    parents
};

fn bone_name(node: usize) -> String {
    match node {
        2 => "pelvis".into(),
        0 => "chest".into(),
        1 => "head".into(),
        6 | 12 => format!("leg_upper_{node}"),
        7 | 13 => format!("leg_lower_{node}"),
        3 | 9 => format!("foot_{node}"),
        8 | 14 => format!("arm_upper_{node}"),
        4 | 10 => format!("arm_lower_{node}"),
        5 | 11 => format!("hand_{node}"),
        19 | 20 => format!("arm_twist_{node}"),
        23 | 24 => format!("arm_twist_end_{node}"),
        _ if NODE_EYES.contains(&node) => format!("eye_{node}"),
        _ => format!("node_{node}"),
    }
}

/// The parents and names of DOA3's and DOA2U's nodes.
pub fn standard_skeleton() -> (Vec<Option<usize>>, Vec<String>) {
    (BONE_PARENTS.to_vec(), (0..NODE_COUNT).map(bone_name).collect())
}

/// A draw of a piece, owned.
#[derive(Clone)]
pub struct ExportDraw {
    pub indices: Vec<u32>,
    pub texture: Option<usize>,
    pub cull_back: bool,
    pub look: Option<MaterialLook>,
}

/// A piece, owned, as the viewer was given it.
#[derive(Clone)]
pub struct ExportPiece {
    pub node: usize,
    pub parent: usize,
    pub vertices: Vec<ModelVertex>,
    pub draws: Vec<ExportDraw>,
    pub skin: Option<(JointBlend, Vec<SkinVertex>)>,
    pub bend: Option<VertexBend>,
    pub multi: Option<MultiBend>,
    pub welds: Vec<(usize, usize, usize)>,
    pub translucent: bool,
}

/// What the loaders gave the renderer, kept for exporting.
#[derive(Clone, Default)]
pub struct ExportModel {
    pub pieces: Vec<ExportPiece>,
    pub textures: Vec<ModelTexture>,
}

impl ExportModel {
    pub fn new(inputs: &[PieceInput], textures: &[ModelTexture]) -> Self {
        let pieces = inputs
            .iter()
            .map(|input| ExportPiece {
                node: input.node,
                parent: input.parent,
                vertices: input.vertices.to_vec(),
                draws: input
                    .draws
                    .iter()
                    .map(|(indices, texture, cull_back, look)| ExportDraw {
                        indices: indices.to_vec(),
                        texture: *texture,
                        cull_back: *cull_back,
                        look: *look,
                    })
                    .collect(),
                skin: input.skin.map(|(blend, vertices)| (blend, vertices.to_vec())),
                bend: input.bend.clone(),
                multi: input.multi.clone(),
                welds: input.welds.clone(),
                translucent: input.translucent,
            })
            .collect();
        Self { pieces, textures: textures.to_vec() }
    }
}

/// A motion, baked per game frame.
pub struct ExportClip {
    pub name: String,
    /// The solve's node matrices, per frame.
    pub frames: Vec<Vec<Mat4>>,
    /// Per frame and per hand of `Export::hands`: from, to and progress.
    pub hands: Vec<Vec<(usize, usize, f32)>>,
}

/// A hand with pose shapes, on a piece.
pub struct ExportHand<'a> {
    pub piece: usize,
    pub hand: &'a PoseHand,
}

pub struct Export<'a> {
    pub model: &'a ExportModel,
    /// The nodes of the rest pose.
    pub bind: Vec<Mat4>,
    /// Each node's parent and name.
    pub parents: Vec<Option<usize>>,
    pub names: Vec<String>,
    pub clips: &'a [ExportClip],
    pub hands: &'a [ExportHand<'a>],
}

/// The solves return nodes with the viewer's Z flip on the left, which is a reflection.
/// Flipping again on the right gives a proper rotation, which glTF needs, and skinning
/// with it gives the same vertices: see `skinning_gives_the_viewer_s_vertices`.
fn joint(node: &Mat4) -> Mat4 {
    *node * Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0))
}

/// A vertex in the rest pose: position, normal, joints and weights.
#[derive(Clone, Copy)]
struct BoundVertex {
    position: Vec3,
    normal: Vec3,
    joints: [u8; 4],
    weights: [f32; 4],
}

fn bound_vertices(pieces: &[ExportPiece], index: usize, bind: &[Mat4]) -> Vec<BoundVertex> {
    let piece = &pieces[index];
    let node = piece.node.min(bind.len() - 1);
    let matrix = bind[node];
    let normals = Mat3::from_mat4(matrix);
    let mut bound: Vec<BoundVertex> = piece
        .vertices
        .iter()
        .map(|vertex| BoundVertex {
            position: matrix.transform_point3(vertex.position.into()),
            normal: normals * Vec3::from(vertex.normal),
            joints: [node as u8, 0, 0, 0],
            weights: [1.0, 0.0, 0.0, 0.0],
        })
        .collect();
    if let Some(bend) = &piece.bend {
        if let Some(parent) = bind.get(bend.parent) {
            let moved = *parent * Mat4::from_translation(bend.offset);
            for ((out, vertex), &weight) in bound.iter_mut().zip(&piece.vertices).zip(&bend.weights) {
                let position = Vec3::from(vertex.position);
                out.position = matrix.transform_point3(position).lerp(moved.transform_point3(position), 1.0 - weight);
                out.joints = [node as u8, bend.parent as u8, 0, 0];
                out.weights = [weight, 1.0 - weight, 0.0, 0.0];
            }
        }
    }
    if let Some(multi) = &piece.multi {
        let matrices: Vec<Mat4> = multi
            .groups
            .iter()
            .map(|&(node, offset)| bind.get(node).copied().unwrap_or(matrix) * Mat4::from_translation(offset))
            .collect();
        for ((out, vertex), weights) in bound.iter_mut().zip(&piece.vertices).zip(&multi.weights) {
            let (position, normal) = (Vec3::from(vertex.position), Vec3::from(vertex.normal));
            let (mut joints, mut shares) = ([0u8; 4], [0f32; 4]);
            let mut left = 1.0;
            let (mut blended, mut blended_normal) = (Vec3::ZERO, Vec3::ZERO);
            for (k, matrix) in matrices.iter().enumerate().take(4) {
                let share = weights.get(k).copied().unwrap_or(left);
                left -= share;
                joints[k] = multi.groups[k].0 as u8;
                shares[k] = share;
                blended += matrix.transform_point3(position) * share;
                blended_normal += matrix.transform_vector3(normal) * share;
            }
            out.position = blended;
            out.normal = blended_normal.normalize_or_zero();
            out.joints = joints;
            out.weights = shares;
        }
    }
    let parent_node = piece.parent.min(bind.len() - 1);
    if let Some((blend, skin)) = &piece.skin {
        let parent = &bind[parent_node];
        for vertex in skin {
            let Some(out) = bound.get_mut(vertex.vertex) else { continue };
            if vertex.weight >= 1.0 {
                out.position = parent.transform_point3(vertex.position);
                out.normal = parent.transform_vector3(vertex.normal);
                out.joints = [parent_node as u8, 0, 0, 0];
                out.weights = [1.0, 0.0, 0.0, 0.0];
            } else {
                let (rotation, origin) = blend.frame(&matrix, parent, vertex.weight);
                out.position = rotation * vertex.position + origin;
                out.normal = rotation * vertex.normal;
                out.joints = [node as u8, parent_node as u8, 0, 0];
                out.weights = [1.0 - vertex.weight, vertex.weight, 0.0, 0.0];
            }
        }
    }
    for &(source_piece, source, target) in &piece.welds {
        let (Some(from), Some(out)) = (pieces.get(source_piece), bound.get_mut(target)) else { continue };
        let Some(rest) = from.vertices.get(source) else { continue };
        let source_node = from.node.min(bind.len() - 1);
        out.position = bind[source_node].transform_point3(rest.position.into());
        out.normal = bind[source_node].transform_vector3(rest.normal.into());
        out.joints = [source_node as u8, 0, 0, 0];
        out.weights = [1.0, 0.0, 0.0, 0.0];
    }
    bound
}

/// The binary chunk and the JSON's lists that point into it.
#[derive(Default)]
struct Writer {
    bin: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
}

impl Writer {
    fn view(&mut self, bytes: &[u8]) -> usize {
        while self.bin.len() % 4 != 0 {
            self.bin.push(0);
        }
        self.views.push(json!({"buffer": 0, "byteOffset": self.bin.len(), "byteLength": bytes.len()}));
        self.bin.extend_from_slice(bytes);
        self.views.len() - 1
    }

    fn accessor(&mut self, bytes: &[u8], component: u32, kind: &str, count: usize, bounds: Option<(Vec<f32>, Vec<f32>)>) -> usize {
        let view = self.view(bytes);
        let mut accessor = json!({"bufferView": view, "componentType": component, "type": kind, "count": count});
        if let Some((min, max)) = bounds {
            accessor["min"] = json!(min);
            accessor["max"] = json!(max);
        }
        self.accessors.push(accessor);
        self.accessors.len() - 1
    }

    fn vec3s(&mut self, values: &[Vec3], bounded: bool) -> usize {
        let bounds = bounded.then(|| {
            let min = values.iter().fold(Vec3::splat(f32::MAX), |a, b| a.min(*b));
            let max = values.iter().fold(Vec3::splat(f32::MIN), |a, b| a.max(*b));
            (min.to_array().to_vec(), max.to_array().to_vec())
        });
        let flat: Vec<[f32; 3]> = values.iter().map(|v| v.to_array()).collect();
        self.accessor(bytemuck::cast_slice(&flat), FLOAT, "VEC3", values.len(), bounds)
    }
}

const FLOAT: u32 = 5126;
const UNSIGNED_BYTE: u32 = 5121;
const UNSIGNED_INT: u32 = 5125;

fn png_bytes(texture: &ModelTexture) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, texture.width, texture.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&texture.rgba)?;
    Ok(out)
}

/// Builds the file's bytes.
pub fn glb_bytes(export: &Export) -> Result<Vec<u8>> {
    let Export { model, bind, parents, names, clips, hands } = export;
    let bone_count = bind.len();
    let mut writer = Writer::default();

    // Textures and materials.
    let mut images = Vec::new();
    let mut textures = Vec::new();
    for (i, texture) in model.textures.iter().enumerate() {
        let view = writer.view(&png_bytes(texture).with_context(|| format!("texture {i}"))?);
        images.push(json!({"bufferView": view, "mimeType": "image/png", "name": format!("texture_{i}")}));
        textures.push(json!({"source": i}));
    }
    let has_alpha: Vec<bool> = model.textures.iter().map(|t| t.rgba.chunks_exact(4).any(|p| p[3] < 250)).collect();
    let mut materials: Vec<Value> = Vec::new();
    let mut material_of = |draw: &ExportDraw, translucent: bool| -> usize {
        let diffuse = draw.look.map_or([1.0; 4], |look| look.diffuse);
        let blended = translucent || diffuse[3] < 1.0 || draw.texture.is_some_and(|t| has_alpha.get(t).copied().unwrap_or(false));
        let mut pbr = json!({"baseColorFactor": diffuse, "metallicFactor": 0.0, "roughnessFactor": 1.0});
        let texture = draw.texture.filter(|&t| t < model.textures.len());
        if let Some(t) = texture {
            pbr["baseColorTexture"] = json!({"index": t});
        }
        let material = json!({
            "name": format!("material_{}", texture.map_or("none".to_string(), |t| t.to_string())),
            "pbrMetallicRoughness": pbr,
            "doubleSided": !draw.cull_back,
            "alphaMode": if blended { "BLEND" } else { "OPAQUE" },
        });
        materials.iter().position(|known| *known == material).unwrap_or_else(|| {
            materials.push(material);
            materials.len() - 1
        })
    };

    // Nodes: first the bones, indexed by node, then a node per piece.
    let joints: Vec<Mat4> = bind.iter().map(joint).collect();
    let local = |frame: &[Mat4], node: usize| -> Mat4 {
        match parents[node] {
            Some(parent) => frame[parent].inverse() * frame[node],
            None => frame[node],
        }
    };
    let mut nodes: Vec<Value> = (0..bone_count)
        .map(|node| {
            let (_, rotation, translation) = local(&joints, node).to_scale_rotation_translation();
            let children: Vec<usize> = (0..bone_count).filter(|&c| parents[c] == Some(node)).collect();
            let mut value = json!({
                "name": names[node],
                "translation": translation.to_array(),
                "rotation": rotation.normalize().to_array(),
            });
            if !children.is_empty() {
                value["children"] = json!(children);
            }
            value
        })
        .collect();
    let inverse_binds: Vec<[f32; 16]> = joints.iter().map(|j| j.inverse().to_cols_array()).collect();
    let inverse_binds = writer.accessor(bytemuck::cast_slice(&inverse_binds), FLOAT, "MAT4", bone_count, None);
    let armature = nodes.len();
    let roots: Vec<usize> = (0..bone_count).filter(|&n| parents[n].is_none()).collect();
    nodes.push(json!({"name": "Armature", "children": roots}));

    // Pieces.
    let mut meshes = Vec::new();
    let mut piece_nodes = Vec::new();
    let mut hand_nodes: Vec<Option<(usize, usize)>> = vec![None; hands.len()];
    for (index, piece) in model.pieces.iter().enumerate() {
        if piece.vertices.is_empty() || piece.draws.iter().all(|d| d.indices.is_empty()) {
            continue;
        }
        let bound = bound_vertices(&model.pieces, index, bind);
        let positions: Vec<Vec3> = bound.iter().map(|v| v.position).collect();
        let normals: Vec<Vec3> = bound.iter().map(|v| v.normal.try_normalize().unwrap_or(Vec3::Y)).collect();
        let uvs: Vec<[f32; 2]> = piece.vertices.iter().map(|v| v.uv).collect();
        let vertex_joints: Vec<[u8; 4]> = bound.iter().map(|v| v.joints).collect();
        let weights: Vec<[f32; 4]> = bound.iter().map(|v| v.weights).collect();
        let position = writer.vec3s(&positions, true);
        let normal = writer.vec3s(&normals, false);
        let uv = writer.accessor(bytemuck::cast_slice(&uvs), FLOAT, "VEC2", uvs.len(), None);
        let joint_list = writer.accessor(bytemuck::cast_slice(&vertex_joints), UNSIGNED_BYTE, "VEC4", vertex_joints.len(), None);
        let weight_list = writer.accessor(bytemuck::cast_slice(&weights), FLOAT, "VEC4", weights.len(), None);

        // A hand's shapes are absolute, in the node's space.
        let hand = hands.iter().position(|h| h.piece == index);
        let mut targets = Vec::new();
        let mut default_weights = Vec::new();
        if let Some(hand) = hand.map(|h| hands[h].hand) {
            let matrix = bind[piece.node.min(bone_count - 1)];
            for (t, shape) in hand.shapes.iter().enumerate() {
                let mut moved = vec![Vec3::ZERO; piece.vertices.len()];
                for (group, value) in hand.vertices.iter().zip(shape) {
                    for &vertex in group {
                        if let Some(rest) = piece.vertices.get(vertex) {
                            moved[vertex] = matrix.transform_vector3(Vec3::from(*value) - Vec3::from(rest.position));
                        }
                    }
                }
                targets.push(json!({"POSITION": writer.vec3s(&moved, true)}));
                default_weights.push(if t == 0 { 1.0 } else { 0.0 });
            }
        }

        let mut primitives = Vec::new();
        for draw in piece.draws.iter().filter(|d| !d.indices.is_empty()) {
            let indices = writer.accessor(bytemuck::cast_slice(&draw.indices), UNSIGNED_INT, "SCALAR", draw.indices.len(), None);
            let mut primitive = json!({
                "attributes": {"POSITION": position, "NORMAL": normal, "TEXCOORD_0": uv, "JOINTS_0": joint_list, "WEIGHTS_0": weight_list},
                "indices": indices,
                "material": material_of(draw, piece.translucent),
            });
            if !targets.is_empty() {
                primitive["targets"] = json!(targets);
            }
            primitives.push(primitive);
        }
        let mut mesh = json!({"name": format!("piece_{index}"), "primitives": primitives});
        if !default_weights.is_empty() {
            mesh["weights"] = json!(default_weights);
        }
        meshes.push(mesh);
        nodes.push(json!({"name": format!("piece_{index}"), "mesh": meshes.len() - 1, "skin": 0}));
        piece_nodes.push(nodes.len() - 1);
        if let Some(hand_index) = hand {
            hand_nodes[hand_index] = Some((nodes.len() - 1, hands[hand_index].hand.shapes.len()));
        }
    }
    if meshes.is_empty() {
        bail!("nothing to export");
    }

    // Animations.
    let mut animations = Vec::new();
    for clip in clips.iter() {
        if clip.frames.is_empty() {
            continue;
        }
        let times: Vec<f32> = (0..clip.frames.len()).map(|f| f as f32 / GAME_FRAMES_PER_SECOND).collect();
        let last = times.last().copied().unwrap_or(0.0);
        let time = writer.accessor(bytemuck::cast_slice(&times), FLOAT, "SCALAR", times.len(), Some((vec![0.0], vec![last])));
        let (mut samplers, mut channels) = (Vec::new(), Vec::new());
        let frames: Vec<Vec<Mat4>> = clip.frames.iter().map(|frame| frame.iter().map(joint).collect()).collect();
        for node in 0..bone_count {
            let mut translations = Vec::with_capacity(frames.len());
            let mut rotations: Vec<Quat> = Vec::with_capacity(frames.len());
            for frame in &frames {
                let (_, rotation, translation) = local(frame, node).to_scale_rotation_translation();
                translations.push(translation);
                // Keeps each key on the same side as the one before, to interpolate the short way.
                let rotation = rotation.normalize();
                let rotation = match rotations.last() {
                    Some(before) if before.dot(rotation) < 0.0 => -rotation,
                    _ => rotation,
                };
                rotations.push(rotation);
            }
            // Bones that never move need no keys, which keeps the file and Blender's import small.
            let (still_translation, still_rotation) = local(&joints, node).to_scale_rotation_translation_parts();
            let moves_t = translations.iter().any(|t| t.distance(still_translation) > 1e-5);
            let moves_r = rotations.iter().any(|r| r.dot(still_rotation).abs() < 1.0 - 1e-6);
            if moves_t {
                let output = writer.vec3s(&translations, false);
                samplers.push(json!({"input": time, "output": output, "interpolation": "LINEAR"}));
                channels.push(json!({"sampler": samplers.len() - 1, "target": {"node": node, "path": "translation"}}));
            }
            if moves_r {
                let flat: Vec<[f32; 4]> = rotations.iter().map(|r| r.to_array()).collect();
                let output = writer.accessor(bytemuck::cast_slice(&flat), FLOAT, "VEC4", flat.len(), None);
                samplers.push(json!({"input": time, "output": output, "interpolation": "LINEAR"}));
                channels.push(json!({"sampler": samplers.len() - 1, "target": {"node": node, "path": "rotation"}}));
            }
        }
        for (hand_index, slot) in hand_nodes.iter().enumerate() {
            let Some((node, count)) = *slot else { continue };
            let mut values = Vec::with_capacity(clip.frames.len() * count);
            for states in &clip.hands {
                let (from, to, progress) = states.get(hand_index).copied().unwrap_or((0, 0, 1.0));
                let start = values.len();
                values.resize(start + count, 0.0f32);
                if let Some(value) = values.get_mut(start + from) {
                    *value += 1.0 - progress;
                }
                if let Some(value) = values.get_mut(start + to) {
                    *value += progress;
                }
            }
            let output = writer.accessor(bytemuck::cast_slice(&values), FLOAT, "SCALAR", values.len(), None);
            samplers.push(json!({"input": time, "output": output, "interpolation": "LINEAR"}));
            channels.push(json!({"sampler": samplers.len() - 1, "target": {"node": node, "path": "weights"}}));
        }
        if !channels.is_empty() {
            animations.push(json!({"name": clip.name, "samplers": samplers, "channels": channels}));
        }
    }

    let joint_nodes: Vec<usize> = (0..bone_count).collect();
    let mut scene_nodes = vec![armature];
    scene_nodes.extend(piece_nodes);
    let mut gltf = json!({
        "asset": {"version": "2.0", "generator": "doa_player"},
        "scene": 0,
        "scenes": [{"nodes": scene_nodes}],
        "nodes": nodes,
        "meshes": meshes,
        "skins": [{"joints": joint_nodes, "inverseBindMatrices": inverse_binds, "skeleton": armature}],
        "materials": materials,
        "accessors": writer.accessors,
        "bufferViews": writer.views,
        "buffers": [{"byteLength": writer.bin.len()}],
    });
    if !images.is_empty() {
        gltf["images"] = json!(images);
        gltf["textures"] = json!(textures);
    }
    if !animations.is_empty() {
        gltf["animations"] = json!(animations);
    }

    let mut text = serde_json::to_vec(&gltf)?;
    while text.len() % 4 != 0 {
        text.push(b' ');
    }
    let mut bin = writer.bin;
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    let total = 12 + 8 + text.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(text.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&text);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&bin);
    Ok(out)
}

/// The motions' time base in Blender's timeline.
const GAME_FRAMES_PER_SECOND: f32 = 60.0;

trait Parts {
    fn to_scale_rotation_translation_parts(&self) -> (Vec3, Quat);
}

impl Parts for Mat4 {
    fn to_scale_rotation_translation_parts(&self) -> (Vec3, Quat) {
        let (_, rotation, translation) = self.to_scale_rotation_translation();
        (translation, rotation.normalize())
    }
}

pub fn write_glb(export: &Export, path: &Path) -> Result<()> {
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder).with_context(|| format!("creating {}", folder.display()))?;
    }
    let bytes = glb_bytes(export)?;
    std::fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))
}

/// A name that is safe as a file or folder name.
pub fn safe_name(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pose(angle: f32, height: f32) -> [Mat4; NODE_COUNT] {
        // A solve's nodes carry the flip on the left.
        let flip = Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0));
        let mut nodes = [flip; NODE_COUNT];
        nodes[2] = flip * Mat4::from_translation(Vec3::new(0.1, height, 0.2)) * Mat4::from_rotation_y(angle);
        nodes[0] = nodes[2] * Mat4::from_translation(Vec3::Y) * Mat4::from_rotation_x(angle);
        nodes
    }

    fn skin(vertex: &BoundVertex, bind: &[Mat4], now: &[Mat4]) -> Vec3 {
        (0..4)
            .map(|i| {
                let node = vertex.joints[i] as usize;
                (joint(&now[node]) * joint(&bind[node]).inverse()).transform_point3(vertex.position) * vertex.weights[i]
            })
            .sum()
    }

    fn piece(node: usize) -> ExportPiece {
        ExportPiece {
            node,
            parent: node,
            vertices: vec![ModelVertex { position: [0.3, 0.2, -0.1], normal: [0.0, 1.0, 0.0], uv: [0.0, 0.0] }],
            draws: vec![ExportDraw { indices: vec![0, 0, 0], texture: None, cull_back: true, look: None }],
            skin: None,
            bend: None,
            multi: None,
            welds: Vec::new(),
            translucent: false,
        }
    }

    #[test]
    fn skinning_gives_the_viewer_s_vertices() {
        let (bind, now) = (pose(0.4, 1.0), pose(1.3, 1.5));
        let pieces = [piece(0)];
        let bound = bound_vertices(&pieces, 0, &bind);
        let viewer = now[0].transform_point3(pieces[0].vertices[0].position.into());
        assert!(skin(&bound[0], &bind, &now).distance(viewer) < 1e-5);
    }

    #[test]
    fn joints_are_proper_rotations() {
        for node in pose(0.7, 1.0).iter() {
            assert!(joint(node).determinant() > 0.0);
        }
    }

    #[test]
    fn welded_vertices_follow_their_source() {
        let (bind, now) = (pose(0.4, 1.0), pose(1.3, 1.5));
        let mut target = piece(2);
        target.welds = vec![(0, 0, 0)];
        let pieces = [piece(0), target];
        let bound = bound_vertices(&pieces, 1, &bind);
        let viewer = now[0].transform_point3(pieces[0].vertices[0].position.into());
        assert!(skin(&bound[0], &bind, &now).distance(viewer) < 1e-5);
    }

    #[test]
    fn writes_a_valid_container() {
        let model = ExportModel {
            pieces: vec![piece(0)],
            textures: vec![ModelTexture { width: 1, height: 1, rgba: vec![255, 0, 0, 255] }],
        };
        let clips = [ExportClip { name: "motion".into(), frames: vec![pose(0.1, 1.0).to_vec(), pose(0.2, 1.1).to_vec()], hands: vec![Vec::new(); 2] }];
        let bytes = glb_bytes(&Export { model: &model, bind: pose(0.0, 1.0).to_vec(), parents: standard_skeleton().0, names: standard_skeleton().1, clips: &clips, hands: &[] }).unwrap();
        assert_eq!(&bytes[..4], b"glTF");
        assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize, bytes.len());
        let length = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let gltf: Value = serde_json::from_slice(&bytes[20..20 + length]).unwrap();
        assert_eq!(gltf["animations"][0]["name"], "motion");
        assert_eq!(gltf["skins"][0]["joints"].as_array().unwrap().len(), NODE_COUNT);
        assert_eq!(gltf["meshes"].as_array().unwrap().len(), 1);
    }
}
