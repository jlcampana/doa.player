use crate::formats::config::CharacterConfigs;
use std::collections::HashMap;

use crate::formats::doa2u;
use crate::formats::doa3::{self, ChestMorphGroup, ChestSpring, MotionTracks, RosterEntry, Xbe};
use crate::formats::doa3_chains::{self, SoftChain};
use crate::formats::doa3_cloth::{self, SoftCloth};
use crate::formats::doa3_morphs::{self, FaceMorph, HandEvent, HandMorph};
use crate::formats::doa3_springs::{self, SoftSprings};
use crate::formats::doa3_tubes::{self, SoftTube};
use crate::formats::doa3_wind::Wind;
use crate::formats::model::{parse_xpr_model, parse_xpr_textures};
use crate::formats::AfsArchive;
use crate::renderer::{Camera, RenderContext};
use crate::renderer::grid::GridRenderer;
use crate::renderer::mesh::{MeshRenderer, PieceInput, VertexBend};
use crate::renderer::wireframe::{WireframeRenderer, LineVertex};
use crate::ui::{AnimPanel, BgSelectPanel, CharSelectPanel};
use std::path::{Path, PathBuf};

// ─── Data model ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum GameSource {
    Doa2U,
    Doa3,
}

#[derive(Debug, Clone)]
pub struct CharacterEntry {
    /// Short ID used in .cat/.xpr filenames (e.g. "kas", "zac")
    pub id: String,
    /// Full ID used in .bin/.mot filenames in datahdd.afs (e.g. "kasumi", "zack")
    pub bin_id: String,
    pub display_name: String,
    pub costumes: Vec<String>,
    /// DOA3 only: AFS entry of each costume's model (empty for DOA2U)
    pub afs_indices: Vec<usize>,
    /// DOA3 only: character tables from the game executable
    pub doa3: Option<RosterEntry>,
    /// DOA2U only: the character's ID in the game's tables
    pub doa2u_id: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct AnimationEntry {
    pub name: String,
    pub file_key: String, // e.g. "zack.mot"
    /// DOA3 only: AFS entry of the motion file and the motion's tracks
    pub doa3_motion: Option<(usize, MotionTracks)>,
    pub doa3_motion_id: usize,
}

#[derive(Debug, Clone)]
pub struct BackgroundOption {
    pub name: String,
    pub clear_color: [f32; 4],
}

// ─── App paths ────────────────────────────────────────────────────────────────

const DOA2U_XBE_PATH: &str =
    "/home/pandi/Ports/Apps/DeadOrAlive2Ultimate-Doa2uRe/private/play-disc/DOA2.xbe";
const DOA2U_HDD_PATH: &str =
    "/home/pandi/Ports/Apps/DeadOrAlive2Ultimate-Doa2uRe/private/play-disc/DOA2/datahdd.afs";
const DOA2U_HDD2_PATH: &str =
    "/home/pandi/Ports/Apps/DeadOrAlive2Ultimate-Doa2uRe/private/play-disc/DOA2/datahdd2.afs";
const DOA2U_DVD_PATH: &str =
    "/home/pandi/Ports/Apps/DeadOrAlive2Ultimate-Doa2uRe/private/play-disc/DOA2/datadvd.afs";
const DOA3_LOAD_PATH: &str = "doa3/loadfile.afs";
const DOA3_XBE_PATH: &str = "doa3/default.xbe";
const CHARACTERS_CONFIG: &str = "characters.toml";
/// Passed as the costume to use the character's configured default.
pub const DEFAULT_COSTUME: usize = usize::MAX;
/// Most soft body frames simulated at once, so a long pause does not stall.
const MAX_CHAIN_CATCH_UP: u64 = 10;

/// Finds a file in the project folder, searching like `asset_path`.
fn project_file(name: &str) -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    let cwd = std::env::current_dir().ok();
    exe_dir
        .iter()
        .chain(cwd.iter())
        .flat_map(|start| start.ancestors())
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| PathBuf::from(name))
}

/// Finds a file inside the project's `assets` folder, searching upwards from
/// the executable (`target/release/..`) and then from the working directory.
fn asset_path(relative: &str) -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    let cwd = std::env::current_dir().ok();

    exe_dir
        .iter()
        .chain(cwd.iter())
        .flat_map(|start| start.ancestors())
        .map(|dir| dir.join("assets").join(relative))
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| Path::new("assets").join(relative))
}

// ─── DOA2U character roster ───────────────────────────────────────────────────

fn doa2u_characters(costume_files: Option<&AfsArchive>) -> Vec<CharacterEntry> {
    // (file prefix of the costumes, name of the .bin and .mot files, shown name,
    // character ID, costumes on the select screen, from the counts at 0x6E0F08).
    // IDs are from the counts and the order of the file names at 0x694288.
    let roster = [
        ("kas", "kasumi",  "Kasumi",           5, 20),
        ("aya", "ayane",   "Ayane",           13, 20),
        ("zac", "zack",    "Zack",             0,  8),
        ("tin", "tina",    "Tina Armstrong",   1, 16),
        ("ryu", "ryu",     "Ryu Hayabusa",     4,  9),
        ("lei", "leifang", "Lei Fang",        12, 20),
        ("jan", "janlee",  "Jann Lee",         2,  8),
        ("bas", "bass",    "Bass Armstrong",   9,  8),
        // "sambo" is Bayman and "bayman" is Leon. Bayman's ID and files were
        // measured in a running match. Leon's follow, and from the costume counts.
        ("bay", "sambo",   "Bayman",          24,  6),
        ("gen", "genfu",   "Gen Fu",           6,  8),
        ("leo", "bayman",  "Leon",             8,  7),
        ("kor", "koryu",   "Hitomi",          21, 14),
        ("tng", "tengu",   "Tengu",           17,  3),
        ("ein", "karate",  "Ein",              3,  7),
        ("hel", "hika",    "Helena Douglas",   7, 13),
    ];
    roster
        .iter()
        .map(|(cat_id, bin_id, name, id, costume_count)| CharacterEntry {
            id: cat_id.to_string(),
            bin_id: bin_id.to_string(),
            display_name: name.to_string(),
            // File numbers have gaps, and placeholders repeat the first name.
            costumes: costume_files.map_or_else(Vec::new, |archive| {
                let mut stems: Vec<String> = Vec::new();
                for entry in archive.entries() {
                    let Some(stem) = entry.name.strip_suffix(".xpr") else { continue };
                    if stem.starts_with(cat_id) && !stems.iter().any(|known| known == stem) {
                        stems.push(stem.to_string());
                    }
                }
                stems.truncate(*costume_count);
                stems
            }),
            afs_indices: vec![],
            doa3: None,
            doa2u_id: Some(*id),
        })
        .collect()
}

