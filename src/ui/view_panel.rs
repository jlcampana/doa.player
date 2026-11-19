//! The "Vista" panel: actions (export), the character and costume, how the model is drawn,
//! lighting, animation, physics, voices and camera shots.

use crate::app::{self, App, GameSource};

/// What the panels ask the window side to do. Loading and exporting run as jobs, away from
/// the event loop, so the panels only ask for them.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Load(usize, usize),
    Motion(usize),
    Voice(usize),
    /// The loaded costume to `exports/`.
    ExportCostume,
    /// The loaded costume to a file the user picks.
    ExportCostumeAs,
    /// Every costume of the selected character to `exports/`.
    ExportCharacter,
    OpenExportsFolder,
    ShowJob,
    /// Takes a panel out to its own window (true) or back.
    Detach(Panel, bool),
}

/// Panels that can leave the main window.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Panel {
    View,
    Layers,
}

impl Panel {
    pub fn title(self) -> &'static str {
        match self {
            Panel::View => "DOA Player · Vista",
            Panel::Layers => "DOA Player · Capas",
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

/// The button that takes a panel to its own window, or brings it back.
pub fn detach_button(ui: &mut egui::Ui, detached: bool, panel: Panel, actions: &mut Vec<Action>) {
    let (label, hint) = if detached {
        ("↙ Volver a la ventana principal", "También vuelve al cerrar esta ventana")
    } else {
        ("↗ Sacar a otra ventana", "El panel pasa a una ventana propia que puedes mover fuera de la aplicación")
    };
    if ui.small_button(label).on_hover_text(hint).clicked() {
        actions.push(Action::Detach(panel, !detached));
    }
}

/// The characters of the game picked in the panel.
pub fn characters(app: &App) -> &[app::CharacterEntry] {
    match app.char_panel.selected_source {
        GameSource::Doa2U => &app.doa2u_characters,
        GameSource::Doa3 => &app.doa3_characters,
        GameSource::Doax => &app.doax_characters,
        GameSource::Doax2 => &app.doax2_characters,
        GameSource::Doa4 => &app.doa4_characters,
        GameSource::Doax3 => &app.doax3_characters,
    }
}

pub fn view_panel(ui: &mut egui::Ui, app: &mut App, actions: &mut Vec<Action>, last_job: bool) {
    ui.horizontal(|ui| {
        if let Some(logo) = &app.char_panel.logo {
            ui.image((logo.id(), egui::Vec2::splat(26.0)));
        }
        ui.heading("DOA Player");
    });

    ui.separator();
    ui.heading("Exportar a Blender (.glb)");
    let loaded = app.mesh.is_loaded();
    ui.horizontal_wrapped(|ui| {
        if ui.add_enabled(loaded, egui::Button::new("Traje")).on_hover_text("Escribe este traje en exports/").clicked() {
            actions.push(Action::ExportCostume);
        }
        if ui.add_enabled(loaded, egui::Button::new("Traje como…")).on_hover_text("Escribe este traje donde elijas").clicked() {
            actions.push(Action::ExportCostumeAs);
        }
        let character = app.char_panel.selected_char_idx.is_some();
        if ui.add_enabled(character, egui::Button::new("Todos los trajes")).on_hover_text("Cada traje de este personaje en su propio archivo, en exports/").clicked() {
            actions.push(Action::ExportCharacter);
        }
    });
    ui.checkbox(&mut app.char_panel.export_all_motions, "Todas las animaciones")
        .on_hover_text("Si no, solo la que se reproduce. Los archivos crecen y Blender tarda en importarlos.");
    ui.horizontal(|ui| {
        if ui.small_button("Abrir la carpeta exports").clicked() {
            actions.push(Action::OpenExportsFolder);
        }
        if last_job && ui.small_button("Ver el último proceso").clicked() {
            actions.push(Action::ShowJob);
        }
    });

    ui.separator();
    egui::CollapsingHeader::new(egui::RichText::new("Personaje").heading()).default_open(true).show(ui, |ui| {
        // Borrowed field by field: the list and the panel's state live side by side in the app.
        let App { char_panel, doa2u_characters, doa3_characters, doax_characters, doax2_characters, doa4_characters, doax3_characters, .. } = app;
        let characters = match char_panel.selected_source {
            GameSource::Doa2U => doa2u_characters,
            GameSource::Doa3 => doa3_characters,
            GameSource::Doax => doax_characters,
            GameSource::Doax2 => doax2_characters,
            GameSource::Doa4 => doa4_characters,
            GameSource::Doax3 => doax3_characters,
        };
        char_panel.ui(ui, characters, &mut |ci, costume| actions.push(Action::Load(ci, costume)));
    });

    egui::CollapsingHeader::new(egui::RichText::new("Vista").heading()).default_open(true).show(ui, |ui| {
        let backgrounds = app.background_options.clone();
        app.bg_panel.view_ui(ui, &backgrounds);
    });

    egui::CollapsingHeader::new(egui::RichText::new("Iluminación").heading()).default_open(false).show(ui, |ui| {
        let lit = app.mesh.is_lit();
        let names: Vec<String> = app.stage_light_list().iter().map(|(name, _)| name.clone()).collect();
        app.bg_panel.lighting_ui(ui, lit, &names);
    });

    egui::CollapsingHeader::new(egui::RichText::new("Animación").heading()).default_open(true).show(ui, |ui| {
        let animations = std::mem::take(&mut app.current_animations);
        app.anim_panel.ui(ui, &animations, &mut |index| actions.push(Action::Motion(index)));
        app.current_animations = animations;
    });

    egui::CollapsingHeader::new(egui::RichText::new("Física").heading()).default_open(false).show(ui, |ui| {
        app.anim_panel.physics_ui(ui);
        if app.has_chest_spring() {
            let before = app.bg_panel.bounce_scale;
            ui.add(egui::Slider::new(&mut app.bg_panel.bounce_scale, 0.0..=3.0).text("Rebote del pecho"));
            // Keeps the slider live while the animation is paused.
            if app.bg_panel.bounce_scale != before {
                app.update_doa3_pose();
            }
        }
        ui.weak("Física de una sola pieza: en Capas.");
    });

    // DOA2U's clips have names. DOA3's do not, and which is whose is unknown there.
    if app.char_panel.selected_source == GameSource::Doa2U {
        egui::CollapsingHeader::new(egui::RichText::new("Voces").heading()).default_open(false).show(ui, |ui| {
            let selected = app.char_panel.selected_char_idx.and_then(|i| app.doa2u_characters.get(i)).map(|c| c.display_name.clone());
            let character = if app.voice_panel.show_others { None } else { selected };
            let clips = if app.voice_panel.show_others || character.is_some() { app.voices.clips(app::doa2u_voice_path(), character.as_deref()) } else { Vec::new() };
            app.voice_panel.ui(ui, &clips, &mut |entry| actions.push(Action::Voice(entry)));
        });
    }

    egui::CollapsingHeader::new(egui::RichText::new("Cámara").heading()).default_open(true).show(ui, |ui| {
        app.bg_panel.camera_ui(ui);
        ui.weak("Izquierdo: girar · Derecho: desplazar · Rueda: acercar");
    });
}
