use crate::app::BackgroundOption;

pub struct BgSelectPanel {
    pub selected: usize,
    pub anime_shading: bool,
    /// Draws the model grey, to see its shape.
    pub no_textures: bool,
    /// The model's color with no textures, in sRGB.
    pub plain_color: [u8; 3],
    /// Draws only the triangles' edges, to see how it is built.
    pub mesh_only: bool,
    /// Multiplies the chest spring's deformation; 1 is what the game does.
    pub bounce_scale: f32,
    /// Index in the app's list of stage lights.
    pub lights: usize,
    /// The optional bump from textures. 0 is what the game draws.
    pub relief: f32,
    /// A part of the character to frame, asked for this frame.
    pub frame_request: Option<crate::renderer::mesh::FramePart>,
}

impl BgSelectPanel {
    pub fn new() -> Self {
        Self { selected: 0, anime_shading: false, no_textures: false, plain_color: [128, 128, 132], mesh_only: false, bounce_scale: 1.0, lights: 0, relief: 0.0, frame_request: None }
    }

    pub fn show(&mut self, ctx: &egui::Context, options: &[BackgroundOption], has_bounce: bool, lit: bool, light_names: &[String]) {
        egui::Window::new("Background")
            .anchor(egui::Align2::RIGHT_TOP, [-10.0, 10.0])
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
                ui.horizontal(|ui| {
                    use crate::renderer::mesh::FramePart;
                    ui.label("View:");
                    for (name, part) in [("Full", FramePart::Full), ("Head", FramePart::Head), ("Hands", FramePart::Hands), ("Feet", FramePart::Feet)] {
                        if ui.small_button(name).clicked() {
                            self.frame_request = Some(part);
                        }
                    }
                });
                ui.separator();
                ui.checkbox(&mut self.anime_shading, "Anime shading");
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.no_textures, "No textures");
                    ui.color_edit_button_srgb(&mut self.plain_color);
                });
                ui.checkbox(&mut self.mesh_only, "Mesh only");
                if lit {
                    ui.add(egui::Slider::new(&mut self.relief, 0.0..=1.0).text("Relief (not in game)"));
                    egui::CollapsingHeader::new("Stage lights").show(ui, |ui| {
                        egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                            for (i, name) in light_names.iter().enumerate() {
                                if ui.selectable_label(self.lights == i, name).clicked() {
                                    self.lights = i;
                                }
                            }
                        });
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