/// Builds the DOA3 character list. A costume is every model between a
/// character's BIN entry and the next character's.
pub fn doa3_characters(xbe: &Xbe, archive: &AfsArchive) -> Vec<CharacterEntry> {
    let roster = match xbe.roster() {
        Ok(roster) => roster,
        Err(e) => {
            log::warn!("Could not read the DOA3 roster: {:#}", e);
            return Vec::new();
        }
    };

    let mut starts: Vec<usize> = roster.iter().map(|r| r.bin_entry).collect();
    starts.sort_unstable();

    roster
        .into_iter()
        .map(|entry| {
            let end = starts
                .iter()
                .copied()
                .find(|&start| start > entry.bin_entry)
                .unwrap_or(archive.entries().len());
            let models: Vec<usize> = (entry.bin_entry..end)
                .filter(|&i| {
                    let is_model = archive.get_by_index(i).is_some_and(|d| {
                        d.get(..4) == Some(b"XPR0") && d.get(0x14..0x18) == Some(b"MDL\0")
                    });
                    // Stage models also use MDL blocks, but have no CAT after them.
                    is_model && archive.get_by_index(i + 1).is_some_and(|cat| doa3::parse_node_objects(cat).is_ok())
                })
                .collect();
            let selectable = xbe.selectable_costumes(entry.id);
            CharacterEntry {
                id: entry.name.to_string(),
                bin_id: String::new(),
                display_name: entry.name.to_string(),
                costumes: (0..models.len())
                    .map(|i| {
                        let unused = selectable.as_ref().is_some_and(|list| !list.contains(&i));
                        format!("Costume {:02}{}", i, if unused { " (not in game)" } else { "" })
                    })
                    .collect(),
                afs_indices: models,
                doa2u_id: None,
                doa3: Some(entry),
            }
        })
        .collect()
}

fn doa3_animations(entry: &RosterEntry, archive: &AfsArchive) -> Vec<AnimationEntry> {
    let mut animations = Vec::new();
    for (file, &afs_entry) in entry.motion_files.iter().enumerate() {
        let Some(data) = archive.get_by_index(afs_entry) else { continue };
        for (id, tracks) in doa3::list_motions(data) {
            animations.push(AnimationEntry {
                name: format!("{} {:04} ({} frames)", ["A", "B"][file], id, doa3::motion_length(data, &tracks)),
                file_key: String::new(),
                doa3_motion: Some((afs_entry, tracks)),
                doa3_motion_id: id,
            });
        }
    }
    animations
}

fn doa2u_animations(bin_id: &str, archive: &AfsArchive) -> Vec<AnimationEntry> {
    // The game shortens Lei Fang's motion files to six letters.
    let short = &bin_id[..bin_id.len().min(6)];
    let files = [format!("{short}.mot"), format!("m_{short}.mot"), format!("c_{short}.mot"), "comon.mot".to_string()];
    let mut animations = Vec::new();
    for file in &files {
        let label = file.trim_end_matches(".mot");
        let Some(afs_entry) = archive.entries().iter().position(|e| e.name == *file) else { continue };
        let Some(data) = archive.get_by_index(afs_entry) else { continue };
        for (id, tracks) in doa3::list_motions(data) {
            animations.push(AnimationEntry {
                name: format!("{label} {id:04} ({} frames)", doa3::motion_length(data, &tracks)),
                file_key: file.clone(),
                doa3_motion: Some((afs_entry, tracks)),
                doa3_motion_id: id,
            });
        }
    }
    animations
}

fn background_options() -> Vec<BackgroundOption> {
    vec![
        BackgroundOption { name: "Black".into(),         clear_color: [0.0, 0.0, 0.0, 1.0] },
        BackgroundOption { name: "Dark Gray".into(),     clear_color: [0.1, 0.1, 0.1, 1.0] },
        BackgroundOption { name: "Navy".into(),          clear_color: [0.05, 0.07, 0.15, 1.0] },
        BackgroundOption { name: "Forest".into(),        clear_color: [0.03, 0.12, 0.05, 1.0] },
        BackgroundOption { name: "Crimson".into(),       clear_color: [0.15, 0.02, 0.02, 1.0] },
        BackgroundOption { name: "White".into(),         clear_color: [0.9, 0.9, 0.9, 1.0] },
        BackgroundOption { name: "Stage: Desert".into(), clear_color: [0.55, 0.45, 0.25, 1.0] },
        BackgroundOption { name: "Stage: Snow".into(),   clear_color: [0.7, 0.8, 0.9, 1.0] },
    ]
}

fn load_character_configs() -> CharacterConfigs {
    let path = project_file(CHARACTERS_CONFIG);
    let Ok(text) = std::fs::read_to_string(&path) else {
        log::info!("No {} found, using defaults", path.display());
        return CharacterConfigs::default();
    };
    CharacterConfigs::parse(&text).unwrap_or_else(|e| {
        log::warn!("Could not read {}: {}", path.display(), e);
        CharacterConfigs::default()
    })
}

// ─── App state ────────────────────────────────────────────────────────────────

pub struct App {
    pub ctx: RenderContext,
    pub camera: Camera,

    // Renderers
    pub grid: GridRenderer,
    pub mesh: MeshRenderer,
    pub wireframe: WireframeRenderer,

    // egui
    pub egui_ctx: egui::Context,
    pub egui_state: egui_winit::State,
    pub egui_renderer: egui_wgpu::Renderer,

    // UI panels
    pub char_panel: CharSelectPanel,
    pub anim_panel: AnimPanel,
    pub bg_panel: BgSelectPanel,

    // Data
    pub doa2u_characters: Vec<CharacterEntry>,
    pub doa3_characters: Vec<CharacterEntry>,
    pub current_animations: Vec<AnimationEntry>,
    pub background_options: Vec<BackgroundOption>,

    // Archives (lazily loaded)
    pub hdd_archive: Option<AfsArchive>,
    pub hdd2_archive: Option<AfsArchive>,
    pub dvd_archive: Option<AfsArchive>,
    pub loadfile_archive: Option<AfsArchive>,
    pub doa3_xbe: Option<Xbe>,
    pub doa2u_xbe: Option<Xbe>,
    pub character_configs: CharacterConfigs,
    /// Game frames elapsed, for systems that run at the game's fixed 60 Hz.
    pub game_clock: f32,

