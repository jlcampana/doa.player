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
    /// Width of the anime shading's ink line, 1 as drawn by default.
    pub outline_width: f32,
    /// Lights the model with the game's lights and materials. Off, it gets the viewer's plain
    /// shading, as before the game's lighting was read.
    pub game_lights: bool,
    /// A part of the character to frame, asked for this frame.
    pub frame_request: Option<crate::renderer::mesh::FramePart>,
}

impl BgSelectPanel {
    pub fn new() -> Self {
        Self { selected: 0, outline_width: 1.0, anime_shading: false, no_textures: false, plain_color: [128, 128, 132], mesh_only: false, bounce_scale: 1.0, lights: 0, relief: 0.0, game_lights: true, frame_request: None }
    }

    /// How the model is drawn and the background, drawn into `ui`.
    pub fn view_ui(&mut self, ui: &mut egui::Ui, options: &[BackgroundOption]) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.mesh_only, false, "Texturizado");
            ui.selectable_value(&mut self.mesh_only, true, "Alambre");
        });
        if !self.mesh_only {
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.no_textures, "Sin texturas");
                ui.color_edit_button_srgb(&mut self.plain_color);
            });
            ui.checkbox(&mut self.anime_shading, "Sombreado anime");
            if self.anime_shading {
                ui.indent("outline", |ui| {
                    ui.add(egui::Slider::new(&mut self.outline_width, 0.0..=4.0).text("Contorno"));
                });
            }
        }
        egui::ComboBox::from_label("Fondo").selected_text(options.get(self.selected).map_or("", |o| o.name.as_str())).show_ui(ui, |ui| {
            for (i, opt) in options.iter().enumerate() {
                ui.selectable_value(&mut self.selected, i, &opt.name);
            }
        });
    }

    /// The game's lights, drawn into `ui`. `lit` says whether the model has the game's
    /// materials.
    pub fn lighting_ui(&mut self, ui: &mut egui::Ui, lit: bool, light_names: &[String]) {
        if !lit {
            ui.label(egui::RichText::new("Este modelo usa el sombreado del visor").color(egui::Color32::GRAY));
            return;
        }
        ui.checkbox(&mut self.game_lights, "Luces del juego");
        ui.add(egui::Slider::new(&mut self.relief, 0.0..=1.0).text("Relieve (no está en el juego)"));
        ui.label("Escenario:");
        egui::ScrollArea::vertical().id_salt("stage lights").max_height(180.0).show(ui, |ui| {
            for (i, name) in light_names.iter().enumerate() {
                if ui.selectable_label(self.lights == i, name).clicked() {
                    self.lights = i;
                }
            }
        });
    }

    /// The camera's shots, drawn into `ui`.
    pub fn camera_ui(&mut self, ui: &mut egui::Ui) {
        use crate::renderer::mesh::FramePart;
        ui.horizontal_wrapped(|ui| {
            for (name, part) in [("Entero", FramePart::Full), ("Cabeza", FramePart::Head), ("Manos", FramePart::Hands), ("Pies", FramePart::Feet)] {
                if ui.button(name).clicked() {
                    self.frame_request = Some(part);
                }
            }
        });
    }

    pub fn current_option<'a>(&self, options: &'a [BackgroundOption]) -> Option<&'a BackgroundOption> {
        options.get(self.selected)
    }
}
