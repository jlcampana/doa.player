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

    pub fn show(&mut self, ctx: &egui::Context, clips: &[Clip], on_play: &mut dyn FnMut(usize)) {
        egui::Window::new("Voices")
            .default_pos([210.0, 470.0])
            .default_size([220.0, 240.0])
            .default_open(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Filter:");
                    egui::TextEdit::singleline(&mut self.filter).desired_width(110.0).show(ui);
                });
                ui.checkbox(&mut self.show_others, "Announcers and others");
                if clips.is_empty() {
                    ui.label("No clips for this character.");
                }
                let filter = self.filter.to_lowercase();
                egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                    for clip in clips.iter().filter(|c| c.name.to_lowercase().contains(&filter)) {
                        if ui.small_button(&clip.name).clicked() {
                            on_play(clip.entry);
                        }
                    }
                });
            });
    }
}