    // Input state
    pub mouse_drag: Option<(f64, f64)>,
    pub right_drag: Option<(f64, f64)>,
    pub status_msg: String,

    // Animation state, in frames of the game's 60 Hz clock
    pub anim_time: f32,
    pub doa3_pose: Option<Doa3Pose>,
    /// DOA3 character the camera was last framed on.
    framed_character: Option<usize>,
}

/// What is needed to pose the loaded DOA3 character.
pub struct Doa3Pose {
    /// Set for DOA2U costumes, which have their own solve.
    pub doa2u_style: Option<doa2u::PoseStyle>,
    pub skeleton: [glam::Vec3; 17],
    pub motion: Option<Doa3Motion>,
    pub chest: Option<Doa3Chest>,
    pub chains: Doa3Chains,
}

/// Hair, ribbons and hanging cloth.
pub struct Doa3Chains {
    pub chains: Vec<SoftChain>,
    pub cloth: Vec<SoftCloth>,
    pub tubes: Vec<SoftTube>,
    pub springs: Vec<SoftSprings>,
    /// Input piece and its node for each object, by object index.
    pub pieces: Vec<Option<(usize, usize)>>,
    /// Game frames already simulated.
    pub frame: u64,
    pub wind: Wind,
    pub hands: Vec<HandMorph>,
    pub face: Option<FaceMorph>,
    /// Hand poses of each motion, by motion ID.
    pub hand_events: HashMap<u16, Vec<HandEvent>>,
    pub character: usize,
    /// DOA2U's hands, right one first.
    pub pose_hands: Vec<doa3_morphs::PoseHand>,
    /// DOA2U: objects whose positions follow another object.
    pub copies: Vec<doa2u::VertexCopy>,
}

pub struct Doa3Chest {
    /// Input piece of the chest in the mesh renderer.
    pub piece: usize,
    pub groups: Vec<ChestMorphGroup>,
    pub spring: Option<ChestSpring>,
    /// Game frames already simulated.
    pub frame: u64,
}

pub struct Doa3Motion {
    pub afs_entry: usize,
    pub tracks: MotionTracks,
    pub length: f32,
    pub id: usize,
}

impl App {
    pub fn new(
        ctx: RenderContext,
        egui_ctx: egui::Context,
        egui_state: egui_winit::State,
        _window: &winit::window::Window,
    ) -> Self {
        let w = ctx.surface_config.width;
        let h = ctx.surface_config.height;
        let fmt = ctx.surface_format();

        let grid = GridRenderer::new(&ctx.device, fmt);
        let mesh = MeshRenderer::new(&ctx.device, &ctx.queue, fmt);
        let wireframe = WireframeRenderer::new(&ctx.device, fmt, 8192);
        let egui_renderer = egui_wgpu::Renderer::new(&ctx.device, fmt, None, 1, false);

        let mut camera = Camera::new(w as f32 / h as f32);
        camera.frame_character(1.8);

        let background_options = background_options();
        let hdd2_archive = match AfsArchive::from_file(DOA2U_HDD2_PATH) {
            Ok(a) => Some(a),
            Err(e) => {
                log::warn!("Could not load DOA2U datahdd2.afs: {}", e);
                None
            }
        };
        let doa2u_characters = doa2u_characters(hdd2_archive.as_ref());

        let hdd_archive = match AfsArchive::from_file(DOA2U_HDD_PATH) {
            Ok(a) => {
                log::info!("Loaded DOA2U datahdd.afs: {} files", a.entries().len());
                Some(a)
            }
            Err(e) => {
                log::warn!("Could not load DOA2U datahdd.afs: {}", e);
                None
            }
        };

        let doa3_path = asset_path(DOA3_LOAD_PATH);
        let loadfile_archive = match AfsArchive::from_file(&doa3_path) {
            Ok(a) => {
                log::info!("Loaded {}: {} files", doa3_path.display(), a.entries().len());
                Some(a)
            }
            Err(e) => {
                log::warn!("Could not load {}: {}", doa3_path.display(), e);
                None
            }
        };

        let doa3_xbe = match std::fs::read(asset_path(DOA3_XBE_PATH))
            .map_err(anyhow::Error::from)
            .and_then(Xbe::from_bytes)
        {
            Ok(xbe) => Some(xbe),
            Err(e) => {
                log::warn!("Could not load DOA3 default.xbe: {:#}", e);
                None
            }
        };
        let doa3_characters = match (&doa3_xbe, &loadfile_archive) {
            (Some(xbe), Some(archive)) => doa3_characters(xbe, archive),
            _ => Vec::new(),
        };

        Self {
            ctx,
            camera,
            grid,
            mesh,
            wireframe,
            egui_ctx,
            egui_state,
            egui_renderer,
            char_panel: CharSelectPanel::new(),
            anim_panel: AnimPanel::new(),
            bg_panel: BgSelectPanel::new(),
            doa2u_characters,
            doa3_characters,
            current_animations: Vec::new(),
            background_options,
            hdd_archive,
            hdd2_archive,
            dvd_archive: None,
            loadfile_archive,
            doa3_xbe,
            doa2u_xbe: None,
            character_configs: load_character_configs(),
            game_clock: 0.0,
            mouse_drag: None,
            right_drag: None,
            status_msg: "Ready. Select a character to begin.".into(),
            anim_time: 0.0,
            doa3_pose: None,
            framed_character: None,
        }
    }

    pub fn load_character(&mut self, char_idx: usize, costume_idx: usize) {
        match self.char_panel.selected_source {
            GameSource::Doa2U => self.load_doa2u_character(char_idx, costume_idx),
            GameSource::Doa3  => self.load_doa3_character(char_idx, costume_idx),
        }
    }

    fn load_doa2u_character(&mut self, char_idx: usize, costume_idx: usize) {
        match self.try_load_doa2u_character(char_idx, costume_idx) {
            Ok(status) => self.status_msg = status,
            Err(e) => {
                log::warn!("Could not load DOA2U character: {:#}", e);
                self.status_msg = format!("DOA2U: could not load character: {:#}", e);
                self.mesh.clear_model();
                self.doa3_pose = None;
                // Without a model, show the collision capsules from the character's .bin.
                let bin_id = self.doa2u_characters.get(char_idx).map(|c| c.bin_id.clone());
                if let Some(bin_id) = bin_id {
                    let lines = self.build_skeleton_lines(&bin_id);
                    self.wireframe.update_lines(&self.ctx.queue, &lines);
                }
            }
        }
    }

