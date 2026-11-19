//! The command line: a costume to open with the camera placed, exports, and the icon.

use crate::app::{self, App};
use crate::renderer;

/// `doa_player --load "Jann Lee" 0 [motion] [yaw degrees] [distance] [target height]`
/// opens with a DOA3 character loaded, optionally paused with the camera placed.
/// `--doa2u` looks the name up among DOA2U characters instead.
/// `--doax` does so among DOAX's.
/// `--install-icon` writes the icon and a desktop entry to the user's folders, and exits.
/// `--export-all` writes every costume of all games, without `--load`, and exits. With `--doax`,
/// only that game's.
/// `--export` writes that costume to `exports/` and exits, with the chosen motion or, with
/// `--all-motions`, all of them.
/// `--detach personaje|vista|animacion|capas` opens that panel in its own window (read in `main.rs`).
/// `--no-panels` leaves only the 3D view and the status bar, for screenshots.
/// Also takes `--anime`, `--no-textures`, `--mesh-only`, `--wind`, `--bounce <scale>`, `--motion-id <id>` and
/// `--frame <frame>`, which holds the motion on that frame, and for DOAX3 `--face <n>`, an
/// expression.
pub fn load_from_arguments(app: &mut App) {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--install-icon") {
        match app.install_desktop_entry() {
            Ok(path) => println!("Wrote {}", path.display()),
            Err(error) => println!("Could not install the icon: {error:#}"),
        }
        std::process::exit(0);
    }
    if args.iter().any(|a| a == "--export-all") {
        let all_motions = args.iter().any(|a| a == "--all-motions");
        let mut written = 0;
        // With `--doax` or `--doax2`, only that game, whose suits take far longer than the
        // other two games. DOAX2 is left out of the default run.
        let only_doax = args.iter().any(|a| a == "--doax");
        let only_doax2 = args.iter().any(|a| a == "--doax2");
        for source in [app::GameSource::Doa2U, app::GameSource::Doa3, app::GameSource::Doax, app::GameSource::Doax2] {
            if (only_doax && source != app::GameSource::Doax) || (only_doax2 && source != app::GameSource::Doax2) {
                continue;
            }
            if source == app::GameSource::Doax2 && !only_doax2 {
                continue;
            }
            app.char_panel.selected_source = source;
            let count = match source {
                app::GameSource::Doa2U => app.doa2u_characters.len(),
                app::GameSource::Doa3 => app.doa3_characters.len(),
                app::GameSource::Doax => app.doax_characters.len(),
                app::GameSource::Doax2 => app.doax2_characters.len(),
                app::GameSource::Doa4 => app.doa4_characters.len(),
                app::GameSource::Doax3 => app.doax3_characters.len(),
            };
            for index in 0..count {
                written += app.export_character(index, all_motions, &|_| {});
            }
        }
        println!("Exported {written} costumes to exports/");
        std::process::exit(0);
    }
    let [flag, name, rest @ ..] = args.as_slice() else { return };
    if flag != "--load" {
        return;
    }
    let number = |i: usize| rest.get(i).and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
    let doa2u = args.iter().any(|a| a == "--doa2u");
    let doax = args.iter().any(|a| a == "--doax");
    let doax2 = args.iter().any(|a| a == "--doax2");
    let doa4 = args.iter().any(|a| a == "--doa4");
    let doax3 = args.iter().any(|a| a == "--doax3");
    let characters = if doax3 { &app.doax3_characters } else if doa4 { &app.doa4_characters } else if doax2 { &app.doax2_characters } else if doax { &app.doax_characters } else if doa2u { &app.doa2u_characters } else { &app.doa3_characters };
    let Some(index) = characters.iter().position(|c| c.display_name.eq_ignore_ascii_case(name)) else {
        log::warn!("Unknown character: {}", name);
        // Without a costume an export has nothing to write, and its hidden window would wait.
        if args.iter().any(|a| a == "--export") {
            eprintln!("Export failed: unknown character {name}");
            std::process::exit(1);
        }
        return;
    };
    app.char_panel.selected_source = if doax3 { app::GameSource::Doax3 } else if doa4 { app::GameSource::Doa4 } else if doax2 { app::GameSource::Doax2 } else if doax { app::GameSource::Doax } else if doa2u { app::GameSource::Doa2U } else { app::GameSource::Doa3 };
    app.char_panel.selected_char_idx = Some(index);
    app.char_panel.selected_costume_idx = number(0);
    // `--hair N` picks the character's N-th head, for games with separate heads.
    app.char_panel.selected_variant = args.iter().position(|a| a == "--variant").and_then(|i| args.get(i + 1)?.parse().ok()).unwrap_or(0);
    app.char_panel.selected_hair_idx = args.iter().position(|a| a == "--hair").and_then(|i| args.get(i + 1)?.parse().ok());
    if let Some(scale) = args.iter().position(|a| a == "--bounce").and_then(|i| args.get(i + 1)?.parse().ok()) {
        app.bg_panel.bounce_scale = scale;
    }
    app.load_character(index, number(0));
    // A `-` for the motion leaves the costume at rest (DOAX3 has no default motion).
    if rest.get(1).map(String::as_str) != Some("-") {
        app.select_motion(number(1));
    }
    let value_of = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)?.parse::<usize>().ok());
    if let Some(id) = value_of("--motion-id") {
        if let Some(index) = app.current_animations.iter().position(|a| a.doa3_motion_id == id) {
            app.select_motion(index);
        }
    }

    app.bg_panel.anime_shading = args.iter().any(|a| a == "--anime");
    app.bg_panel.game_lights = !args.iter().any(|a| a == "--no-game-lights");
    app.bg_panel.no_textures = args.iter().any(|a| a == "--no-textures");
    app.bg_panel.mesh_only = args.iter().any(|a| a == "--mesh-only");
    // Arguments set the camera themselves, unless they ask for a part.
    app.bg_panel.frame_request = None;
    if let Some(part) = args.iter().position(|a| a == "--view").and_then(|i| args.get(i + 1)) {
        use renderer::mesh::FramePart;
        app.bg_panel.frame_request = match part.as_str() {
            "full" => Some(FramePart::Full),
            "head" => Some(FramePart::Head),
            "hands" => Some(FramePart::Hands),
            "feet" => Some(FramePart::Feet),
            _ => None,
        };
    }
    if let Some(relief) = value_of("--relief") {
        app.bg_panel.relief = relief as f32 / 100.0;
    }
    if let Some(lights) = value_of("--lights") {
        app.bg_panel.lights = lights.min(app.stage_light_list().len().saturating_sub(1));
    }
    if args.iter().any(|a| a == "--export") {
        let all_motions = args.iter().any(|a| a == "--all-motions");
        match app.export_current(all_motions, true) {
            Ok(path) => println!("Exported {}", path.display()),
            Err(error) => eprintln!("Export failed: {error:#}"),
        }
        std::process::exit(0);
    }
    app.anim_panel.wind = args.iter().any(|a| a == "--wind");
    app.anim_panel.physics = !args.iter().any(|a| a == "--no-physics");
    let float = |i: usize| rest.get(i).and_then(|v| v.parse::<f32>().ok());
    if let Some(yaw) = float(2) {
        app.camera.yaw = yaw.to_radians();
        app.camera.pitch = 0.05;
        app.anim_panel.playing = false;
    }
    if let Some(distance) = float(3) {
        app.camera.distance = distance;
    }
    if let Some(height) = float(4) {
        app.camera.target.y = height;
    }
    // DOAX3: `--face N` shows the face's expression N - 1 (0 is the rest face).
    if let Some(face) = value_of("--face") {
        app.char_panel.selected_face = face.min(app.char_panel.face_count);
        app.update_doa3_pose();
    }
    if let Some(frame) = value_of("--frame") {
        app.anim_panel.playing = false;
        app.anim_time = frame as f32;
    }
}
