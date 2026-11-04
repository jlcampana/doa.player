use crate::app::AnimationEntry;

pub struct AnimPanel {
    pub visible: bool,
    pub selected_anim_idx: Option<usize>,
    pub playing: bool,
    pub loop_anim: bool,
    pub speed: f32,
    pub reset_requested: bool,
    pub wind: bool,
    /// Multiplies the wind's force; 1 is what the game's stages use.
    pub wind_scale: f32,
    filter: String,
}

impl AnimPanel {
    pub fn new() -> Self {
        Self {
            visible: true,
            selected_anim_idx: None,
            playing: false,
            loop_anim: true,
            speed: 1.0,
            reset_requested: false,
            wind: false,
            wind_scale: 1.0,
            filter: String::new(),
        }
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        animations: &[AnimationEntry],
        on_anim_selected: &mut dyn FnMut(usize),
    ) {
        if !self.visible {
            return;
        }

        egui::Window::new("Animations")
            .default_pos([210.0, 10.0])
            .default_size([220.0, 400.0])
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Filter:");
                    egui::TextEdit::singleline(&mut self.filter)
                        .desired_width(120.0)
                        .show(ui);
                });

                ui.horizontal(|ui| {
                    if ui.button(if self.playing { "⏸ Pause" } else { "▶ Play" }).clicked() {
                        self.playing = !self.playing;
                    }
                    if ui.button("⏮ Reset").clicked() {
                        self.reset_requested = true;
                    }
                    ui.checkbox(&mut self.loop_anim, "Loop");
                    ui.checkbox(&mut self.wind, "Wind");
                });

                ui.horizontal(|ui| {
                    ui.label("Speed:");
                    ui.add(egui::Slider::new(&mut self.speed, 0.1..=3.0).step_by(0.1));
                });

                if self.wind {
                    ui.add(egui::Slider::new(&mut self.wind_scale, 0.0..=5.0).text("Wind force"));
                }
                ui.separator();

                if animations.is_empty() {
                    ui.label(egui::RichText::new("No animations loaded").color(egui::Color32::GRAY));
                    return;
                }

                let filter_lower = self.filter.to_lowercase();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (i, anim) in animations.iter().enumerate() {
                        if !filter_lower.is_empty()
                            && !anim.name.to_lowercase().contains(&filter_lower)
                        {
                            continue;
                        }
                        let selected = self.selected_anim_idx == Some(i);
                        if ui.selectable_label(selected, &anim.name).clicked() {
                            self.selected_anim_idx = Some(i);
                            self.playing = true;
                            on_anim_selected(i);
                        }
                    }
                });
            });
    }
}