    fn try_load_doa2u_character(&mut self, char_idx: usize, costume_idx: usize) -> anyhow::Result<String> {
        use anyhow::Context;

        let costume_idx = if costume_idx == DEFAULT_COSTUME { 0 } else { costume_idx };
        self.char_panel.selected_costume_idx = costume_idx;
        let entry = self.doa2u_characters.get(char_idx).context("unknown character")?;
        let display_name = entry.display_name.clone();
        let bin_id = entry.bin_id.clone();
        let id = entry.doa2u_id.context("character has no ID")?;

        if self.doa2u_xbe.is_none() {
            self.doa2u_xbe = Some(Xbe::from_bytes(std::fs::read(DOA2U_XBE_PATH)?)?);
        }
        let archive = self.hdd2_archive.as_ref().context("datahdd2.afs not loaded")?;
        let xbe = self.doa2u_xbe.as_ref().context("DOA2.xbe not loaded")?;
        let stem = entry.costumes.get(costume_idx).context("unknown costume")?.clone();
        let mut xpr = archive.get_by_name(&format!("{stem}.xpr")).with_context(|| format!("no {stem}.xpr"))?.to_vec();
        let mut cat = archive.get_by_name(&format!("{stem}.cat")).with_context(|| format!("no {stem}.cat"))?.to_vec();
        doa2u::decrypt_xpr(&mut xpr)?;
        doa2u::decrypt_cat(&mut cat)?;

        let mut model = doa2u::parse_model(&xpr)?;
        let textures = parse_xpr_textures(&xpr)?;
        let skeleton = doa2u::skeleton(xbe, id)?;
        let drawn = doa2u::parse_drawn_objects(&cat, &skeleton)?;
        let extras = doa2u::extra_objects(&cat, model.pieces.len())?;
        for extra in &extras {
            let matrix = extra.prop.matrix();
            for vertex in &mut model.pieces[extra.prop.object].vertices {
                vertex.position = matrix.transform_point3(vertex.position.into()).into();
                vertex.normal = matrix.transform_vector3(vertex.normal.into()).into();
            }
        }
        let style = doa2u::pose_style(&cat)?;

        let skins = doa2u::joint_skins(&cat)?;

        let texture_count = textures.len();
        fn draws_of(piece: &crate::formats::model::ModelPiece, texture_count: usize) -> Vec<(&[u32], Option<usize>, bool)> {
            piece
                .materials
                .iter()
                .map(|m| (m.indices.as_slice(), m.base_texture(texture_count), m.cull_back))
                .collect()
        }
        let mut inputs: Vec<PieceInput> = drawn
            .iter()
            .filter_map(|placed| {
                let piece = model.pieces.get(placed.object)?;
                let bend = placed.bend.zip(model.weights[placed.object].clone()).map(|(bend, weights)| VertexBend {
                    parent: bend.parent,
                    offset: bend.offset,
                    weights,
                });
                Some(PieceInput {
                    node: placed.node,
                    vertices: &piece.vertices,
                    draws: draws_of(piece, texture_count),
                    parent: placed.node,
                    skin: None,
                    bend,
                    welds: Vec::new(),
                    translucent: false,
                })
            })
            .collect();
        let rigid_count = inputs.len();
        let cloth = doa2u::soft_cloth(&cat, &model, id)?;
        let rigid_objects: Vec<usize> =
            drawn.iter().map(|placed| placed.object).filter(|&o| o < model.pieces.len()).collect();
        let mut drawn_nodes: Vec<(usize, usize)> =
            drawn.iter().filter(|p| p.object < model.pieces.len()).map(|p| (p.object, p.node)).collect();
        for skin in &skins {
            let Some(piece) = model.pieces.get(skin.object) else { continue };
            drawn_nodes.push((skin.object, skin.node));
            let welds = skin
                .welds
                .iter()
                .filter_map(|weld| Some((rigid_objects.iter().position(|&o| o == weld.object)?, weld)))
                .flat_map(|(input, weld)| {
                    weld.links.iter().flat_map(move |(source, targets)| {
                        targets.iter().map(move |&target| (input, *source, target))
                    })
                })
                .filter(|&(input, source, target)| {
                    source < model.pieces[rigid_objects[input]].vertices.len() && target < piece.vertices.len()
                })
                .collect();
            inputs.push(PieceInput {
                node: skin.node,
                vertices: &piece.vertices,
                draws: draws_of(piece, texture_count),
                parent: skin.parent,
                skin: Some((skin.blend, skin.vertices.as_slice())),
                bend: None,
                welds,
                translucent: false,
            });
        }
        log::info!("DOA2U: {stem}: {rigid_count} placed objects, {} joint pieces", skins.len());
        // Soft bodies write into the first piece that draws their object. A detached
        // object is only drawn through its chain.
        let chains = doa2u::soft_chains(&cat, &model, id)?;
        let mut chain_pieces: Vec<Option<(usize, usize)>> = vec![None; model.pieces.len()];
        for (input, (object, node)) in drawn_nodes.iter().enumerate() {
            chain_pieces[*object].get_or_insert((input, *node));
        }
        for chain in &chains {
            for target in chain.targets.iter().filter(|t| t.detached) {
                if chain_pieces.get(target.object) != Some(&None) {
                    continue;
                }
                let piece = &model.pieces[target.object];
                chain_pieces[target.object] = Some((inputs.len(), chain.bone));
                inputs.push(PieceInput {
                    node: chain.bone,
                    vertices: &piece.vertices,
                    draws: draws_of(piece, texture_count),
                    parent: chain.bone,
                    skin: None,
                    bend: None,
                    welds: Vec::new(),
                    translucent: false,
                });
            }
        }
        for extra in &extras {
            if chain_pieces[extra.prop.object].is_some() {
                continue;
            }
            let piece = &model.pieces[extra.prop.object];
            log::info!("Object {} on bone {}, translucent: {}", extra.prop.object, extra.prop.bone, extra.translucent);
            chain_pieces[extra.prop.object] = Some((inputs.len(), extra.prop.bone));
            inputs.push(PieceInput {
                node: extra.prop.bone,
                vertices: &piece.vertices,
                draws: draws_of(piece, texture_count),
                parent: extra.prop.bone,
                skin: None,
                bend: None,
                welds: Vec::new(),
                translucent: extra.translucent,
            });
        }
        let triangles: usize = inputs.iter().flat_map(|p| &p.draws).map(|(t, _, _)| t.len() / 3).sum();
        let piece_count = inputs.len();
        self.mesh.set_model(&self.ctx.device, &self.ctx.queue, &inputs, &textures);
        drop(inputs);
        for chain in &chains {
            for target in chain.targets.iter().filter(|t| t.detached) {
                let Some(&Some((piece, _))) = chain_pieces.get(target.object) else { continue };
                let rest: Vec<glam::Vec3> =
                    model.pieces[target.object].vertices.iter().map(|v| glam::Vec3::from(v.position)).collect();
                self.mesh.set_rest_positions(piece, chain.anchored_vertices(target, &rest));
            }
        }

        self.current_animations = self.hdd_archive.as_ref().map_or_else(Vec::new, |a| doa2u_animations(&bin_id, a));
        self.wireframe.update_lines(&self.ctx.queue, &[]);
        if self.framed_character != Some(char_idx) {
            self.camera.frame_character(1.8);
            self.framed_character = Some(char_idx);
        }
        let chest_morph = doa2u::chest_morph(&cat, id)?;
        let chain_piece_of_chest = chest_morph.as_ref().and_then(|(object, _)| chain_pieces.get(*object).copied().flatten()).map(|(piece, _)| piece);
        let chains = Doa3Chains {
            chains,
            cloth,
            tubes: Vec::new(),
            springs: Vec::new(),
            pieces: chain_pieces,
            frame: 0,
            wind: Wind::new(),
            hands: Vec::new(),
            face: doa2u::face(&cat)?,
            // The scripts use DOA3's format. The motions are looked up by ID.
            hand_events: self.hdd_archive.as_ref().and_then(|a| a.get_by_name(&format!("{bin_id}.bin"))).map(doa3_morphs::parse_hand_events).unwrap_or_default(),
            character: id,
            pose_hands: doa2u::hands(&cat)?,
            copies: doa2u::vertex_copies(&cat, &xpr)?,
        };
        let chest = chest_morph.and_then(|(object, groups)| {
            let piece = chain_piece_of_chest?;
            log::info!("Chest spring on object {object}, {} groups", groups.len());
            Some(Doa3Chest { piece, groups, spring: None, frame: 0 })
        });
        self.doa3_pose = Some(Doa3Pose { doa2u_style: Some(style), skeleton, motion: None, chest, chains });
        self.game_clock = 0.0;
        self.select_motion(0);

        Ok(format!(
            "DOA2U: {} — {}: {} pieces, {} triangles, {} textures, {} motions",
            display_name,
            stem,
            piece_count,
            triangles,
            textures.len(),
            self.current_animations.len()
        ))
    }

