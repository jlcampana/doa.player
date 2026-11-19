use crate::voices::Clip;

/// Lists the voice clips of the selected character, and the rest under "Others".
pub struct VoicePanel {
    pub filter: String,
    pub show_others: bool,
}

impl VoicePanel {
    pub fn new() -> Self {
        Self { filter: String::new(), show_others: false }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, clips: &[Clip], on_play: &mut dyn FnMut(usize)) {
        egui::TextEdit::singleline(&mut self.filter).hint_text("Filtrar…").show(ui);
        ui.checkbox(&mut self.show_others, "Locutores y otros");
        if clips.is_empty() {
            ui.label("Sin voces para este personaje.");
        }
        let filter = self.filter.to_lowercase();
        egui::ScrollArea::vertical().id_salt("voices").max_height(200.0).show(ui, |ui| {
            for clip in clips.iter().filter(|c| c.name.to_lowercase().contains(&filter)) {
                if ui.small_button(&clip.name).clicked() {
                    on_play(clip.entry);
                }
            }
        });
    }
}
