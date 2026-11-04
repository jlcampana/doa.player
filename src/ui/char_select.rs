use crate::app::{CharacterEntry, GameSource};

pub struct CharSelectPanel {
    pub visible: bool,
    pub selected_source: GameSource,
    pub selected_char_idx: Option<usize>,
    pub selected_costume_idx: usize,
    filter: String,
}

impl CharSelectPanel {
    pub fn new() -> Self {
        Self {
            visible: true,
            selected_source: GameSource::Doa2U,
            selected_char_idx: None,
            selected_costume_idx: 0,
            filter: String::new(),
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
                ui.heading("DOA Player");
                ui.separator();

                // Game source toggle
                ui.horizontal(|ui| {
                    ui.label("Game:");
                    if ui.selectable_label(
                        self.selected_source == GameSource::Doa2U, "DOA2U"
                    ).clicked() {
                        self.selected_source = GameSource::Doa2U;
                        self.selected_char_idx = None;
                        self.selected_costume_idx = 0;
                    }
                    if ui.selectable_label(
                        self.selected_source == GameSource::Doa3, "DOA3"
                    ).clicked() {
                        self.selected_source = GameSource::Doa3;
                        self.selected_char_idx = None;
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
                    for (i, entry) in characters.iter().enumerate() {
                        if !filter_lower.is_empty()
                            && !entry.display_name.to_lowercase().contains(&filter_lower)
                        {
                            continue;
                        }
                        let selected = self.selected_char_idx == Some(i);
                        if ui.selectable_label(selected, &entry.display_name).clicked() {
                            if !selected {
                                self.selected_char_idx = Some(i);
                                self.selected_costume_idx = 0;
                                on_char_selected(i, crate::app::DEFAULT_COSTUME);
                            }
                        }
                    }
                });

                // Costume selector
                if let Some(char_idx) = self.selected_char_idx {
                    if let Some(entry) = characters.get(char_idx) {
                        ui.separator();
                        ui.label("Costume:");
                        egui::ScrollArea::vertical().id_salt("costumes").auto_shrink([false, true]).show(ui, |ui| {
                            for (ci, name) in entry.costumes.iter().enumerate() {
                                let selected = self.selected_costume_idx == ci;
                                if ui.selectable_label(selected, name).clicked() {
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