    /// The archive that holds the current character's motion files.
    fn motion_archive(&self) -> Option<&AfsArchive> {
        match self.char_panel.selected_source {
            GameSource::Doa2U => self.hdd_archive.as_ref(),
            GameSource::Doa3 => self.loadfile_archive.as_ref(),
        }
    }

    fn load_doa3_character(&mut self, char_idx: usize, costume_idx: usize) {
        match self.try_load_doa3_character(char_idx, costume_idx) {
            Ok(status) => self.status_msg = status,
            Err(e) => {
                log::warn!("Could not load DOA3 character: {:#}", e);
                self.status_msg = format!("DOA3: could not load character: {:#}", e);
                self.mesh.clear_model();
                self.doa3_pose = None;
            }
        }
    }

    fn try_load_doa3_character(&mut self, char_idx: usize, costume_idx: usize) -> anyhow::Result<String> {
        use anyhow::Context;

        let entry = self.doa3_characters.get(char_idx).context("unknown character")?;
        let display_name = entry.display_name.clone();
        let config = self.character_configs.get(&display_name);
        let costume_idx = if costume_idx == DEFAULT_COSTUME {
            config.costume.filter(|&c| c < entry.afs_indices.len()).unwrap_or(0)
        } else {
            costume_idx
        };
        self.char_panel.selected_costume_idx = costume_idx;
        let roster = entry.doa3.clone().context("character has no DOA3 data")?;
        let model_entry = *entry.afs_indices.get(costume_idx).context("unknown costume")?;

        let archive = self.loadfile_archive.as_ref().context("loadfile.afs not loaded")?;
        let xbe = self.doa3_xbe.as_ref().context("default.xbe not loaded")?;
        let xpr = archive.get_by_index(model_entry).context("model entry missing")?;
        let cat = archive.get_by_index(model_entry + 1).context("CAT entry missing")?;

        let mut pieces = parse_xpr_model(xpr)?;
        let textures = parse_xpr_textures(xpr)?;
        let nodes = doa3::parse_node_objects(cat)?;
        doa3_morphs::apply_base_shapes(cat, &nodes.rigid, &mut pieces);
        // Worn objects are drawn on their bone, moved to their place.
        let grouped = doa3::grouped_objects(cat);
        let props: Vec<_> = doa3_chains::parse_props(cat, pieces.len())
            .into_iter()
            .filter(|prop| !grouped.contains(&prop.object))
            .collect();
        let overlays: Vec<_> = doa3_chains::parse_overlays(cat, pieces.len())
            .into_iter()
            .filter(|overlay| !grouped.contains(&overlay.prop.object))
            .collect();
        for prop in props.iter().chain(overlays.iter().map(|overlay| &overlay.prop)) {
            let matrix = prop.matrix();
            for vertex in &mut pieces[prop.object].vertices {
                vertex.position = matrix.transform_point3(vertex.position.into()).into();
                vertex.normal = matrix.transform_vector3(vertex.normal.into()).into();
            }
        }
        let skeleton = xbe.skeleton(roster.id)?;
        let animations = doa3_animations(&roster, archive);

        let draws_of = |object: usize| {
            pieces.get(object).map(|piece| {
                let draws = piece
                    .materials
                    .iter()
                    .map(|m| (m.indices.as_slice(), m.base_texture(textures.len()), m.cull_back))
                    .collect::<Vec<_>>();
                (piece.vertices.as_slice(), draws)
            })
        };
        let rigid = nodes.rigid.iter().filter_map(|&(node, object)| {
            let (vertices, draws) = draws_of(object)?;
            Some(PieceInput { node, vertices, draws, parent: node, skin: None, welds: Vec::new(), translucent: false, bend: None })
        });
        let rigid_input = |object: usize| {
            nodes.rigid.iter().filter(|&&(_, o)| o < pieces.len()).position(|&(_, o)| o == object)
        };
        let skins = nodes.skins.iter().filter_map(|skin| {
            let (vertices, draws) = draws_of(skin.object)?;
            let welds = skin
                .welds
                .iter()
                .filter_map(|weld| Some((rigid_input(weld.object)?, weld)))
                .flat_map(|(input, weld)| {
                    weld.links.iter().flat_map(move |(source, targets)| {
                        targets.iter().map(move |&target| (input, *source, target))
                    })
                })
                .filter(|&(input, source, target)| {
                    let source_count = nodes
                        .rigid
                        .iter()
                        .filter(|&&(_, o)| o < pieces.len())
                        .nth(input)
                        .map_or(0, |&(_, o)| pieces[o].vertices.len());
                    source < source_count && target < vertices.len()
                })
                .collect();
            Some(PieceInput {
                node: skin.node,
                vertices,
                draws,
                parent: skin.parent,
                skin: Some((skin.blend, skin.vertices.as_slice())),
                welds,
                translucent: false,
                bend: None,
            })
        });
        let mut inputs: Vec<PieceInput> = rigid.chain(skins).collect();

        // Hair and cloth. A detached object is only drawn through its chain.
        let mut chains = doa3_chains::parse_chains(cat);
        if !doa3::torso_front_chain_allowed(roster.id) {
            chains.retain(|chain| !chain.is_on_torso_front());
        }
        let mut cloth = doa3_cloth::parse_cloth(cat);
        if !doa3::torso_front_chain_allowed(roster.id) {
            cloth.retain(|sheet| !sheet.is_on_torso_front());
        }
        let mut tubes = doa3_tubes::parse_tubes(cat);
        if !doa3::torso_front_chain_allowed(roster.id) {
            tubes.retain(|tube| !tube.is_on_torso_front());
        }
        let mut springs = doa3_springs::parse_springs(cat);
        if !doa3::torso_front_chain_allowed(roster.id) {
            springs.retain(|spring| !spring.is_on_torso_front());
        }
        let mut chain_pieces: Vec<Option<(usize, usize)>> = vec![None; pieces.len()];
        let drawn = nodes
            .rigid
            .iter()
            .map(|&(node, object)| (node, object))
            .chain(nodes.skins.iter().map(|skin| (skin.node, skin.object)))
            .filter(|&(_, object)| object < pieces.len());
        for (input, (node, object)) in drawn.enumerate() {
            chain_pieces[object].get_or_insert((input, node));
        }
        for chain in &chains {
            for target in chain.targets.iter().filter(|t| t.detached) {
                if chain_pieces.get(target.object) != Some(&None) {
                    continue;
                }
                let Some((vertices, draws)) = draws_of(target.object) else { continue };
                chain_pieces[target.object] = Some((inputs.len(), chain.bone));
                inputs.push(PieceInput {
                    node: chain.bone,
                    vertices,
                    draws,
                    parent: chain.bone,
                    skin: None,
                    welds: Vec::new(),
                    translucent: false,
                    bend: None,
                });
            }
        }
        let extras = props.iter().map(|prop| (prop, false));
        let extras = extras.chain(overlays.iter().map(|overlay| (&overlay.prop, overlay.translucent)));
        for (prop, translucent) in extras {
            if chain_pieces[prop.object].is_some() {
                continue;
            }
            let Some((vertices, draws)) = draws_of(prop.object) else { continue };
            log::info!("Object {} on bone {} at {:?}, translucent: {}", prop.object, prop.bone, prop.offset, translucent);
            chain_pieces[prop.object] = Some((inputs.len(), prop.bone));
            inputs.push(PieceInput {
                node: prop.bone,
                vertices,
                draws,
                parent: prop.bone,
                skin: None,
                welds: Vec::new(),
                translucent,
                bend: None,
            });
        }
        let triangles: usize = inputs.iter().flat_map(|p| &p.draws).map(|(t, _, _)| t.len() / 3).sum();
        let piece_count = inputs.len();
        self.mesh.set_model(&self.ctx.device, &self.ctx.queue, &inputs, &textures);
        for chain in &chains {
            for target in chain.targets.iter().filter(|t| t.detached) {
                let Some(&Some((piece, _))) = chain_pieces.get(target.object) else { continue };
                let rest: Vec<glam::Vec3> =
                    pieces[target.object].vertices.iter().map(|v| glam::Vec3::from(v.position)).collect();
                self.mesh.set_rest_positions(piece, chain.anchored_vertices(target, &rest));
            }
        }

        self.wireframe.update_lines(&self.ctx.queue, &[]);
        // Changing costume keeps the camera; only a new character is framed.
        if self.framed_character != Some(char_idx) {
            self.camera.frame_character(1.8);
            self.framed_character = Some(char_idx);
        }
        self.current_animations = animations;
        log::info!("DOA3: {} costume {} is entry {}", display_name, costume_idx, model_entry);
        // The chest spring only exists for characters on the fixed list.
        let chest_object = nodes.rigid.iter().find(|&&(node, _)| node == 0).map(|&(_, object)| object);
        let spring_enabled = doa3::has_chest_spring(roster.id) && config.bouncing.unwrap_or(true);
        let chest = chest_object
            .filter(|_| spring_enabled)
            .and_then(|object| {
                let piece = nodes.rigid.iter().filter(|&&(_, o)| o < pieces.len()).position(|&(_, o)| o == object)?;
                Some(Doa3Chest { piece, groups: doa3::parse_chest_morph(cat)?, spring: None, frame: 0 })
            });

        let hands = doa3_morphs::parse_hands(cat, &nodes.rigid);
        let face = doa3_morphs::parse_face(cat, &nodes.rigid);
        let hand_events =
            archive.get_by_index(roster.bin_entry).map(doa3_morphs::parse_hand_events).unwrap_or_default();
        let chains = Doa3Chains {
            chains,
            cloth,
            tubes,
            springs,
            pieces: chain_pieces,
            frame: 0,
            wind: Wind::new(),
            hands,
            face,
            hand_events,
            character: roster.id,
            pose_hands: Vec::new(),
            copies: Vec::new(),
        };
        self.doa3_pose = Some(Doa3Pose { doa2u_style: None, skeleton, motion: None, chest, chains });
        self.game_clock = 0.0;
        let motion_index = config
            .motion
            .as_deref()
            .and_then(|start| self.current_animations.iter().position(|a| a.name.starts_with(start)))
            .unwrap_or(0);
        self.select_motion(motion_index);

        Ok(format!(
            "DOA3: {} — costume {:02} (entry {}): {} pieces, {} triangles, {} textures, {} motions",
            display_name,
            costume_idx,
            model_entry,
            piece_count,
            triangles,
            textures.len(),
            self.current_animations.len()
        ))
    }

