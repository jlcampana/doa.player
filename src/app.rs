use crate::formats::config::CharacterConfigs;
use std::collections::HashMap;

use crate::formats::doa2u;
use crate::formats::doax;
use crate::formats::doa3::{self, ChestMorphGroup, ChestSpring, MotionTracks, RosterEntry, Xbe};
use crate::formats::doa3_chains::{self, SoftChain};
use crate::formats::doa3_cloth::{self, SoftCloth};
use crate::formats::doa3_morphs::{self, FaceMorph, HandEvent, HandMorph};
use crate::formats::doa3_springs::{self, SoftSprings};
use crate::formats::doa3_tubes::{self, SoftTube};
use crate::formats::doa3_wind::Wind;
use crate::formats::model::{parse_sprite_xpr, parse_xpr_model, parse_xpr_textures, ModelTexture};
use crate::formats::AfsArchive;
use crate::renderer::{Camera, RenderContext};
use crate::renderer::grid::GridRenderer;
use crate::renderer::mesh::{MeshRenderer, PieceInput, VertexBend};
use crate::renderer::wireframe::{WireframeRenderer, LineVertex};
use crate::ui::{AnimPanel, BgSelectPanel, CharSelectPanel};
use std::path::{Path, PathBuf};

// ─── Data model ───────────────────────────────────────────────────────────────

