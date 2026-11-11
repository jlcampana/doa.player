use crate::app::{CharacterEntry, GameSource};

pub struct CharSelectPanel {
    pub visible: bool,
    pub selected_source: GameSource,
    pub selected_char_idx: Option<usize>,
    pub selected_costume_idx: usize,
    /// The hairstyle picked for games with separate heads; `None` is the costume's own.
    pub selected_hair_idx: Option<usize>,
    /// Exports with all motions, not only the playing one.
    pub export_all_motions: bool,
    pub export_request: Option<ExportRequest>,
    filter: String,
    /// The app's icon, beside its name.
    pub logo: Option<egui::TextureHandle>,
}

/// What the Export buttons ask for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExportRequest {
    Costume,
    Character,
}

impl CharSelectPanel {
    pub fn new() -> Self {
        Self {
            visible: true,
            selected_source: GameSource::Doa2U,
            selected_char_idx: None,
            selected_costume_idx: 0,
            selected_hair_idx: None,
            export_all_motions: false,
            export_request: None,
            filter: String::new(),
            logo: None,
        }
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        characters: &[CharacterEntry],
        on_char_selected: &mut dyn FnMut(usize, usize),
    ) {
        if !self.visible {
            return;
        }

        egui::SidePanel::left("char_select")
            .resizable(true)
            .default_width(200.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if let Some(logo) = &self.logo {
                        ui.image((logo.id(), egui::Vec2::splat(26.0)));
                    }
                    ui.heading("DOA Player");
                });
                ui.separator();

                // Game source toggle
                ui.horizontal(|ui| {
                    ui.label("Game:");
                    if ui.selectable_label(
                        self.selected_source == GameSource::Doa2U, "DOA2U"
                    ).clicked() {
                        self.selected_source = GameSource::Doa2U;
                        self.selected_char_idx = None;
                        self.selected_hair_idx = None;
                        self.selected_costume_idx = 0;
                    }
                    if ui.selectable_label(
                        self.selected_source == GameSource::Doa3, "DOA3"
                    ).clicked() {
                        self.selected_source = GameSource::Doa3;
                        self.selected_char_idx = None;
                        self.selected_hair_idx = None;
                        self.selected_costume_idx = 0;
                    }
                    if ui.selectable_label(
                        self.selected_source == GameSource::Doax, "DOAX"
                    ).clicked() {
                        self.selected_source = GameSource::Doax;
                        self.selected_char_idx = None;
                        self.selected_hair_idx = None;
                        self.selected_costume_idx = 0;
                    }
                    if ui.selectable_label(
                        self.selected_source == GameSource::Doax2, "DOAX2"
                    ).clicked() {
                        self.selected_source = GameSource::Doax2;
                        self.selected_char_idx = None;
                        self.selected_hair_idx = None;
                        self.selected_costume_idx = 0;
                    }
                    if ui.selectable_label(
                        self.selected_source == GameSource::Doa4, "DOA4"
                    ).clicked() {
                        self.selected_source = GameSource::Doa4;
                        self.selected_char_idx = None;
                        self.selected_hair_idx = None;
                        self.selected_costume_idx = 0;
                    }
                });

                ui.separator();
                ui.label("Characters:");

                egui::TextEdit::singleline(&mut self.filter)
                    .hint_text("Filter...")
                    .show(ui);

                let filter_lower = self.filter.to_lowercase();

                // Each list scrolls on its own, so that long costume lists stay reachable.
                let list_height = (ui.available_height() * 0.5).max(80.0);
                egui::ScrollArea::vertical().id_salt("characters").max_height(list_height).auto_shrink([false, true]).show(ui, |ui| {
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
                                ui.weak("Intro and others");
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
                        ui.separator();
                        ui.label("Export to Blender (.glb):");
                        ui.horizontal(|ui| {
                            if ui.button("Costume").on_hover_text("Writes this costume to exports/").clicked() {
                                self.export_request = Some(ExportRequest::Costume);
                            }
                            if ui.button("All costumes").on_hover_text("Writes each costume of this character to its own file").clicked() {
                                self.export_request = Some(ExportRequest::Character);
                            }
                        });
                        ui.checkbox(&mut self.export_all_motions, "All motions")
                            .on_hover_text("Otherwise only the playing motion. Files get large, and Blender takes long to import them.");
                        if entry.hairstyles.len() > 1 {
                            ui.separator();
                            ui.label("Hair:");
                            ui.horizontal_wrapped(|ui| {
                                let mut pick = None;
                                if ui.selectable_label(self.selected_hair_idx.is_none(), "Default").on_hover_text("The head the costume comes with").clicked() {
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
                        ui.separator();
                        ui.label("Costume:");
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
                        egui::ScrollArea::vertical().id_salt("costumes").auto_shrink([false, true]).show(ui, |ui| {
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
            });
    }
}