    /// Starts a motion of the loaded DOA3 character. Other characters ignore this.
    pub fn select_motion(&mut self, index: usize) {
        let Some(pose) = &mut self.doa3_pose else { return };
        let Some(entry) = self.current_animations.get(index) else { return };
        let id = entry.doa3_motion_id;
        let Some((afs_entry, tracks)) = entry.doa3_motion else { return };
        let length = match self.char_panel.selected_source {
            GameSource::Doa2U => self.hdd_archive.as_ref(),
            GameSource::Doa3 => self.loadfile_archive.as_ref(),
        }
        .and_then(|archive| archive.get_by_index(afs_entry))
        .map_or(0, |data| doa3::motion_length(data, &tracks));
        pose.motion = Some(Doa3Motion { afs_entry, tracks, length: length as f32, id });
        self.anim_panel.selected_anim_idx = Some(index);
        self.anim_panel.playing = true;
        self.anim_time = 0.0;
        self.update_doa3_pose();
    }

    pub fn update_doa3_pose(&mut self) {
        let Some(pose) = &self.doa3_pose else { return };
        let Some(motion) = &pose.motion else { return };
        let Some(data) = self.motion_archive().and_then(|a| a.get_by_index(motion.afs_entry)) else {
            return;
        };
        match doa3::eval_motion(data, &motion.tracks, self.anim_time) {
            Ok(values) => {
                let doa2u_pose = pose.doa2u_style.is_some();
                let nodes = match pose.doa2u_style {
                    Some(style) => doa2u::solve_pose(&values, &pose.skeleton, style),
                    None => doa3::solve_pose(&values, &pose.skeleton),
                };
                let clock = self.game_clock as u64;
                let (motion_id, motion_frame) = (motion.id, self.anim_time as u16);
                if let Some(chest) = self.doa3_pose.as_mut().and_then(|p| p.chest.as_mut()) {
                    let parameters = if doa2u_pose { doa2u::CHEST_SPRING } else { ChestSpring::DOA3 };
                    let spring = chest.spring.get_or_insert_with(|| ChestSpring::with_parameters(&nodes[0], parameters));
                    // Holding the pose between 60 Hz frames, as the game does.
                    while chest.frame < clock {
                        spring.step(&nodes[0]);
                        chest.frame += 1;
                    }
                    let scaled = spring.value * self.bg_panel.bounce_scale;
                    log::debug!(
                        "chest spring {:.4}, scaled {:.4}, largest move {:.4} m",
                        spring.value,
                        scaled,
                        chest.groups.iter().map(|g| (if scaled >= 0.0 { g.positive } else { g.negative }).length()).fold(0.0, f32::max) * scaled.abs(),
                    );
                    self.mesh.deform_piece(chest.piece, &chest.groups, scaled);
                }
                if let Some(soft) = self.doa3_pose.as_mut().map(|p| &mut p.chains) {
                    // Holding the pose between 60 Hz frames, as the spring does.
                    let steps = clock.saturating_sub(soft.frame).clamp(u64::from(soft.frame == 0), MAX_CHAIN_CATCH_UP);
                    let (wind_on, wind_scale) = (self.anim_panel.wind, self.anim_panel.wind_scale);
                    for _ in 0..steps {
                        soft.wind.step(if wind_on { wind_scale } else { 0.0 });
                        for (i, chain) in soft.chains.iter_mut().enumerate() {
                            chain.step(&nodes, soft.wind.force(i));
                        }
                        // DOA2U sheets can start on vertices that an earlier sheet writes.
                        let chained = soft.cloth.iter().any(SoftCloth::has_vertex_roots);
                        let mut written = std::collections::HashMap::new();
                        for (i, sheet) in soft.cloth.iter_mut().enumerate() {
                            let object = sheet.object;
                            sheet.follow_vertices(|vertex| written.get(&(object, vertex)).copied());
                            sheet.step(&nodes, soft.wind.force(i));
                            if chained {
                                for (vertex, position) in sheet.vertex_positions(&nodes) {
                                    written.insert((object, vertex), position);
                                }
                            }
                        }
                        for tube in &mut soft.tubes {
                            tube.step(&nodes, &soft.wind);
                        }
                        for spring in &mut soft.springs {
                            spring.step(&nodes, &soft.wind);
                        }
                        let events = soft.hand_events.get(&(motion_id as u16));
                        for (index, hand) in soft.hands.iter_mut().enumerate() {
                            // Hand 1 of a script is the first hand here, the right one.
                            let script_hand = index ^ 1;
                            let event = events.and_then(|list| {
                                list.iter().rev().find(|e| e.targets(script_hand) && e.frame <= motion_frame)
                            });
                            let pose = event.map_or(0, |e| e.pose);
                            let rate = event.map_or(1.0 / 26.0, HandEvent::rate);
                            hand.step(doa3_morphs::character_pose(soft.character, pose), rate);
                        }
                        for (index, hand) in soft.pose_hands.iter_mut().enumerate() {
                            let script_hand = index ^ 1;
                            let event = events.and_then(|list| {
                                list.iter().rev().find(|e| e.targets(script_hand) && e.frame <= motion_frame)
                            });
                            let pose = event.map_or(0, |e| e.pose);
                            hand.step(pose as usize, event.map_or(1.0 / 26.0, HandEvent::rate));
                        }
                    }
                    if let Some(face) = &soft.face {
                        if let Some(&Some((piece, _))) = soft.pieces.get(face.object) {
                            self.mesh.set_rest_positions(
                                piece,
                                face.positions(clock).flat_map(|(vertices, value)| {
                                    vertices.iter().map(move |&vertex| (vertex, glam::Vec3::from(value)))
                                }),
                            );
                        }
                    }
                    for hand in &soft.hands {
                        let Some(&Some((piece, _))) = soft.pieces.get(hand.object) else { continue };
                        self.mesh.set_rest_positions(
                            piece,
                            hand.positions().flat_map(|(vertices, value)| {
                                vertices.iter().map(move |&vertex| (vertex, glam::Vec3::from(value)))
                            }),
                        );
                        self.mesh.set_rest_normals(
                            piece,
                            hand.normals().flat_map(|(vertices, value)| vertices.iter().map(move |&vertex| (vertex, value))),
                        );
                    }
                    for hand in &soft.pose_hands {
                        let Some(&Some((piece, _))) = soft.pieces.get(hand.object) else { continue };
                        self.mesh.set_rest_positions(
                            piece,
                            hand.positions().flat_map(|(vertices, value)| {
                                vertices.iter().map(move |&vertex| (vertex, glam::Vec3::from(value)))
                            }),
                        );
                        self.mesh.set_rest_normals(
                            piece,
                            hand.normals().flat_map(|(vertices, value)| vertices.iter().map(move |&vertex| (vertex, value))),
                        );
                    }
                    soft.frame = clock.max(1);
                    for sheet in &soft.cloth {
                        let Some(&Some((piece, node))) = soft.pieces.get(sheet.object) else { continue };
                        // Vertices are written in the bone's space.
                        let to_node = (node != sheet.bone).then(|| nodes[node].inverse() * nodes[sheet.bone]);
                        self.mesh.set_rest_positions(
                            piece,
                            sheet.vertex_positions(&nodes).into_iter().map(|(vertex, position)| {
                                (vertex, to_node.map_or(position, |m| m.transform_point3(position)))
                            }),
                        );
                    }
                    for spring in &soft.springs {
                        let Some(&Some((piece, node))) = soft.pieces.get(spring.object) else { continue };
                        // Vertices are written in the bone's space.
                        let to_node = (node != spring.bone).then(|| nodes[node].inverse() * nodes[spring.bone]);
                        self.mesh.set_rest_positions(
                            piece,
                            spring.vertex_positions(&nodes).into_iter().map(|(vertex, position)| {
                                (vertex, to_node.map_or(position, |m| m.transform_point3(position)))
                            }),
                        );
                    }
                    for tube in &soft.tubes {
                        let Some(&Some((piece, node))) = soft.pieces.get(tube.object) else { continue };
                        // Vertices are written in the bone's space.
                        let to_node = (node != tube.bone).then(|| nodes[node].inverse() * nodes[tube.bone]);
                        self.mesh.set_rest_positions(
                            piece,
                            tube.vertex_positions(&nodes).into_iter().map(|(vertex, position)| {
                                (vertex, to_node.map_or(position, |m| m.transform_point3(position)))
                            }),
                        );
                    }
                    for chain in &soft.chains {
                        for target in &chain.targets {
                            let Some(&Some((piece, node))) = soft.pieces.get(target.object) else { continue };
                            // Vertices are written in the bone's space.
                            let to_node = (node != chain.bone).then(|| nodes[node].inverse() * nodes[chain.bone]);
                            self.mesh.set_rest_positions(
                                piece,
                                chain.vertex_positions(target, &nodes).map(|(vertex, position)| {
                                    (vertex, to_node.map_or(position, |m| m.transform_point3(position)))
                                }),
                            );
                        }
                    }
                    for copy in &soft.copies {
                        let (Some(&Some((from, _))), Some(&Some((to, _)))) = (soft.pieces.get(copy.from), soft.pieces.get(copy.to)) else { continue };
                        self.mesh.copy_rest_positions(from, to, &copy.links);
                    }
                }
                self.mesh.apply_pose(&self.ctx.queue, &nodes);
            }
            Err(e) => log::warn!("Could not evaluate motion: {:#}", e),
        }
    }

