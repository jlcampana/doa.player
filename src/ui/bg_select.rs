use crate::app::BackgroundOption;

pub struct BgSelectPanel {
    pub selected: usize,
    pub anime_shading: bool,
    /// Multiplies the chest spring's deformation; 1 is what the game does.
    pub bounce_scale: f32,
    /// Index in `DOA2U_LIGHTS`.
    pub lights: usize,
    /// The optional bump from textures. 0 is what the game draws.
    pub relief: f32,
}

impl BgSelectPanel {
    pub fn new() -> Self {
        Self { selected: 0, anime_shading: false, bounce_scale: 1.0, lights: 0, relief: 0.0 }
    }

    pub fn show(&mut self, ctx: &egui::Context, options: &[BackgroundOption], has_bounce: bool, lit: bool) {
        egui::Window::new("Background")
            .default_pos([440.0, 10.0])
            .default_size([160.0, 220.0])
            .resizable(false)
            .show(ctx, |ui| {
                for (i, opt) in options.iter().enumerate() {
                    let sel = self.selected == i;
                    if ui.selectable_label(sel, &opt.name).clicked() {
                        self.selected = i;
                    }
                }
                ui.separator();
                ui.checkbox(&mut self.anime_shading, "Anime shading");
                if lit {
                    ui.add(egui::Slider::new(&mut self.relief, 0.0..=1.0).text("Relief (not in game)"));
                    egui::CollapsingHeader::new("Stage lights").show(ui, |ui| {
                        for (i, (name, _)) in crate::renderer::mesh::DOA2U_LIGHTS.iter().enumerate() {
                            if ui.selectable_label(self.lights == i, *name).clicked() {
                                self.lights = i;
                            }
                        }
                    });
                }
                if has_bounce {
                    ui.add(egui::Slider::new(&mut self.bounce_scale, 0.0..=3.0).text("Bounce"));
                }
            });
    }

    pub fn current_option<'a>(&self, options: &'a [BackgroundOption]) -> Option<&'a BackgroundOption> {
        options.get(self.selected)
    }
}