/// The costumes of a character still to export.
pub struct ExportQueue {
    character: usize,
    next: usize,
    count: usize,
    written: usize,
    all_motions: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GameSource {
    Doa2U,
    Doa3,
    Doax,
    Doax2,
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
    /// Not a fighter: listed apart. Some are the intro's, as the user recognized them.
    pub intro: bool,
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

const DOA2U_XBE_PATH: &str = "doa2u/DOA2.xbe";
const DOA2U_HDD_PATH: &str = "doa2u/datahdd.afs";
const DOA2U_HDD2_PATH: &str = "doa2u/datahdd2.afs";
const DOA2U_VOICE_PATH: &str = "doa2u/voice.afs";
const DOA2U_DVD_PATH: &str = "doa2u/datadvd.afs";
/// The app's icon, from the game's files, so that it stays out of the repository: Popularity in
/// the ranking menu. 20 is Popularity (tag).
const APP_ICON_SPRITE: (&str, usize) = ("spOptrnk.xpr", 19);
const DOAX_XBE_PATH: &str = "doax/default.xbe";
const DOAX_HDD_PATH: &str = "doax/datahdd.afs";
const DOAX_DVD_PATH: &str = "doax/datadvd.afs";
const DOAX2_CHR_PATH: &str = "doax2/chr.afs";
const DOAX2_MOT_PATH: &str = "doax2/mot.afs";
const DOAX2_WEAR_PATHS: [&str; 2] = ["doax2/werone.afs", "doax2/wertwo.afs"];
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

/// Where DOA2U's voices are. The window asks every frame, so this looks once.
pub fn doa2u_voice_path() -> &'static str {
    static PATH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    PATH.get_or_init(|| asset_path(DOA2U_VOICE_PATH).to_string_lossy().into_owned())
}

fn read_app_icon(archive: Option<&AfsArchive>) -> Option<ModelTexture> {
    let (name, index) = APP_ICON_SPRITE;
    let sprites = parse_sprite_xpr(archive?.get_by_name(name)?);
    match sprites {
        Ok(mut sprites) if index < sprites.len() => Some(sprites.swap_remove(index)),
        Ok(_) => None,
        Err(e) => {
            log::warn!("Could not read the app's icon: {e:#}");
            None
        }
    }
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

fn doa2u_characters(archives: &[&AfsArchive]) -> Vec<CharacterEntry> {
    // (file prefixes of the costumes, name of the .bin and .mot files, shown name, character
    // ID). IDs are from the counts at 0x6E0F08 and the order of the file names at 0x694288.
    // Costumes come from the archives, not from the select screen's counts: the game's own
    // table at 0x694500 also lists the ones it does not offer there.
    //
    // Prefixes in the `A0` series follow the order of the table at 0x695848, which runs by ID:
    // that `bay` there is Leon and `sam` Bayman, and the IDs of `nin`, `sui` and `gok`, are
    // inferred from it. `hks` has no .bin, and its ID is inferred from the costume counts.
    // `nin`, `sui` and `gok` are named from the pieces and textures they share with DOA3's
    // models (`compare_a0_series_with_doa3`), and move as in DOA3: see `a0_doa3_character`.
    let roster: [(&[&str], &[&str], &str, &str, usize); 24] = [
        (&["kas"], &[], "kasumi", "Kasumi", 5),
        (&["aya"], &[], "ayane", "Ayane", 13),
        (&["zac"], &[], "zack", "Zack", 0),
        (&["tin"], &[], "tina", "Tina Armstrong", 1),
        (&["ryu"], &[], "ryu", "Ryu Hayabusa", 4),
        (&["lei"], &[], "leifang", "Lei Fang", 12),
        (&["jan"], &[], "janlee", "Jann Lee", 2),
        (&["bas"], &[], "bass", "Bass Armstrong", 9),
        // "sambo" is Bayman and "bayman" is Leon. Bayman's ID and files were
        // measured in a running match. Leon's follow, and from the costume counts.
        (&["bay", "sam"], &["bayA"], "sambo", "Bayman", 24),
        (&["gen"], &[], "genfu", "Gen Fu", 6),
        (&["leo", "bayA"], &[], "bayman", "Leon", 8),
        (&["kor"], &[], "koryu", "Hitomi", 21),
        (&["tng"], &[], "tengu", "Tengu", 17),
        (&["ein"], &[], "karate", "Ein", 3),
        (&["hel", "hik"], &[], "hika", "Helena Douglas", 7),
        (&["mam"], &[], "mama", "Mama", 10),
        (&["pdg"], &[], "pdog", "Pdog", 11),
        (&["wba"], &[], "wakabs", "Wakabs", 14),
        (&["lor"], &[], "loran", "Loran", 15),
        (&["hks"], &[], "", "hks", 16),
        (&["yti"], &[], "ytina", "ytina", 18),
        // These three have no motions in DOA2U. Without DOA3's files they borrow some, so as
        // not to lie down.
        (&["nin"], &[], "ryu", "Hayate", 10),
        (&["sui"], &[], "karate", "Brad Wong", 22),
        (&["gok"], &[], "genfu", "Christie", 23),
    ];
    roster
        .iter()
        .map(|(prefixes, excluded, bin_id, name, id)| CharacterEntry {
            id: prefixes[0].to_string(),
            bin_id: bin_id.to_string(),
            display_name: name.to_string(),
            // File numbers have gaps, and placeholders repeat another costume's name.
            costumes: {
                let mut stems: Vec<String> = Vec::new();
                for entry in archives.iter().flat_map(|archive| archive.entries()) {
                    let Some(stem) = entry.name.strip_suffix(".xpr") else { continue };
                    let ours = prefixes.iter().any(|p| stem.starts_with(p)) && !excluded.iter().any(|p| stem.starts_with(p));
                    if ours && !stems.iter().any(|known| known == stem) {
                        stems.push(stem.to_string());
                    }
                }
                stems
            },
            afs_indices: vec![],
            doa3: None,
            doa2u_id: Some(*id),
            intro: ["mam", "pdg", "wba", "lor", "hks", "yti"].contains(&prefixes[0]),
        })
        .collect()
}

/// DOA3's ID of a character with `A0` models and no files in DOA2U: Hayate, Brad Wong and
/// Christie. Their bones and motions are in DOA3's, when that game's files are present.
fn a0_doa3_character(prefix: &str) -> Option<usize> {
    match prefix {
        "nin" => Some(11),
        "sui" => Some(19),
        "gok" => Some(20),
        _ => None,
    }
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
                intro: false,
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

/// Every stage's lights from DOA2's executable, the desert first. The snowy night and the
/// cloudy stage use what was read from the running game, which differs from the table.
fn load_stage_lights() -> Vec<(String, crate::renderer::mesh::SceneLights)> {
    use crate::renderer::mesh::{SceneLights, CLOUDY_LIGHTS, FALLBACK_LIGHTS, SNOWY_NIGHT_LIGHTS};
    let areas = std::fs::read(asset_path(DOA2U_XBE_PATH)).map_err(anyhow::Error::from).and_then(Xbe::from_bytes).and_then(|xbe| doa2u::area_lights(&xbe));
    let mut list: Vec<(String, SceneLights)> = match areas {
        Ok(areas) => areas
            .into_iter()
            .map(|area| {
                let lights = match area.area {
                    15 => CLOUDY_LIGHTS,
                    19 => SNOWY_NIGHT_LIGHTS,
                    _ => SceneLights { ambient: area.ambient, lights: area.lights },
                };
                (area.name, lights)
            })
            .collect(),
        Err(e) => {
            log::warn!("Could not read the stage lights: {e:#}");
            Vec::new()
        }
    };
    if let Some(desert) = list.iter().position(|(name, _)| name.contains("desert")) {
        let entry = list.remove(desert);
        list.insert(0, entry);
    }
    if let Some(storm) = list.iter().position(|(name, _)| name.contains("storm")) {
        let flash = list[storm].1.lightning_flash();
        list.insert(storm + 1, (format!("{}: lightning", list[storm].0), flash));
    }
    if list.is_empty() {
        list.push(("Desert".to_string(), FALLBACK_LIGHTS));
    }
    log::debug!("Stage lights: {:?}", list.iter().map(|(name, _)| name).collect::<Vec<_>>());
    list
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
    pub app_icon: Option<ModelTexture>,
    pub doax_characters: Vec<CharacterEntry>,
    pub doax_stage_lights: Vec<(String, crate::renderer::mesh::SceneLights)>,
    pub doax2_stage_lights: Vec<(String, crate::renderer::mesh::SceneLights)>,
    /// The loaded DOAX character's offsets and `.mot` files, and the motion playing.
    doax_pose: Option<DoaxPose>,
    doax2_pose: Option<Doax2Pose>,
    doax_hdd: Option<doax::LazyArchive>,
    doax_dvd: Option<doax::LazyArchive>,
    pub doax2_characters: Vec<CharacterEntry>,
    doax2_chr: Option<doax::LazyArchive>,
    doax2_wear: Vec<doax::LazyArchive>,
    doax2_wear_names: Vec<Vec<String>>,
    doax2_mot: Option<doax::LazyArchive>,
    pub anim_panel: AnimPanel,
    pub bg_panel: BgSelectPanel,

    // Data
    pub doa2u_characters: Vec<CharacterEntry>,
    pub doa3_characters: Vec<CharacterEntry>,
    pub current_animations: Vec<AnimationEntry>,
    /// The loaded costume as the renderer got it, for exporting.
    pub export_source: crate::export::ExportModel,
    /// Name of the loaded costume, and of its character and game, for the export's path.
    pub loaded_name: (String, String, String),
    pub export_queue: Option<ExportQueue>,
    pub background_options: Vec<BackgroundOption>,

    // Archives (lazily loaded)
    pub hdd_archive: Option<AfsArchive>,
    pub hdd2_archive: Option<AfsArchive>,
    /// Names and lights of the stages, for the selector.
    pub stage_lights: Vec<(String, crate::renderer::mesh::SceneLights)>,
    pub voices: crate::voices::VoicePlayer,
    pub voice_panel: crate::ui::VoicePanel,
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

/// The loaded DOAX2 character's motions and, per drawn piece, its DOAX bone and its place
/// from that bone at rest.
struct Doax2Pose {
    files: Vec<Vec<u8>>,
    motions: Vec<(usize, doax::Motion)>,
    playing: Option<usize>,
    rows: [glam::Vec3; 24],
    pieces: Vec<(usize, glam::Mat4)>,
    /// Pieces with chest shapes: the mesh piece, and each vertex's base, up and down positions.
    chest: Vec<(usize, Vec<[glam::Vec3; 3]>)>,
    chest_spring: Option<doa3::ChestSpring>,
    chest_frame: u64,
    /// The nodes as first shown, before a motion: the export's bind pose.
    rest_nodes: Vec<glam::Mat4>,
    /// Per hand: the scripts' hand number, the mesh piece, its poses.
    hands: Vec<(u16, usize, crate::formats::doax2::HandPoses)>,
    /// Blinks by motion key, as hand events: (frame, eye shape).
    blinks: std::collections::HashMap<u16, Vec<(u16, u16)>>,
    /// The head's face shape sets and the piece they shape.
    faces: Vec<(usize, crate::formats::doax2::HandPoses)>,
    /// Hand events by motion key: own motions by number, shared ones by number + 0x2000.
    events: std::collections::HashMap<u16, Vec<doax::HandEvent>>,
    /// Per motion file, whether it is a shared one.
    shared_files: Vec<bool>,
}

/// What is needed to pose the loaded DOA3 character.
pub struct DoaxPose {
    rows: [glam::Vec3; 24],
    files: Vec<Vec<u8>>,
    /// Each animation entry's file and motion.
    motions: Vec<(usize, doax::Motion)>,
    playing: Option<usize>,
    /// Each file's events by motion: the character's own, or the shared ones under their flag.
    events: std::collections::HashMap<u16, Vec<doax::HandEvent>>,
    shared_events: std::collections::HashMap<u16, Vec<doax::HandEvent>>,
    /// Which files are the shared ones.
    shared_files: Vec<bool>,
    pose_shapes: Vec<usize>,
    /// (slot in the scripts, the hand's shapes, its piece).
    hands: Vec<(u16, crate::formats::doa3_morphs::PoseHand, usize)>,
    /// The chest's shapes and those of pieces that follow it, with their pieces.
    chest_sets: Vec<(crate::formats::doa3_morphs::PoseHand, usize)>,
    chest_spring: Option<doa3::ChestSpring>,
    /// Game frames the spring has run.
    chest_frame: u64,
}

pub struct Doa3Pose {
    /// Set for DOA2U costumes, which have their own solve.
    pub doa2u_style: Option<doa2u::PoseStyle>,
    pub skeleton: [glam::Vec3; 17],
    pub motion: Option<Doa3Motion>,
    pub chest: Option<Doa3Chest>,
    pub chains: Doa3Chains,
    /// The motions are in DOA3's `loadfile.afs`, not in DOA2U's archive.
    pub motions_in_doa3: bool,
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
    /// DOA2U: objects whose normals are rebuilt as they move.
    pub normal_rebuilds: Vec<doa2u::NormalRebuild>,
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
        window: &winit::window::Window,
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
        let stage_lights = load_stage_lights();
        let hdd2_archive = match AfsArchive::from_file(asset_path(DOA2U_HDD2_PATH)) {
            Ok(a) => Some(a),
            Err(e) => {
                log::warn!("Could not load DOA2U datahdd2.afs: {}", e);
                None
            }
        };
        let dvd_archive = match AfsArchive::from_file(asset_path(DOA2U_DVD_PATH)) {
            Ok(a) => Some(a),
            Err(e) => {
                log::warn!("Could not load DOA2U datadvd.afs: {}", e);
                None
            }
        };
        let costume_archives: Vec<&AfsArchive> = hdd2_archive.iter().chain(dvd_archive.iter()).collect();
        let doa2u_characters = doa2u_characters(&costume_archives);

        let hdd_archive = match AfsArchive::from_file(asset_path(DOA2U_HDD_PATH)) {
            Ok(a) => {
                log::info!("Loaded DOA2U datahdd.afs: {} files", a.entries().len());
                Some(a)
            }
            Err(e) => {
                log::warn!("Could not load DOA2U datahdd.afs: {}", e);
                None
            }
        };
        let doax_hdd = doax::LazyArchive::open(asset_path(DOAX_HDD_PATH)).map_err(|e| log::info!("No DOAX: {e:#}")).ok();
        let doax_dvd = doax::LazyArchive::open(asset_path(DOAX_DVD_PATH)).map_err(|e| log::info!("No DOAX: {e:#}")).ok();
        let doax_characters: Vec<CharacterEntry> = doax_hdd
            .iter()
            .flat_map(|archive| {
                doax::ROSTER.iter().map(move |(name, code, _)| CharacterEntry {
                    id: code.to_string(),
                    bin_id: String::new(),
                    display_name: name.to_string(),
                    costumes: doax::costumes(archive.names(), code),
                    afs_indices: vec![],
                    doa3: None,
                    doa2u_id: None,
                    intro: false,
                })
            })
            .collect();
        let doax2_chr = doax::LazyArchive::open(asset_path(DOAX2_CHR_PATH)).map_err(|e| log::info!("No DOAX2: {e:#}")).ok();
        let doax2_wear: Vec<doax::LazyArchive> = DOAX2_WEAR_PATHS
            .iter()
            .filter_map(|path| doax::LazyArchive::open(asset_path(path)).map_err(|e| log::info!("No DOAX2 suits: {e:#}")).ok())
            .collect();
        let doax2_wear_names: Vec<Vec<String>> = doax2_wear.iter().map(|w| w.names().map(str::to_string).collect()).collect();
        let doax2_characters: Vec<CharacterEntry> = doax2_chr
            .iter()
            .flat_map(|_| {
                let names = &doax2_wear_names;
                crate::formats::doax2::ROSTER.iter().map(move |(name, code)| CharacterEntry {
                    id: code.to_string(),
                    bin_id: String::new(),
                    display_name: name.to_string(),
                    costumes: crate::formats::doax2::suits(names, code).into_iter().map(|s| s.label).collect(),
                    afs_indices: vec![],
                    doa3: None,
                    doa2u_id: None,
                    intro: false,
                })
            })
            .collect();
        let doax_stage_lights = std::fs::read(asset_path(DOAX_XBE_PATH)).map(|xbe| doax::stage_lights(&xbe)).unwrap_or_default();
        let app_icon = read_app_icon(hdd_archive.as_ref());
        let mut char_panel = CharSelectPanel::new();
        if let Some(icon) = &app_icon {
            // Wayland ignores this: there the icon comes from the desktop entry.
            window.set_window_icon(winit::window::Icon::from_rgba(icon.rgba.clone(), icon.width, icon.height).ok());
            let image = egui::ColorImage::from_rgba_unmultiplied([icon.width as usize, icon.height as usize], &icon.rgba);
            char_panel.logo = Some(egui_ctx.load_texture("app icon", image, egui::TextureOptions::LINEAR));
        }

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
            char_panel,
            app_icon,
            doax_characters,
            doax_stage_lights,
            doax_pose: None,
            doax2_pose: None,
            doax_hdd,
            doax_dvd,
            doax2_characters,
            doax2_stage_lights: crate::formats::doax2::stage_lights(),
            doax2_chr,
            doax2_wear,
            doax2_wear_names,
            doax2_mot: doax::LazyArchive::open(asset_path(DOAX2_MOT_PATH)).map_err(|e| log::info!("No DOAX2 motions: {e:#}")).ok(),
            anim_panel: AnimPanel::new(),
            bg_panel: BgSelectPanel::new(),
            doa2u_characters,
            doa3_characters,
            current_animations: Vec::new(),
            export_source: Default::default(),
            loaded_name: Default::default(),
            export_queue: None,
            background_options,
            hdd_archive,
            hdd2_archive,
            stage_lights,
            voices: Default::default(),
            voice_panel: crate::ui::VoicePanel::new(),
            dvd_archive,
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

    /// The stage lights picked in the panel.
    /// The list the stage lights selector offers, for the game on show.
    pub fn stage_light_list(&self) -> &[(String, crate::renderer::mesh::SceneLights)] {
        match self.char_panel.selected_source {
            GameSource::Doax => &self.doax_stage_lights,
            GameSource::Doax2 => &self.doax2_stage_lights,
            _ => &self.stage_lights,
        }
    }

    pub fn current_stage_lights(&self) -> crate::renderer::mesh::SceneLights {
        self.stage_light_list().get(self.bg_panel.lights).or(self.stage_light_list().first()).map_or(crate::renderer::mesh::FALLBACK_LIGHTS, |entry| entry.1)
    }

    pub fn load_character(&mut self, char_idx: usize, costume_idx: usize) {
        // The motion carries over, playing or paused, on the same frame, if the new list has
        // the same motion: changing a costume keeps what was on screen. A paused motion stays
        // paused even when the new list lacks it.
        let playing = self.anim_panel.playing;
        let kept = self
            .anim_panel
            .selected_anim_idx
            .and_then(|i| self.current_animations.get(i))
            .map(|entry| (entry.name.clone(), entry.doa3_motion_id, self.anim_time));
        self.doax_pose = None;
        self.doax2_pose = None;
        self.mesh.set_lights_follow_camera(false);
        self.load_character_files(char_idx, costume_idx);
        if let Some((name, id, time)) = kept {
            let same = self.current_animations.iter().position(|a| a.name == name)
                .or_else(|| self.current_animations.iter().position(|a| a.doa3_motion_id == id));
            if let Some(index) = same {
                self.select_motion(index);
                self.anim_time = time;
                self.anim_panel.playing = playing;
            } else if !playing {
                self.anim_panel.playing = false;
            }
            self.update_doa3_pose();
        }
    }

    fn load_character_files(&mut self, char_idx: usize, costume_idx: usize) {
        match self.char_panel.selected_source {
            GameSource::Doa2U => self.load_doa2u_character(char_idx, costume_idx),
            GameSource::Doa3  => self.load_doa3_character(char_idx, costume_idx),
            GameSource::Doax2 => {
                self.status_msg = match self.try_load_doax2_character(char_idx, costume_idx) {
                    Ok(status) => status,
                    Err(e) => {
                        log::warn!("Could not load DOAX2 character: {e:#}");
                        self.mesh.clear_model();
                        format!("DOAX2: could not load character: {e:#}")
                    }
                };
            }
            GameSource::Doax  => {
                self.status_msg = match self.try_load_doax_character(char_idx, costume_idx) {
                    Ok(status) => status,
                    Err(e) => {
                        log::warn!("Could not load DOAX character: {e:#}");
                        self.mesh.clear_model();
                        format!("DOAX: could not load character: {e:#}")
                    }
                };
            }
        }
    }

    /// A DOAX2 costume: the body, the first head and a suit, in the files' rest pose. No
    /// motions yet.
    fn try_load_doax2_character(&mut self, char_idx: usize, costume_idx: usize) -> anyhow::Result<String> {
        use crate::formats::doax2;
        use anyhow::Context;

        let costume_idx = if costume_idx == DEFAULT_COSTUME { 0 } else { costume_idx };
        self.char_panel.selected_costume_idx = costume_idx;
        let entry = self.doax2_characters.get(char_idx).context("unknown character")?;
        let (code, display_name) = (entry.id.clone(), entry.display_name.clone());
        let chr = self.doax2_chr.as_ref().context("no DOAX2 chr.afs")?;
        let suits = doax2::suits(&self.doax2_wear_names, &code);
        let suit = suits.get(costume_idx).context("unknown suit")?;
        let wear = &self.doax2_wear[suit.archive];
        let head = format!("v{code}AA");
        let body_cat = chr.read(&format!("v{code}1.cat"))?;
        // Suits hold a list per body type; the body's CAT says which is hers.
        let body_type = doax2::body_type(&body_cat).ok();
        let files = [
            doax2::load(&chr.read(&format!("v{code}1.tpr"))?, &body_cat).context("body")?,
            doax2::load(&chr.read(&format!("{head}.tpr"))?, &chr.read(&format!("{head}.cat"))?).context("head")?,
            doax2::load_for(&wear.read_index(suit.entry)?, &wear.read_index(suit.entry + 1)?, body_type).context("suit")?,
        ];
        // Each file has its own node tree; they are laid one after another. The feet stand
        // on the ground: the lowest body vertex at rest is put at height 0.
        let flip = glam::Mat4::from_scale(glam::Vec3::new(1.0, 1.0, -1.0));
        let body_rest = &files[0].rest;
        let floor = files[0]
            .pieces
            .iter()
            .flat_map(|(piece, node, _)| piece.vertices.iter().map(move |v| body_rest[*node].transform_point3(v.position.into()).y))
            .fold(f32::MAX, f32::min);
        let lift = glam::Mat4::from_translation(glam::Vec3::Y * -floor.min(0.0));
        let mut textures = Vec::new();
        // The head file's root goes on the body's head node.
        let head_frame = match (files[0].head_frame(), files[1].rest.first()) {
            (Some(frame), Some(root)) => frame * root.inverse(),
            _ => glam::Mat4::IDENTITY,
        };
        // Motions use DOAX's tracks and solve, with the body's offsets. Each piece follows the
        // DOAX bone of its CAT number from where it sits at rest; a blended piece gets a node per
        // weight group, on that group's bone.
        let rows = doax2::bone_rows(&files[0].rest)?;
        let bone_rest = doax2::bone_rest(&rows);
        let pelvis = files[0].rest[0].w_axis.truncate();
        struct Drawn<'a> {
            piece: &'a crate::formats::model::ModelPiece,
            first_texture: usize,
            texture_count: usize,
            node: usize,
            multi: Option<crate::renderer::mesh::MultiBend>,
        }
        let mut drawn = Vec::new();
        // (bone, place from the bone) and the rest matrix, per node.
        let mut pose_nodes: Vec<(usize, glam::Mat4)> = Vec::new();
        let mut rest_nodes: Vec<glam::Mat4> = Vec::new();
        let mut chest = Vec::new();
        let mut body_objects = Vec::new();
        let mut head_objects = Vec::new();
        for (index, file) in files.iter().enumerate() {
            let drawn_before = drawn.len();
            let texture_base = textures.len();
            let parent = if index == 1 { head_frame } else { glam::Mat4::IDENTITY };
            textures.extend(file.textures.iter().cloned());
            for ((piece, node, placed), weights) in file.pieces.iter().zip(&file.weights) {
                let rest = glam::Mat4::from_translation(-pelvis) * parent * file.rest[*node];
                let shown = flip * lift * parent * file.rest[*node];
                let own_bone = doax2::SLOT_BONES.get(placed.slot as usize).copied().unwrap_or(2);
                let groups = weights.first().map_or(0, |w| w.len());
                let bones = doax2::blend_bones(placed.slot, groups).filter(|_| groups > 0);
                if groups > 0 && bones.is_none() {
                    log::warn!("DOAX2: no groups for bone number {} with {groups} weights", placed.slot);
                }
                let node = pose_nodes.len();
                pose_nodes.push((own_bone, bone_rest[own_bone].inverse() * rest));
                rest_nodes.push(shown);
                let multi = bones.map(|bones| {
                    let groups = bones
                        .iter()
                        .map(|&bone| {
                            pose_nodes.push((bone, bone_rest[bone].inverse() * rest));
                            rest_nodes.push(shown);
                            (pose_nodes.len() - 1, glam::Vec3::ZERO)
                        })
                        .collect();
                    crate::renderer::mesh::MultiBend { groups, weights: weights.clone() }
                });
                if let Some(Some(shapes)) = file.shapes.get(drawn.len() - drawn_before) {
                    let positions = piece
                        .vertices
                        .iter()
                        .zip(shapes)
                        .map(|(v, s)| [glam::Vec3::from(v.position), s[doax2::SHAPE_UP].into(), s[doax2::SHAPE_DOWN].into()])
                        .collect();
                    chest.push((drawn.len(), positions));
                }
                if index == 0 {
                    body_objects.push((placed.object, drawn.len()));
                }
                if index == 1 {
                    head_objects.push((placed.object, drawn.len()));
                }
                drawn.push(Drawn { piece, first_texture: texture_base, texture_count: file.textures.len(), node, multi });
            }
        }
        let nodes = rest_nodes;
        // Each piece as drawn, then its hair strands again as a piece on the same node,
        // blended by the texture's alpha after the solid ones. Pieces shaped at run time
        // (chest, hands) have no strands, so their indices stay those of `drawn`.
        fn input_of<'a>(d: &Drawn<'a>, blended: bool) -> PieceInput<'a> {
            PieceInput {
                node: d.node,
                vertices: &d.piece.vertices,
                draws: d
                    .piece
                    .materials
                    .iter()
                    .filter(|m| crate::formats::doax2::is_blended(m) == blended)
                    .map(|m| (m.indices.as_slice(), m.base_texture(d.texture_count).map(|t| t + d.first_texture), m.cull_back, m.look))
                    .collect(),
                parent: d.node,
                skin: None,
                bend: None,
                multi: d.multi.clone(),
                welds: Vec::new(),
                // Blended through the material, which writes depth: the hair's inner shadow
                // layers stay under its outer strands. Without depth they drew over them.
                translucent: false,
            }
        }
        let mut inputs: Vec<PieceInput> = drawn.iter().map(|d| input_of(d, false)).collect();
        inputs.extend(drawn.iter().filter(|d| d.piece.materials.iter().any(doax2::is_blended)).map(|d| input_of(d, true)));
        let pose_pieces = pose_nodes;
        // Hands: the pose file for holidays, the scripts' events. Hand k of the scripts is taken
        // to be the file's k-th block; not checked.
        // Face shapes: the head CAT's pose sets, from the offset in its word 3.
        let faces: Vec<(usize, doax2::HandPoses)> = chr
            .read(&format!("{head}.cat"))
            .ok()
            .and_then(|cat| {
                let start = u32::from_be_bytes(cat.get(12..16)?.try_into().ok()?) as usize;
                doax2::hand_poses(cat.get(start..)?, &files[1].objects).map_err(|e| log::warn!("DOAX2 faces: {e:#}")).ok()
            })
            .unwrap_or_default()
            .into_iter()
            .filter_map(|set| Some((head_objects.iter().find(|(object, _)| *object == set.object)?.1, set)))
            .collect();
        let hands: Vec<(u16, usize, doax2::HandPoses)> = chr
            .read("hand_vacance.bin")
            .ok()
            .and_then(|file| doax2::hand_poses(&file, &files[0].objects).map_err(|e| log::warn!("DOAX2 hands: {e:#}")).ok())
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .filter_map(|(k, hand)| {
                let piece = body_objects.iter().find(|(object, _)| *object == hand.object)?.1;
                Some((k as u16, piece, hand))
            })
            .collect();
        let mut events = std::collections::HashMap::new();
        let mut blinks = std::collections::HashMap::new();
        let mut shared_files = Vec::new();
        let mut motion_data = Vec::new();
        let mut motions = Vec::new();
        let mut animations = Vec::new();
        if let Some(mot) = self.doax2_mot.as_ref() {
            let names: Vec<String> = mot.names().map(str::to_string).collect();
            if let Some(name) = doax2::motion_name(&code) {
                if let Ok(bin) = mot.read(&format!("{name}.bin")) {
                    events.extend(doax2::hand_events(&bin));
                    blinks.extend(doax2::eye_events(&bin));
                }
            }
            if let Ok(bin) = mot.read("anime_cm.bin") {
                events.extend(doax2::hand_events(&bin));
                blinks.extend(doax2::eye_events(&bin));
            }
            for file in doax2::motion_files(&names, &code) {
                let Ok(data) = mot.read(&file) else { continue };
                shared_files.push(file.starts_with("comon"));
                for motion in doax::list_motions_big(&data) {
                    motions.push((motion_data.len(), motion));
                    animations.push(AnimationEntry {
                        name: format!("{} {:04} ({} frames)", file.trim_end_matches(".mot"), motion.id, motion.frames),
                        file_key: file.clone(),
                        doa3_motion: None,
                        doa3_motion_id: motion.id,
                    });
                }
                motion_data.push(data);
            }
        }
        self.doa3_pose = None;
        self.doax_pose = None;
        self.doax2_pose = Some(Doax2Pose {
            files: motion_data, motions, playing: None, rows, pieces: pose_pieces, chest, chest_spring: None, chest_frame: 0,
            rest_nodes: nodes.clone(),
            hands, events, shared_files, faces, blinks,
        });
        self.current_animations = animations;
        self.anim_panel.selected_anim_idx = None;
        self.anim_time = 0.0;
        self.loaded_name = ("DOAX2".to_string(), display_name.clone(), suit.label.clone());
        self.mesh.set_lights(Some(self.current_stage_lights()));
        // DOAX2's light directions are in the camera's space.
        self.mesh.set_lights_follow_camera(true);
        self.export_source = crate::export::ExportModel::new(&inputs, &textures);
        self.mesh.set_model(&self.ctx.device, &self.ctx.queue, &inputs, &textures);
        let pieces = inputs.len();
        drop(inputs);
        self.mesh.apply_pose(&self.ctx.queue, &nodes);
        let count = self.current_animations.len();
        if count > 0 {
            self.select_motion(0);
        }
        Ok(format!("DOAX2: {display_name} - {}, {pieces} pieces, {count} motions", suit.label))
    }

    /// A DOAX costume: the character's body with a suit, standing in the rest pose.
    fn try_load_doax_character(&mut self, char_idx: usize, costume_idx: usize) -> anyhow::Result<String> {
        use anyhow::Context;

        let costume_idx = if costume_idx == DEFAULT_COSTUME { 0 } else { costume_idx };
        self.char_panel.selected_costume_idx = costume_idx;
        let entry = self.doax_characters.get(char_idx).context("unknown character")?;
        let display_name = entry.display_name.clone();
        let code = entry.id.clone();
        let stem = entry.costumes.get(costume_idx).context("unknown costume")?.clone();
        let table = doax::ROSTER.iter().find(|r| r.1 == code).context("not in the roster")?.2;
        let hdd = self.doax_hdd.as_ref().context("no DOAX datahdd.afs")?;
        let dvd = self.doax_dvd.as_ref().context("no DOAX datadvd.afs")?;

        let (body_xpr, body_cat) = (dvd.read(&format!("v{code}00.xpr"))?, dvd.read(&format!("v{code}00.cat"))?);
        let (suit_xpr, suit_cat) = (hdd.read(&format!("{stem}.xpr"))?, hdd.read(&format!("{stem}.cat"))?);
        let body = doa2u::parse_model(&body_xpr)?;
        let suit = doa2u::parse_model(&suit_xpr)?;
        let mut textures = parse_xpr_textures(&body_xpr)?;
        let body_textures = textures.len();
        textures.extend(parse_xpr_textures(&suit_xpr)?);

        fn draws_of(piece: &crate::formats::model::ModelPiece, first: usize, count: usize) -> Vec<(&[u32], Option<usize>, bool, Option<crate::formats::model::MaterialLook>)> {
            piece
                .materials
                .iter()
                .map(|m| {
                    let mut look = m.look;
                    if let Some(look) = &mut look {
                        look.layer = look.layer.map(|t| t + first);
                    }
                    (m.indices.as_slice(), m.base_texture(count).map(|t| t + first), m.cull_back, look)
                })
                .collect()
        }
        let suit_textures = textures.len() - body_textures;
        let xbe = std::fs::read(asset_path(DOAX_XBE_PATH))?;
        let rows = doax::bone_rows(&xbe, table)?;
        let skins = doax::joint_skins(&body_cat)?;
        let mut inputs: Vec<PieceInput> = Vec::new();
        let suit_pieces = doax::placed_objects(&suit_cat)?;
        let body_pieces = doax::placed_objects(&body_cat)?;
        // The same object can be listed twice: a joint record takes over from a plain place.
        let mut rigid_objects: Vec<usize> = Vec::new();
        for placed in &body_pieces {
            // Knees are listed twice: as a joint with no record, and with a slot, which is the one to use.
            let has_slot_entry = placed.slot.is_none() && body_pieces.iter().any(|p| p.object == placed.object && p.slot.is_some());
            if has_slot_entry || skins.iter().any(|skin| skin.object == placed.object) || rigid_objects.contains(&placed.object) {
                continue;
            }
            let Some(piece) = body.pieces.get(placed.object) else { continue };
            let multi = doax::multi_bend(&body_xpr, placed, &xbe, &rows);
            rigid_objects.push(placed.object);
            inputs.push(PieceInput {
                node: multi.as_ref().map_or(placed.bone, |m| m.groups[0].0),
                vertices: &piece.vertices,
                draws: draws_of(piece, 0, body_textures),
                parent: placed.bone,
                skin: None,
                bend: None,
                multi,
                welds: Vec::new(),
                translucent: false,
            });
        }
        for skin in &skins {
            let Some(piece) = body.pieces.get(skin.object) else { continue };
            let welds = skin
                .welds
                .iter()
                .filter_map(|weld| Some((rigid_objects.iter().position(|&o| o == weld.object)?, weld)))
                .flat_map(|(input, weld)| {
                    weld.links.iter().flat_map(move |(source, targets)| targets.iter().map(move |&target| (input, *source, target)))
                })
                .filter(|&(input, source, target)| {
                    source < body.pieces[rigid_objects[input]].vertices.len() && target < piece.vertices.len()
                })
                .collect();
            inputs.push(PieceInput {
                node: skin.node,
                vertices: &piece.vertices,
                draws: draws_of(piece, 0, body_textures),
                parent: skin.parent,
                skin: Some((skin.blend, skin.vertices.as_slice())),
                bend: None, multi: None,
                welds,
                translucent: false,
            });
        }
        let mut suit_inputs: Vec<(usize, usize)> = Vec::new();
        for placed in suit_pieces {
            suit_inputs.push((placed.object, inputs.len()));
            let Some(piece) = suit.pieces.get(placed.object) else { continue };
            let multi = doax::multi_bend(&suit_xpr, &placed, &xbe, &rows);
            inputs.push(PieceInput {
                node: multi.as_ref().map_or(placed.bone, |m| m.groups[0].0),
                vertices: &piece.vertices,
                draws: draws_of(piece, body_textures, suit_textures),
                parent: placed.bone,
                skin: None,
                bend: None,
                multi,
                welds: Vec::new(),
                translucent: false,
            });
        }
        let nodes = doax::rest_pose(&rows);

        let mut files = Vec::new();
        let mut shared_files = Vec::new();
        let mut motions = Vec::new();
        let mut animations = Vec::new();
        for file in doax::motion_files(&code) {
            let Ok(data) = dvd.read(&file) else { continue };
            for motion in doax::list_motions(&data) {
                motions.push((files.len(), motion));
                animations.push(AnimationEntry {
                    name: format!("{} {:04} ({} frames)", file.trim_end_matches(".mot"), motion.id, motion.frames),
                    file_key: file.clone(),
                    doa3_motion: None,
                    doa3_motion_id: motion.id,
                });
            }
            shared_files.push(file.contains("comon"));
            files.push(data);
        }
        let events_of = |name: &str| -> std::collections::HashMap<u16, Vec<doax::HandEvent>> {
            dvd.read(name).ok().and_then(|bin| doax::hand_events(&bin, &xbe).map_err(|e| log::warn!("{name}: {e:#}")).ok()).unwrap_or_default()
        };
        let events = doax::motion_files(&code).first().map_or_else(Default::default, |own| events_of(&own.replace(".mot", ".bin")));
        let shared_events = events_of("anime_cm.bin");
        let pose_shapes = doax::pose_shapes(&xbe)?;
        // Scripts count hand 0 as the first morph set's and hand 1 as the second's.
        let hands: Vec<_> = doa2u::hands(&body_cat)?
            .into_iter()
            .rev()
            .enumerate()
            .filter_map(|(slot, hand)| {
                let piece = rigid_objects.iter().position(|&object| object == hand.object)?;
                Some((slot as u16, hand, piece))
            })
            .collect();
        let mut chest_sets = Vec::new();
        for set in doax::chest_sets(&body_cat, &body_xpr, true) {
            if let Some(piece) = rigid_objects.iter().position(|&object| object == set.object) {
                chest_sets.push((set, piece));
            }
        }
        for set in doax::chest_sets(&suit_cat, &suit_xpr, false) {
            if let Some(&(_, piece)) = suit_inputs.iter().find(|(object, _)| *object == set.object) {
                chest_sets.push((set, piece));
            }
        }
        self.doa3_pose = None;
        self.doax_pose = Some(DoaxPose {
            rows, files, motions, playing: None, events, shared_events, shared_files, pose_shapes, hands,
            chest_sets, chest_spring: None, chest_frame: 0,
        });
        self.current_animations = animations;
        self.anim_panel.selected_anim_idx = None;
        self.anim_time = 0.0;
        self.loaded_name = ("DOAX".to_string(), display_name.clone(), stem.clone());
        self.mesh.set_lights(Some(self.current_stage_lights()));
        self.export_source = crate::export::ExportModel::new(&inputs, &textures);
        self.mesh.set_model(&self.ctx.device, &self.ctx.queue, &inputs, &textures);
        drop(inputs);
        self.mesh.apply_pose(&self.ctx.queue, &nodes);
        let count = self.current_animations.len();
        if count > 0 {
            self.select_motion(0);
        }
        Ok(format!("DOAX: {display_name} - {stem}, {count} motions"))
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
        let doa3_character = a0_doa3_character(&entry.id);

        if self.doa2u_xbe.is_none() {
            self.doa2u_xbe = Some(Xbe::from_bytes(std::fs::read(asset_path(DOA2U_XBE_PATH))?)?);
        }
        let xbe = self.doa2u_xbe.as_ref().context("DOA2.xbe not loaded")?;
        let stem = entry.costumes.get(costume_idx).context("unknown costume")?.clone();
        self.loaded_name = ("DOA2U".to_string(), display_name.clone(), stem.clone());
        // datahdd2.afs first: it holds the game's own copy where a name is in both.
        let archive = self
            .hdd2_archive
            .iter()
            .chain(self.dvd_archive.iter())
            .find(|archive| archive.get_by_name(&format!("{stem}.xpr")).is_some())
            .with_context(|| format!("no {stem}.xpr"))?;
        let mut xpr = archive.get_by_name(&format!("{stem}.xpr")).context("no xpr")?.to_vec();
        let mut cat = archive.get_by_name(&format!("{stem}.cat")).with_context(|| format!("no {stem}.cat"))?.to_vec();
        doa2u::decrypt_xpr(&mut xpr)?;
        doa2u::decrypt_cat(&mut cat)?;
        if cat.get(..4) == Some(&[1, 0, 0, 0][..]) {
            let home = self.doa3_xbe.as_ref().filter(|_| self.loadfile_archive.is_some()).and_then(|xbe| {
                let home = xbe.roster().ok()?.into_iter().find(|r| Some(r.id) == doa3_character)?;
                Some((xbe.skeleton(home.id).ok()?, home))
            });
            let skeleton = match &home {
                Some((skeleton, _)) => *skeleton,
                None => doa2u::skeleton(xbe, id)?,
            };
            let home = home.map(|(_, home)| home);
            return self.load_doa3_format_costume(char_idx, &display_name, &stem, &bin_id, id, skeleton, home, &xpr, &cat);
        }

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
        fn draws_of(piece: &crate::formats::model::ModelPiece, texture_count: usize) -> Vec<(&[u32], Option<usize>, bool, Option<crate::formats::model::MaterialLook>)> {
            piece
                .materials
                .iter()
                .map(|m| (m.indices.as_slice(), m.base_texture(texture_count), m.cull_back, m.look))
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
                    multi: None,
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
                bend: None, multi: None,
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
                    bend: None, multi: None,
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
                bend: None, multi: None,
                welds: Vec::new(),
                translucent: extra.translucent,
            });
        }
        let triangles: usize = inputs.iter().flat_map(|p| &p.draws).map(|(t, _, _, _)| t.len() / 3).sum();
        let piece_count = inputs.len();
        self.mesh.set_lights(Some(self.current_stage_lights()));
        self.export_source = crate::export::ExportModel::new(&inputs, &textures);
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
            self.bg_panel.frame_request = Some(crate::renderer::mesh::FramePart::Full);
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
            normal_rebuilds: doa2u::normal_rebuilds(&cat)?,
            copies: doa2u::vertex_copies(&cat, &xpr)?,
        };
        let chest = chest_morph.and_then(|(object, groups)| {
            let piece = chain_piece_of_chest?;
            log::info!("Chest spring on object {object}, {} groups", groups.len());
            Some(Doa3Chest { piece, groups, spring: None, frame: 0 })
        });
        self.doa3_pose = Some(Doa3Pose { doa2u_style: Some(style), skeleton, motion: None, chest, chains, motions_in_doa3: false });
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
        if self.doa3_pose.as_ref().is_some_and(|pose| pose.motions_in_doa3) {
            self.loadfile_archive.as_ref()
        } else {
            self.hdd_archive.as_ref()
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

    /// The `A0` series of DOA2U's `char_doa3` folder, in DOA3's format, posed by DOA3's solve
    /// with DOA2U's bones and motions, or those of `home` in DOA3's files. It has DOA3's
    /// shading and no chest spring. How the game itself uses these files was not read.
    #[allow(clippy::too_many_arguments)]
    fn load_doa3_format_costume(
        &mut self,
        char_idx: usize,
        display_name: &str,
        stem: &str,
        bin_id: &str,
        id: usize,
        skeleton: [glam::Vec3; 17],
        home: Option<RosterEntry>,
        xpr: &[u8],
        cat: &[u8],
    ) -> anyhow::Result<String> {
        let mut pieces = parse_xpr_model(xpr)?;
        let textures = parse_xpr_textures(xpr)?;
        let nodes = doa3::parse_node_objects(cat)?;
        doa3_morphs::apply_base_shapes(cat, &nodes.rigid, &mut pieces);
        let grouped = doa3::grouped_objects(cat);
        let props: Vec<_> = doa3_chains::parse_props(cat, pieces.len()).into_iter().filter(|prop| !grouped.contains(&prop.object)).collect();
        let overlays: Vec<_> = doa3_chains::parse_overlays(cat, pieces.len()).into_iter().filter(|overlay| !grouped.contains(&overlay.prop.object)).collect();
        for prop in props.iter().chain(overlays.iter().map(|overlay| &overlay.prop)) {
            let matrix = prop.matrix();
            for vertex in &mut pieces[prop.object].vertices {
                vertex.position = matrix.transform_point3(vertex.position.into()).into();
                vertex.normal = matrix.transform_vector3(vertex.normal.into()).into();
            }
        }
        let draws_of = |object: usize| {
            pieces.get(object).map(|piece| {
                let draws = piece.materials.iter().map(|m| (m.indices.as_slice(), m.base_texture(textures.len()), m.cull_back, m.look)).collect::<Vec<_>>();
                (piece.vertices.as_slice(), draws)
            })
        };
        let rigid = nodes.rigid.iter().filter_map(|&(node, object)| {
            let (vertices, draws) = draws_of(object)?;
            Some(PieceInput { node, vertices, draws, parent: node, skin: None, welds: Vec::new(), translucent: false, bend: None, multi: None })
        });
        let rigid_input = |object: usize| nodes.rigid.iter().filter(|&&(_, o)| o < pieces.len()).position(|&(_, o)| o == object);
        let skins = nodes.skins.iter().filter_map(|skin| {
            let (vertices, draws) = draws_of(skin.object)?;
            let welds = skin
                .welds
                .iter()
                .filter_map(|weld| Some((rigid_input(weld.object)?, weld)))
                .flat_map(|(input, weld)| weld.links.iter().flat_map(move |(source, targets)| targets.iter().map(move |&target| (input, *source, target))))
                .filter(|&(input, source, target)| {
                    let count = nodes.rigid.iter().filter(|&&(_, o)| o < pieces.len()).nth(input).map_or(0, |&(_, o)| pieces[o].vertices.len());
                    source < count && target < vertices.len()
                })
                .collect();
            Some(PieceInput { node: skin.node, vertices, draws, parent: skin.parent, skin: Some((skin.blend, skin.vertices.as_slice())), welds, translucent: false, bend: None, multi: None })
        });
        let mut inputs: Vec<PieceInput> = rigid.chain(skins).collect();

        // Only hair on the characters that keep no soft bodies on the body.
        let allowed = doa2u::allows_body_soft_bodies(id);
        let mut chains = doa3_chains::parse_chains(cat);
        if !allowed {
            chains.retain(|chain| chain.bone == 1);
        }
        let cloth = if allowed { doa3_cloth::parse_cloth(cat) } else { Vec::new() };
        let tubes = if allowed { doa3_tubes::parse_tubes(cat) } else { Vec::new() };
        let springs = if allowed { doa3_springs::parse_springs(cat) } else { Vec::new() };
        let mut chain_pieces: Vec<Option<(usize, usize)>> = vec![None; pieces.len()];
        let drawn = nodes.rigid.iter().copied().chain(nodes.skins.iter().map(|skin| (skin.node, skin.object))).filter(|&(_, object)| object < pieces.len());
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
                inputs.push(PieceInput { node: chain.bone, vertices, draws, parent: chain.bone, skin: None, welds: Vec::new(), translucent: false, bend: None, multi: None });
            }
        }
        for (prop, translucent) in props.iter().map(|prop| (prop, false)).chain(overlays.iter().map(|overlay| (&overlay.prop, overlay.translucent))) {
            if chain_pieces[prop.object].is_some() {
                continue;
            }
            let Some((vertices, draws)) = draws_of(prop.object) else { continue };
            chain_pieces[prop.object] = Some((inputs.len(), prop.bone));
            inputs.push(PieceInput { node: prop.bone, vertices, draws, parent: prop.bone, skin: None, welds: Vec::new(), translucent, bend: None, multi: None });
        }
        let triangles: usize = inputs.iter().flat_map(|p| &p.draws).map(|(t, _, _, _)| t.len() / 3).sum();
        let piece_count = inputs.len();
        self.mesh.set_lights(None);
        self.export_source = crate::export::ExportModel::new(&inputs, &textures);
        self.mesh.set_model(&self.ctx.device, &self.ctx.queue, &inputs, &textures);
        for chain in &chains {
            for target in chain.targets.iter().filter(|t| t.detached) {
                let Some(&Some((piece, _))) = chain_pieces.get(target.object) else { continue };
                let rest: Vec<glam::Vec3> = pieces[target.object].vertices.iter().map(|v| glam::Vec3::from(v.position)).collect();
                self.mesh.set_rest_positions(piece, chain.anchored_vertices(target, &rest));
            }
        }
        self.wireframe.update_lines(&self.ctx.queue, &[]);
        if self.framed_character != Some(char_idx) {
            self.camera.frame_character(1.8);
            self.bg_panel.frame_request = Some(crate::renderer::mesh::FramePart::Full);
            self.framed_character = Some(char_idx);
        }
        self.current_animations = match (&home, &self.loadfile_archive, &self.hdd_archive) {
            (Some(home), Some(archive), _) => doa3_animations(home, archive),
            (_, _, Some(archive)) => doa2u_animations(bin_id, archive),
            _ => Vec::new(),
        };
        let hands = doa3_morphs::parse_hands(cat, &nodes.rigid);
        let face = doa3_morphs::parse_face(cat, &nodes.rigid);
        let script = match (&home, &self.loadfile_archive, &self.hdd_archive) {
            (Some(home), Some(archive), _) => archive.get_by_index(home.bin_entry),
            (_, _, Some(archive)) => archive.get_by_name(&format!("{bin_id}.bin")),
            _ => None,
        };
        let hand_events = script.map(doa3_morphs::parse_hand_events).unwrap_or_default();
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
            character: home.as_ref().map_or(id, |home| home.id),
            pose_hands: Vec::new(),
            normal_rebuilds: Vec::new(),
            copies: Vec::new(),
        };
        self.doa3_pose = Some(Doa3Pose { doa2u_style: None, skeleton, motion: None, chest: None, chains, motions_in_doa3: home.is_some() });
        self.game_clock = 0.0;
        self.select_motion(0);
        Ok(format!("DOA2U: {display_name} {stem}, DOA3 format: {piece_count} pieces, {triangles} triangles, {} textures", textures.len()))
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
        self.loaded_name = ("DOA3".to_string(), display_name.clone(), format!("costume_{costume_idx:02}"));

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
                    .map(|m| (m.indices.as_slice(), m.base_texture(textures.len()), m.cull_back, m.look))
                    .collect::<Vec<_>>();
                (piece.vertices.as_slice(), draws)
            })
        };
        let rigid = nodes.rigid.iter().filter_map(|&(node, object)| {
            let (vertices, draws) = draws_of(object)?;
            Some(PieceInput { node, vertices, draws, parent: node, skin: None, welds: Vec::new(), translucent: false, bend: None, multi: None })
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
                bend: None, multi: None,
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
                    bend: None, multi: None,
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
                bend: None, multi: None,
            });
        }
        let triangles: usize = inputs.iter().flat_map(|p| &p.draws).map(|(t, _, _, _)| t.len() / 3).sum();
        let piece_count = inputs.len();
        self.mesh.set_lights(None);
        self.export_source = crate::export::ExportModel::new(&inputs, &textures);
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
            self.bg_panel.frame_request = Some(crate::renderer::mesh::FramePart::Full);
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
            normal_rebuilds: Vec::new(),
            copies: Vec::new(),
        };
        self.doa3_pose = Some(Doa3Pose { doa2u_style: None, skeleton, motion: None, chest, chains, motions_in_doa3: true });
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

    /// Writes the loaded costume to `exports/<game>/<character>/<costume>.glb`, with the
    /// playing motion or all of them.
    pub fn export_current(&mut self, all_motions: bool) -> anyhow::Result<PathBuf> {
        use anyhow::Context;
        use crate::export::{safe_name, write_glb, Export, ExportClip, ExportHand};

        if let Some(pose) = &self.doax2_pose {
            // A node per piece and per weight group, each driven by its DOAX bone. The chest
            // shapes are not exported.
            let chosen: Vec<usize> = if all_motions {
                (0..pose.motions.len()).collect()
            } else {
                self.anim_panel.selected_anim_idx.into_iter().collect()
            };
            let mut clips = Vec::new();
            for index in chosen {
                let Some(&(file, motion)) = pose.motions.get(index) else { continue };
                let mut frames = Vec::new();
                for frame in 0..motion.frames {
                    let values = doax::eval_motion_endian(&pose.files[file], &motion, frame as f32, true)?;
                    let bones = doax::solve_pose(&values, &pose.rows);
                    frames.push(pose.pieces.iter().map(|(bone, place)| bones[*bone] * *place).collect());
                }
                let hands = vec![Vec::new(); frames.len()];
                clips.push(ExportClip { name: self.current_animations[index].name.clone(), frames, hands });
            }
            let (game, character, costume) = &self.loaded_name;
            let path = PathBuf::from("exports").join(game).join(safe_name(character)).join(format!("{}.glb", safe_name(costume)));
            let names = pose.pieces.iter().enumerate().map(|(i, (bone, _))| format!("node_{i}_bone_{bone}")).collect();
            let parents = vec![None; pose.pieces.len()];
            write_glb(&Export { model: &self.export_source, bind: pose.rest_nodes.clone(), parents, names, clips: &clips, hands: &[] }, &path)?;
            return Ok(path);
        }
        if let Some(pose) = &self.doax_pose {
            // Hands, then the chest spring's sets, which follow the spring as in the viewer.
            let hands: Vec<ExportHand> = pose
                .hands
                .iter()
                .map(|(_, hand, piece)| ExportHand { piece: *piece, hand })
                .chain(pose.chest_sets.iter().map(|(set, piece)| ExportHand { piece: *piece, hand: set }))
                .collect();
            let bounce = self.bg_panel.bounce_scale;
            let chosen: Vec<usize> = if all_motions {
                (0..pose.motions.len()).collect()
            } else {
                self.anim_panel.selected_anim_idx.into_iter().collect()
            };
            let mut clips = Vec::new();
            for index in chosen {
                let Some(&(file, motion)) = pose.motions.get(index) else { continue };
                let events = if pose.shared_files[file] {
                    pose.shared_events.get(&(motion.id as u16 | doax::SHARED_MOTION_FLAG))
                } else {
                    pose.events.get(&(motion.id as u16))
                };
                let events = events.map_or(&[][..], Vec::as_slice);
                let (mut frames, mut hand_frames) = (Vec::new(), Vec::new());
                // The spring starts at rest on the motion's first frame and steps once a frame.
                let mut spring: Option<doa3::ChestSpring> = None;
                for frame in 0..motion.frames {
                    let values = doax::eval_motion(&pose.files[file], &motion, frame as f32)?;
                    let nodes = doax::solve_pose(&values, &pose.rows);
                    let mut states: Vec<(usize, usize, f32)> =
                        pose.hands.iter().map(|(slot, _, _)| doax::hand_blend(events, *slot, frame as f32, &pose.pose_shapes)).collect();
                    if !pose.chest_sets.is_empty() {
                        let spring = spring.get_or_insert_with(|| doa3::ChestSpring::with_parameters(&nodes[0], doax::CHEST_SPRING));
                        spring.step(&nodes[0]);
                        let (shape, weight) = doax::chest_weight(spring.value * bounce);
                        states.extend(pose.chest_sets.iter().map(|_| (0, shape, weight)));
                    }
                    frames.push(nodes.to_vec());
                    hand_frames.push(states);
                }
                clips.push(ExportClip { name: self.current_animations[index].name.clone(), frames, hands: hand_frames });
            }
            let (game, character, costume) = &self.loaded_name;
            let path = PathBuf::from("exports").join(game).join(safe_name(character)).join(format!("{}.glb", safe_name(costume)));
            let (parents, names) = doax::skeleton();
            let bind = doax::rest_pose(&pose.rows).to_vec();
            write_glb(&Export { model: &self.export_source, bind, parents, names, clips: &clips, hands: &hands }, &path)?;
            return Ok(path);
        }
        let pose = self.doa3_pose.as_ref().context("no costume loaded")?;
        let archive = self.motion_archive().context("no motion archive")?;
        let solve = |values: &[f32; doa3::TRACK_COUNT]| match pose.doa2u_style {
            Some(style) => doa2u::solve_pose(values, &pose.skeleton, style),
            None => doa3::solve_pose(values, &pose.skeleton),
        };
        // The first motion's first frame is the rest pose, so that all exports of a
        // character share it.
        let first = self.current_animations.iter().find_map(|a| a.doa3_motion.as_ref());
        let bind = first
            .and_then(|(entry, tracks)| doa3::eval_motion(archive.get_by_index(*entry)?, tracks, 0.0).ok())
            .map(|values| solve(&values))
            .context("no motion to take a rest pose from")?;
        let soft = &pose.chains;
        let hands: Vec<ExportHand> = soft
            .pose_hands
            .iter()
            .filter_map(|hand| {
                let &(piece, _) = soft.pieces.get(hand.object)?.as_ref()?;
                Some(ExportHand { piece, hand })
            })
            .collect();
        let chosen: Vec<&AnimationEntry> = if all_motions {
            self.current_animations.iter().collect()
        } else {
            self.anim_panel.selected_anim_idx.and_then(|i| self.current_animations.get(i)).into_iter().collect()
        };
        let mut clips = Vec::new();
        for entry in chosen {
            let Some((afs_entry, tracks)) = entry.doa3_motion.as_ref() else { continue };
            let Some(data) = archive.get_by_index(*afs_entry) else { continue };
            let events = soft.hand_events.get(&(entry.doa3_motion_id as u16));
            let mut states: Vec<_> = hands.iter().map(|h| h.hand.at_rest()).collect();
            let (mut frames, mut hand_frames) = (Vec::new(), Vec::new());
            for frame in 0..=doa3::motion_length(data, tracks) {
                let values = doa3::eval_motion(data, tracks, frame as f32)?;
                frames.push(solve(&values).to_vec());
                for (hand, state) in hands.iter().zip(&mut states) {
                    let index = soft.pose_hands.iter().position(|h| std::ptr::eq(h, hand.hand)).unwrap_or(0);
                    let script_hand = index ^ 1;
                    let event = events.and_then(|list| list.iter().rev().find(|e| e.targets(script_hand) && e.frame <= frame as u16));
                    state.step(event.map_or(0, |e| e.pose) as usize, event.map_or(1.0 / 26.0, doa3_morphs::HandEvent::rate));
                }
                hand_frames.push(states.iter().map(|s| s.state()).collect());
            }
            clips.push(ExportClip { name: entry.name.clone(), frames, hands: hand_frames });
        }
        let (game, character, costume) = &self.loaded_name;
        let path = PathBuf::from("exports").join(game).join(safe_name(character)).join(format!("{}.glb", safe_name(costume)));
        let (parents, names) = crate::export::standard_skeleton();
        write_glb(&Export { model: &self.export_source, bind: bind.to_vec(), parents, names, clips: &clips, hands: &hands }, &path)?;
        Ok(path)
    }

    /// Queues every costume of a character for `step_character_export`.
    pub fn start_character_export(&mut self, char_idx: usize, all_motions: bool) {
        let costumes = match self.char_panel.selected_source {
            GameSource::Doa2U => self.doa2u_characters.get(char_idx),
            GameSource::Doa3 => self.doa3_characters.get(char_idx),
            GameSource::Doax => self.doax_characters.get(char_idx),
            GameSource::Doax2 => self.doax2_characters.get(char_idx),
        }
        .map_or(0, |entry| entry.costumes.len());
        self.export_queue = Some(ExportQueue { character: char_idx, next: 0, count: costumes, written: 0, all_motions });
    }

    /// Writes the icon and a desktop entry to the user's folders, so that the desktop shows
    /// the icon. Wayland has no other way. Returns the entry's path.
    pub fn install_desktop_entry(&self) -> anyhow::Result<PathBuf> {
        let icon = self.app_icon.as_ref().ok_or_else(|| anyhow::anyhow!("the icon could not be read from assets/doa2u"))?;
        let data = PathBuf::from(std::env::var("HOME")?).join(".local/share");
        std::fs::create_dir_all(data.join("icons"))?;
        std::fs::create_dir_all(data.join("applications"))?;
        let icon_path = data.join("icons/doa-player.png");
        let mut encoder = png::Encoder::new(std::fs::File::create(&icon_path)?, icon.width, icon.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header()?.write_image_data(&icon.rgba)?;
        let entry = data.join("applications/doa-player.desktop");
        std::fs::write(
            &entry,
            format!(
                "[Desktop Entry]\nType=Application\nName=DOA Player\nExec={}\nIcon={}\nTerminal=false\nCategories=Graphics;Viewer;\nStartupWMClass=doa-player\n",
                std::env::current_exe()?.display(),
                icon_path.display(),
            ),
        )?;
        Ok(entry)
    }

    /// Exports the next queued costume, if any.
    pub fn step_character_export(&mut self) {
        let Some(mut queue) = self.export_queue.take() else { return };
        if queue.next >= queue.count {
            self.status_msg = format!("Exported {} of {} costumes to exports/", queue.written, queue.count);
            return;
        }
        self.load_character(queue.character, queue.next);
        match self.export_current(queue.all_motions) {
            Ok(_) => queue.written += 1,
            Err(error) => log::warn!("Export of costume {} failed: {error:#}", queue.next),
        }
        queue.next += 1;
        self.status_msg = format!("Exporting costume {} of {}...", queue.next, queue.count);
        self.export_queue = Some(queue);
    }

    /// Exports every costume of a character, loading each. Returns how many it wrote,
    /// and logs the ones that fail. Stays on the last one loaded.
    pub fn export_character(&mut self, char_idx: usize, all_motions: bool) -> usize {
        let costumes = match self.char_panel.selected_source {
            GameSource::Doa2U => self.doa2u_characters.get(char_idx),
            GameSource::Doa3 => self.doa3_characters.get(char_idx),
            GameSource::Doax => self.doax_characters.get(char_idx),
            GameSource::Doax2 => self.doax2_characters.get(char_idx),
        }
        .map_or(0, |entry| entry.costumes.len());
        let mut written = 0;
        for costume in 0..costumes {
            self.doa3_pose = None;
            self.load_character(char_idx, costume);
            match self.export_current(all_motions) {
                Ok(path) => {
                    log::info!("Exported {}", path.display());
                    written += 1;
                }
                Err(error) => log::warn!("Export of costume {costume} failed: {error:#}"),
            }
            // The command line exports in one loop, with no frames to do this. wgpu keeps
            // uploads and dropped models until a submit and a poll, and over the whole roster
            // that filled the machine's memory.
            self.ctx.queue.submit(std::iter::empty());
            self.ctx.device.poll(wgpu::Maintain::Wait);
        }
        written
    }

    /// Starts a motion of the loaded DOA3 character. Other characters ignore this.
    pub fn select_motion(&mut self, index: usize) {
        if let Some(pose) = &mut self.doax2_pose {
            if index < pose.motions.len() {
                pose.playing = Some(index);
                self.anim_panel.selected_anim_idx = Some(index);
                self.anim_panel.playing = true;
                self.anim_time = 0.0;
                self.update_doa3_pose();
            }
            return;
        }
        if let Some(pose) = &mut self.doax_pose {
            if index < pose.motions.len() {
                pose.playing = Some(index);
                self.anim_panel.selected_anim_idx = Some(index);
                self.anim_panel.playing = true;
                self.anim_time = 0.0;
                self.update_doa3_pose();
            }
            return;
        }
        let Some(pose) = &mut self.doa3_pose else { return };
        let Some(entry) = self.current_animations.get(index) else { return };
        let id = entry.doa3_motion_id;
        let Some((afs_entry, tracks)) = entry.doa3_motion else { return };
        let length = if pose.motions_in_doa3 { self.loadfile_archive.as_ref() } else { self.hdd_archive.as_ref() }
        .and_then(|archive| archive.get_by_index(afs_entry))
        .map_or(0, |data| doa3::motion_length(data, &tracks));
        pose.motion = Some(Doa3Motion { afs_entry, tracks, length: length as f32, id });
        self.anim_panel.selected_anim_idx = Some(index);
        self.anim_panel.playing = true;
        self.anim_time = 0.0;
        self.update_doa3_pose();
    }

    pub fn update_doa3_pose(&mut self) {
        if let Some(pose) = &mut self.doax2_pose {
            let Some(index) = pose.playing else { return };
            let (file, motion) = pose.motions[index];
            match doax::eval_motion_endian(&pose.files[file], &motion, self.anim_time, true) {
                Ok(values) => {
                    let bones = doax::solve_pose(&values, &pose.rows);
                    // The chest: DOAX's vertical spring on the chest bone, as measured in
                    // DOAX, on the up and down shapes. The sideways and forward pairs are not
                    // driven; how DOAX2 drives them is not known.
                    if !pose.chest.is_empty() {
                        let clock = self.game_clock as u64;
                        let spring = pose.chest_spring.get_or_insert_with(|| doa3::ChestSpring::with_parameters(&bones[0], doax::CHEST_SPRING));
                        while pose.chest_frame < clock {
                            spring.step(&bones[0]);
                            pose.chest_frame += 1;
                        }
                        // DOAX's shape 1, for positive values, lowers the chest; shape 2 raises it.
                        let (shape, weight) = doax::chest_weight(spring.value * self.bg_panel.bounce_scale);
                        let target = if shape == 1 { 2 } else { 1 };
                        for (piece, positions) in &pose.chest {
                            self.mesh.set_rest_positions(*piece, positions.iter().enumerate().map(|(v, p)| (v, p[0].lerp(p[target], weight))));
                        }
                    }
                    // Hands: the scripts' poses, blended as in DOAX.
                    let key = if pose.shared_files[file] { motion.id as u16 | doax::SHARED_MOTION_FLAG } else { motion.id as u16 };
                    let events = pose.events.get(&key).map_or(&[][..], Vec::as_slice);
                    let entries: Vec<usize> = (0..64).map(crate::formats::doax2::hand_entry).collect();
                    for (slot, piece, hand) in &pose.hands {
                        let (from, to, weight) = doax::hand_blend(events, *slot, self.anim_time, &entries);
                        let base = hand.poses.get(1).and_then(Option::as_ref);
                        let (Some(a), Some(b)) = (hand.poses.get(from).and_then(Option::as_ref).or(base), hand.poses.get(to).and_then(Option::as_ref).or(base)) else { continue };
                        self.mesh.set_rest_positions(
                            *piece,
                            hand.groups.iter().zip(a.iter().zip(b)).flat_map(|(group, (p, q))| {
                                let position = glam::Vec3::from(*p).lerp(glam::Vec3::from(*q), weight);
                                group.iter().map(move |&v| (v, position))
                            }),
                        );
                    }
                    // Blinks: the scripts' op 22, on the head's eye shapes (the face set with
                    // three entries), from open to the asked shape and back.
                    let blinks = pose.blinks.get(&key).map_or(&[][..], Vec::as_slice);
                    if let Some((piece, eyes)) = pose.faces.iter().find(|(_, set)| set.poses.len() == 4) {
                        let started = blinks.iter().rev().find(|(frame, _)| *frame as f32 <= self.anim_time);
                        let (shape, weight) = started.map_or((2, 0.0), |&(frame, shape)| (shape as usize, crate::formats::doax2::blink_weight(self.anim_time - frame as f32)));
                        let base = eyes.poses.get(1).and_then(Option::as_ref);
                        let target = eyes.poses.get(shape).and_then(Option::as_ref).or(base);
                        if let (Some(a), Some(b)) = (base, target) {
                            self.mesh.set_rest_positions(
                                *piece,
                                eyes.groups.iter().zip(a.iter().zip(b)).flat_map(|(group, (p, q))| {
                                    let position = glam::Vec3::from(*p).lerp(glam::Vec3::from(*q), weight);
                                    group.iter().map(move |&v| (v, position))
                                }),
                            );
                        }
                    }
                    let nodes: Vec<glam::Mat4> = pose.pieces.iter().map(|(bone, place)| bones[*bone] * *place).collect();
                    self.mesh.apply_pose(&self.ctx.queue, &nodes);
                }
                Err(e) => log::warn!("Could not evaluate the DOAX2 motion: {e:#}"),
            }
            return;
        }
        if let Some(pose) = &mut self.doax_pose {
            let Some(index) = pose.playing else { return };
            let (file, motion) = pose.motions[index];
            let events = if pose.shared_files[file] {
                pose.shared_events.get(&(motion.id as u16 | doax::SHARED_MOTION_FLAG))
            } else {
                pose.events.get(&(motion.id as u16))
            };
            let events = events.map_or(&[][..], Vec::as_slice);
            for (slot, hand, piece) in &mut pose.hands {
                let (from, to, weight) = doax::hand_blend(events, *slot, self.anim_time, &pose.pose_shapes);
                hand.set_blend(from, to, weight);
                self.mesh.set_rest_positions(
                    *piece,
                    hand.positions().flat_map(|(vertices, value)| vertices.iter().map(move |&vertex| (vertex, glam::Vec3::from(value)))),
                );
            }
            match doax::eval_motion(&pose.files[file], &motion, self.anim_time) {
                Ok(values) => {
                    let nodes = doax::solve_pose(&values, &pose.rows);
                    if !pose.chest_sets.is_empty() {
                        let clock = self.game_clock as u64;
                        let spring = pose.chest_spring.get_or_insert_with(|| doa3::ChestSpring::with_parameters(&nodes[0], doax::CHEST_SPRING));
                        // Holding the pose between 60 Hz frames, as for DOA3.
                        while pose.chest_frame < clock {
                            spring.step(&nodes[0]);
                            pose.chest_frame += 1;
                        }
                        let (shape, weight) = doax::chest_weight(spring.value * self.bg_panel.bounce_scale);
                        for (set, piece) in &mut pose.chest_sets {
                            set.set_blend(0, shape, weight);
                            self.mesh.set_rest_positions(
                                *piece,
                                set.positions().flat_map(|(vertices, value)| vertices.iter().map(move |&vertex| (vertex, glam::Vec3::from(value)))),
                            );
                        }
                    }
                    self.mesh.apply_pose(&self.ctx.queue, &nodes);
                }
                Err(e) => log::warn!("Could not evaluate the motion: {e:#}"),
            }
            return;
        }
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
                    for rebuild in &soft.normal_rebuilds {
                        let Some(&Some((piece, _))) = soft.pieces.get(rebuild.object) else { continue };
                        let normals = rebuild.normals(|vertex| self.mesh.rest_position(piece, vertex));
                        self.mesh.set_rest_normals(piece, normals.into_iter());
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
            let doax_length = self.doax_pose.as_ref().and_then(|p| Some(p.motions[p.playing?].1.frames as f32))
                .or_else(|| self.doax2_pose.as_ref().and_then(|p| Some(p.motions[p.playing?].1.frames as f32)));
            let length = doax_length.unwrap_or_else(|| self.doa3_pose.as_ref().and_then(|p| p.motion.as_ref()).map_or(0.0, |m| m.length));
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
        self.doax_pose.as_ref().is_some_and(|pose| !pose.chest_sets.is_empty())
            || self.doax2_pose.as_ref().is_some_and(|pose| !pose.chest.is_empty())
            || self.doa3_pose.as_ref().is_some_and(|pose| pose.chest.is_some())
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