    fn build_skeleton_lines(&mut self, bin_id: &str) -> Vec<LineVertex> {
        let bin_name = format!("{}.bin", bin_id);
        let Some(archive) = &self.hdd_archive else { return Vec::new() };
        let Some(bin_data) = archive.get_by_name(&bin_name) else {
            log::warn!("Could not find {} in datahdd.afs", bin_name);
            return Vec::new();
        };

        use crate::formats::character::CharacterBin;
        let char_bin = match CharacterBin::from_bytes(bin_data) {
            Ok(b) => b,
            Err(e) => {
                log::warn!("Failed to parse {}: {}", bin_name, e);
                return Vec::new();
            }
        };

        // Visualize collision capsules as lines
        let mut lines = Vec::new();
        let capsule_color = [0.3, 0.8, 0.4, 1.0f32];

        for cap in &char_bin.collision_capsules {
            let c = cap.center;
            let r = cap.radius;
            // Draw a small cross at the capsule center
            lines.push(LineVertex { position: [c.x - r, c.y, c.z], color: capsule_color });
            lines.push(LineVertex { position: [c.x + r, c.y, c.z], color: capsule_color });
            lines.push(LineVertex { position: [c.x, c.y - r, c.z], color: capsule_color });
            lines.push(LineVertex { position: [c.x, c.y + r, c.z], color: capsule_color });
            lines.push(LineVertex { position: [c.x, c.y, c.z - r], color: capsule_color });
            lines.push(LineVertex { position: [c.x, c.y, c.z + r], color: capsule_color });
        }

        lines
    }

