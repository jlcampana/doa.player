use crate::app::AnimationEntry;

pub struct AnimPanel {
    pub selected_anim_idx: Option<usize>,
    /// The viewer's reference pose is shown (the list's first row, not a game motion).
    pub reference: bool,
    pub playing: bool,
    pub loop_anim: bool,
    pub speed: f32,
    pub reset_requested: bool,
    pub wind: bool,
    /// DOAX3: the viewer's stand-in physics for hair and accessories.
    pub physics: bool,
    /// Multiplies the wind's force; 1 is what the game's stages use.
    pub wind_scale: f32,
    filter: String,
}

impl AnimPanel {
    pub fn new() -> Self {
        Self {
            selected_anim_idx: None,
            reference: false,
            playing: false,
            loop_anim: true,
            speed: 1.0,
            reset_requested: false,
            wind: false,
            physics: true,
            wind_scale: 1.0,
            filter: String::new(),
        }
    }

    /// Play controls and the motion list, drawn into `ui`. The list starts with the viewer's
    /// reference pose, which `on_anim_selected` gets as `None`.
    pub fn ui(&mut self, ui: &mut egui::Ui, animations: &[AnimationEntry], on_anim_selected: &mut dyn FnMut(Option<usize>)) {
        ui.horizontal(|ui| {
            if ui.button(if self.playing { "⏸ Pausa" } else { "▶ Reproducir" }).clicked() {
                self.playing = !self.playing;
            }
            if ui.button("⏮ Inicio").clicked() {
                self.reset_requested = true;
            }
            ui.checkbox(&mut self.loop_anim, "Bucle");
        });
        ui.add(egui::Slider::new(&mut self.speed, 0.1..=3.0).step_by(0.1).text("Velocidad"));
        if animations.is_empty() {
            ui.label(egui::RichText::new("Sin animaciones").color(egui::Color32::GRAY));
            return;
        }
        egui::TextEdit::singleline(&mut self.filter).hint_text("Filtrar…").show(ui);
        let filter_lower = self.filter.to_lowercase();
        // The last list of its panel: it takes the height left.
        egui::ScrollArea::vertical().id_salt("animations").max_height((ui.available_height() - 6.0).max(120.0)).auto_shrink([false, true]).show(ui, |ui| {
            let name = "Pose de referencia";
            if filter_lower.is_empty() || name.to_lowercase().contains(&filter_lower) {
                if ui.selectable_label(self.reference, name).on_hover_text("Del visor, no del juego: el modelo como está hecho, de pie en el suelo").clicked() {
                    self.reference = true;
                    self.selected_anim_idx = None;
                    on_anim_selected(None);
                }
            }
            for (i, anim) in animations.iter().enumerate() {
                if !filter_lower.is_empty() && !anim.name.to_lowercase().contains(&filter_lower) {
                    continue;
                }
                let selected = self.selected_anim_idx == Some(i);
                if ui.selectable_label(selected, &anim.name).clicked() {
                    self.selected_anim_idx = Some(i);
                    self.playing = true;
                    on_anim_selected(Some(i));
                }
            }
        });
    }

    /// Wind and the stand-in physics, drawn into `ui`.
    pub fn physics_ui(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.wind, "Viento");
        if self.wind {
            ui.add(egui::Slider::new(&mut self.wind_scale, 0.0..=5.0).text("Fuerza del viento"));
        }
        ui.checkbox(&mut self.physics, "Física de DOAX3").on_hover_text("DOAX3: la física sustituta del visor para pelo y accesorios, no la del juego");
    }
}
