use crate::app::{CharacterEntry, GameSource};

pub struct CharSelectPanel {
    pub selected_source: GameSource,
    pub selected_char_idx: Option<usize>,
    pub selected_costume_idx: usize,
    /// The hairstyle picked for games with separate heads; `None` is the costume's own.
    pub selected_hair_idx: Option<usize>,
    /// The variant of the costume's records that have them (kind 0xFE), 0 being the default.
    pub selected_variant: usize,
    /// How many the loaded costume has, set at load.
    pub variant_count: usize,
    /// DOAX3: the face's expressions (one-frame motions of `*_FACE_MPM.mpm`), set at load, and
    /// the one shown, 0 being the model's rest face.
    pub face_count: usize,
    pub selected_face: usize,
    /// Exports with all motions, not only the playing one.
    pub export_all_motions: bool,
    filter: String,
    /// The app's icon, beside its name.
    pub logo: Option<egui::TextureHandle>,
}

impl CharSelectPanel {
    pub fn new() -> Self {
        Self {
            selected_source: GameSource::Doa2U,
            selected_char_idx: None,
            selected_costume_idx: 0,
            selected_hair_idx: None,
            selected_variant: 0,
            variant_count: 1,
            face_count: 0,
            selected_face: 0,
            export_all_motions: false,
            filter: String::new(),
            logo: None,
        }
    }

    /// The game buttons, the character list and the costume's choices, drawn into `ui`.
    /// `on_char_selected` asks for a costume to be loaded.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        characters: &[CharacterEntry],
        on_char_selected: &mut dyn FnMut(usize, usize),
    ) {
        ui.horizontal_wrapped(|ui| {
            for (source, name) in [
                (GameSource::Doa2U, "DOA2U"),
                (GameSource::Doa3, "DOA3"),
                (GameSource::Doax, "DOAX"),
                (GameSource::Doax2, "DOAX2"),
                (GameSource::Doa4, "DOA4"),
                (GameSource::Doax3, "DOAX3"),
            ] {
                if ui.selectable_label(self.selected_source == source, name).clicked() && self.selected_source != source {
                    self.selected_source = source;
                    self.selected_char_idx = None;
                    self.selected_hair_idx = None;
                    self.selected_costume_idx = 0;
                }
            }
        });
        {
            {
                ui.separator();
                ui.label("Personajes:");

                egui::TextEdit::singleline(&mut self.filter)
                    .hint_text("Filtrar…")
                    .show(ui);

                let filter_lower = self.filter.to_lowercase();

                // Each list scrolls on its own, so that long costume lists stay reachable.
                egui::ScrollArea::vertical().id_salt("characters").max_height(220.0).auto_shrink([false, true]).show(ui, |ui| {
                    // Fighters first, then the others under their own heading.
                    for intro in [false, true] {
                        let mut heading_shown = !intro;
                        for (i, entry) in characters.iter().enumerate() {
                            if entry.intro != intro
                                || (!filter_lower.is_empty()
                                    && !entry.display_name.to_lowercase().contains(&filter_lower))
                            {
                                continue;
                            }
                            if !heading_shown {
                                ui.separator();
                                ui.weak("Intro y otros");
                                heading_shown = true;
                            }
                            let selected = self.selected_char_idx == Some(i);
                            if ui.selectable_label(selected, &entry.display_name).clicked() {
                                if !selected {
                                    self.selected_char_idx = Some(i);
                                    self.selected_costume_idx = 0;
                                    self.selected_hair_idx = None;
                                    on_char_selected(i, crate::app::DEFAULT_COSTUME);
                                }
                            }
                        }
                    }
                });

                // Costume selector
                if let Some(char_idx) = self.selected_char_idx {
                    if let Some(entry) = characters.get(char_idx) {
                        if entry.hairstyles.len() > 1 {
                            ui.separator();
                            ui.label("Peinado:");
                            ui.horizontal_wrapped(|ui| {
                                let mut pick = None;
                                if ui.selectable_label(self.selected_hair_idx.is_none(), "Propio").on_hover_text("La cabeza con la que viene el traje").clicked() {
                                    pick = Some(None);
                                }
                                for (hi, name) in entry.hairstyles.iter().enumerate() {
                                    // The last two letters tell the heads apart.
                                    let short = name.get(name.len().saturating_sub(2)..).unwrap_or(name);
                                    if ui.selectable_label(self.selected_hair_idx == Some(hi), short).on_hover_text(name).clicked() {
                                        pick = Some(Some(hi));
                                    }
                                }
                                if let Some(pick) = pick.filter(|pick| *pick != self.selected_hair_idx) {
                                    self.selected_hair_idx = pick;
                                    on_char_selected(char_idx, self.selected_costume_idx);
                                }
                            });
                        }
                        if self.face_count > 0 {
                            ui.separator();
                            ui.label("Cara:").on_hover_text("Las expresiones del archivo de la cara, en su orden; no se sabe cuál es cuál. Reposo es la cara del modelo.");
                            ui.horizontal_wrapped(|ui| {
                                if ui.selectable_label(self.selected_face == 0, "Reposo").clicked() {
                                    self.selected_face = 0;
                                }
                                for face in 1..=self.face_count {
                                    if ui.selectable_label(self.selected_face == face, (face - 1).to_string()).clicked() {
                                        self.selected_face = face;
                                    }
                                }
                            });
                        }
                        if self.variant_count > 1 {
                            ui.separator();
                            ui.label("Variante:").on_hover_text("Registros que el traje tiene en más de una versión, como un objeto puesto o en la mano. El juego elige uno al jugar; 0 es el de por defecto.");
                            ui.horizontal_wrapped(|ui| {
                                for variant in 0..self.variant_count {
                                    if ui.selectable_label(self.selected_variant == variant, variant.to_string()).clicked() && variant != self.selected_variant {
                                        self.selected_variant = variant;
                                        on_char_selected(char_idx, self.selected_costume_idx);
                                    }
                                }
                            });
                        }
                        ui.separator();
                        ui.label("Traje:");
                        // The up and down keys walk the list, loading each costume, unless a
                        // text box has the keyboard.
                        let step = if ui.ctx().wants_keyboard_input() {
                            0
                        } else {
                            ui.input(|i| i.key_pressed(egui::Key::ArrowDown) as i32 - i.key_pressed(egui::Key::ArrowUp) as i32)
                        };
                        let moved_to = (step != 0 && !entry.costumes.is_empty()).then(|| {
                            (self.selected_costume_idx as i32 + step).clamp(0, entry.costumes.len() as i32 - 1) as usize
                        }).filter(|&next| next != self.selected_costume_idx);
                        if let Some(next) = moved_to {
                            self.selected_costume_idx = next;
                            on_char_selected(char_idx, next);
                        }
                        egui::ScrollArea::vertical().id_salt("costumes").max_height(260.0).auto_shrink([false, true]).show(ui, |ui| {
                            for (ci, name) in entry.costumes.iter().enumerate() {
                                let selected = self.selected_costume_idx == ci;
                                let label = ui.selectable_label(selected, name);
                                if moved_to == Some(ci) {
                                    label.scroll_to_me(None);
                                }
                                if label.clicked() {
                                    self.selected_costume_idx = ci;
                                    on_char_selected(char_idx, ci);
                                }
                            }
                        });
                    }
                }
            }
        }
    }
}