    pub fn tick(&mut self, dt: f32) {
        // Hair, cloth and hands keep moving while the motion is paused.
        self.game_clock += dt * self.anim_panel.speed * 60.0;
        if self.anim_panel.playing {
            self.anim_time += dt * self.anim_panel.speed * 60.0;
            let length = self.doa3_pose.as_ref().and_then(|p| p.motion.as_ref()).map_or(0.0, |m| m.length);
            if length > 0.0 && self.anim_time > length {
                if self.anim_panel.loop_anim {
                    self.anim_time %= length;
                } else {
                    self.anim_time = length;
                    self.anim_panel.playing = false;
                }
            }
        }
        self.update_doa3_pose();
    }

    pub fn has_chest_spring(&self) -> bool {
        self.doa3_pose.as_ref().is_some_and(|pose| pose.chest.is_some())
    }

    pub fn reset_animation(&mut self) {
        self.anim_time = 0.0;
        self.update_doa3_pose();
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        self.ctx.resize(w, h);
        self.camera.set_aspect(w as f32 / h as f32);
    }

    pub fn clear_color(&self) -> wgpu::Color {
        let cc = self
            .bg_panel
            .current_option(&self.background_options)
            .map(|o| o.clear_color)
            .unwrap_or([0.08, 0.08, 0.1, 1.0]);
        wgpu::Color { r: cc[0] as f64, g: cc[1] as f64, b: cc[2] as f64, a: cc[3] as f64 }
    }
}
