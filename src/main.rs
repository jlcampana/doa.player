mod app;
mod export;
mod formats;
mod renderer;
mod ui;
mod voices;

use app::App;
use renderer::RenderContext;
use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};
/// Pixels on the left that panels cover by default.
const LEFT_PANELS_WIDTH: f32 = 440.0;


struct Handler {
    window: Option<Arc<Window>>,
    app: Option<App>,
    last_frame: std::time::Instant,
    mouse_left_down: bool,
    mouse_right_down: bool,
    last_pos: (f64, f64),
}

impl Handler {
    fn new() -> Self {
        Self {
            window: None,
            app: None,
            last_frame: std::time::Instant::now(),
            mouse_left_down: false,
            mouse_right_down: false,
            last_pos: (0.0, 0.0),
        }
    }
}

impl ApplicationHandler for Handler {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.app.is_some() {
            return;
        }

        // The name the desktop matches with `doa-player.desktop`, on each of Wayland and X11.
        let attributes = winit::platform::wayland::WindowAttributesExtWayland::with_name(Window::default_attributes(), "doa-player", "doa-player");
        let attributes = winit::platform::x11::WindowAttributesExtX11::with_name(attributes, "doa-player", "doa-player");
        let window = Arc::new(
            event_loop
                .create_window(
                    attributes
                        // `shot.sh` gives its windows a title of their own, so as not to capture
                        // another viewer that is open.
                        .with_title(std::env::var("DOA_PLAYER_TITLE").unwrap_or_else(|_| "DOA Player".to_string()))
                        // Exporting from the command line needs the GPU, not a window on the desktop.
                        .with_visible(!std::env::args().any(|a| a == "--export" || a == "--export-all" || a == "--install-icon"))
                        .with_inner_size(winit::dpi::LogicalSize::new(1280u32, 720u32)),
                )
                .expect("window creation failed"),
        );

        let ctx = pollster::block_on(RenderContext::new(window.clone()))
            .expect("render context creation failed");

        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &*window,
            Some(window.scale_factor() as f32),
            None,
            Some(2048),
        );

        let mut app = App::new(ctx, egui_ctx, egui_state, &window);
        load_from_arguments(&mut app);
        self.window = Some(window);
        self.app = Some(app);
        self.last_frame = std::time::Instant::now();

        log::info!("DOA Player initialized");
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(app) = &mut self.app else { return };
        let Some(window) = &self.window else { return };

        let egui_consumed = app.egui_state.on_window_event(window, &event).consumed;

        match &event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
                return;
            }
            WindowEvent::Resized(size) if !egui_consumed => {
                app.resize(size.width, size.height);
            }
            WindowEvent::MouseInput { button, state, .. } if !egui_consumed => {
                match (button, state) {
                    (MouseButton::Left, ElementState::Pressed) => {
                        self.mouse_left_down = true;
                        // Later drags turn around the point of the character under the cursor.
                        let size = window.inner_size();
                        let (width, height) = (size.width as f32, size.height as f32);
                        let (origin, direction) = app.camera.ray(self.last_pos.0 as f32, self.last_pos.1 as f32, width, height);
                        app.camera.pivot = app.mesh.pick(origin, direction, app.camera.pixel_spread(6.0, height));
                    }
                    (MouseButton::Left, ElementState::Released) => self.mouse_left_down = false,
                    (MouseButton::Right, ElementState::Pressed) => self.mouse_right_down = true,
                    (MouseButton::Right, ElementState::Released) => self.mouse_right_down = false,
                    _ => {}
                }
            }
            WindowEvent::CursorMoved { position, .. } if !egui_consumed => {
                let dx = (position.x - self.last_pos.0) as f32;
                let dy = (position.y - self.last_pos.1) as f32;
                if self.mouse_left_down {
                    app.camera.orbit(glam::Vec2::new(dx, dy));
                }
                if self.mouse_right_down {
                    app.camera.pan(glam::Vec2::new(dx, dy));
                }
                self.last_pos = (position.x, position.y);
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.last_pos = (position.x, position.y);
            }
            WindowEvent::MouseWheel { delta, .. } if !egui_consumed => {
                let scroll = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 * 0.01,
                };
                app.camera.zoom(scroll);
            }
            WindowEvent::RedrawRequested => {
                let now = std::time::Instant::now();
                let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.05);
                self.last_frame = now;
                app.tick(dt);
                render_frame(app, window);
                window.request_redraw();
            }
            _ => {}
        }
    }

    /// `run_app` drops the event loop and its Wayland connection when it returns. The clipboard
    /// inside `App` borrows that connection, so it has to go first or its thread crashes on exit.
    fn exiting(&mut self, _: &ActiveEventLoop) {
        self.app = None;
        self.window = None;
    }

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}

// ─── UI collection struct ─────────────────────────────────────────────────────

#[derive(Default)]
struct UiActions {
    load_char: Option<(usize, usize)>,
    selected_anim: Option<usize>,
    play_voice: Option<usize>,
}

fn render_frame(app: &mut App, window: &Window) {
    let raw_input = app.egui_state.take_egui_input(window);

    // Build UI and collect actions — we pass cloned data to avoid borrow conflicts
    let mut actions = UiActions::default();

    let egui_ctx = app.egui_ctx.clone();
    let characters: &[app::CharacterEntry] = match app.char_panel.selected_source {
        app::GameSource::Doa2U => &app.doa2u_characters,
        app::GameSource::Doa3  => &app.doa3_characters,
        app::GameSource::Doax  => &app.doax_characters,
        app::GameSource::Doax2 => &app.doax2_characters,
        app::GameSource::Doa4 => &app.doa4_characters,
    };
    // DOA2U's clips have names. DOA3's do not, and which is whose is unknown there.
    let voice_clips = if app.char_panel.selected_source == app::GameSource::Doa2U {
        let selected = app.char_panel.selected_char_idx.and_then(|i| app.doa2u_characters.get(i)).map(|c| c.display_name.clone());
        let character = if app.voice_panel.show_others { None } else { selected };
        if app.voice_panel.show_others || character.is_some() {
            app.voices.clips(app::doa2u_voice_path(), character.as_deref())
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };
    let light_names: Vec<String> = app.stage_light_list().iter().map(|(name, _)| name.clone()).collect();
    let has_bounce = app.has_chest_spring();
    let is_lit = app.mesh.is_lit();
    let bounce_before = app.bg_panel.bounce_scale;
    let egui_output = egui_ctx.run(raw_input, |ctx| {
        build_ui(
            ctx,
            characters,
            &app.current_animations,
            &app.background_options,
            &mut app.char_panel,
            &mut app.anim_panel,
            &mut app.bg_panel,
            has_bounce,
            is_lit,
            &mut app.voice_panel,
            &voice_clips,
            &light_names,
            &app.status_msg,
            &mut actions,
        );
    });

    // Apply actions after UI is built
    if let Some(entry) = actions.play_voice {
        if let Err(e) = app.voices.play(app::doa2u_voice_path(), entry) {
            app.status_msg = format!("Could not play the clip: {e:#}");
        }
    }
    if let Some((ci, costume)) = actions.load_char {
        app.load_character(ci, costume);
    }
    // Keeps the slider live while the animation is paused.
    if app.bg_panel.bounce_scale != bounce_before {
        app.update_doa3_pose();
    }
    if std::mem::take(&mut app.anim_panel.reset_requested) {
        app.reset_animation();
    }
    if let Some(index) = actions.selected_anim {
        app.select_motion(index);
    }

    // GPU rendering
    let surface_texture = match app.ctx.surface.get_current_texture() {
        Ok(t) => t,
        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
            let w = app.ctx.surface_config.width;
            let h = app.ctx.surface_config.height;
            app.ctx.resize(w, h);
            return;
        }
        Err(e) => {
            log::error!("Surface error: {}", e);
            return;
        }
    };

    let view = surface_texture
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());

    let vp = app.camera.view_proj();
    app.grid.update(&app.ctx.queue, &vp);
    // The character list and the animations window on the left. The inset stays fixed when
    // they are moved or collapsed.
    let size = window.inner_size();
    app.camera.set_left_inset(LEFT_PANELS_WIDTH, size.width as f32, size.height as f32);
    if let Some(request) = app.char_panel.export_request.take() {
        let all_motions = app.char_panel.export_all_motions;
        app.status_msg = match request {
            ui::char_select::ExportRequest::Costume => match app.export_current(all_motions) {
                Ok(path) => format!("Exported {}", path.display()),
                Err(error) => format!("Export failed: {error:#}"),
            },
            ui::char_select::ExportRequest::Character => match app.char_panel.selected_char_idx {
                Some(index) => {
                    app.start_character_export(index, all_motions);
                    "Exporting...".to_string()
                }
                None => "No character selected".to_string(),
            },
        };
    }
    // One costume per frame, so that the window keeps responding.
    app.step_character_export();
    if let Some(part) = app.bg_panel.frame_request.take() {
        if let Some((low, high)) = app.mesh.frame_box(part) {
            app.camera.frame_box(low, high);
        }
    }
    if app.mesh.is_lit() {
        let lights = app.current_stage_lights();
        app.mesh.set_lights(Some(lights));
    }
    app.mesh.plain_lighting = !app.bg_panel.game_lights;
    app.mesh.update(&app.ctx.queue, &vp, &app.camera.view_matrix(), app.bg_panel.anime_shading, app.bg_panel.relief, app.bg_panel.no_textures.then_some(app.bg_panel.plain_color));
    app.mesh.mesh_only = app.bg_panel.mesh_only;
    app.mesh.outline_width = app.bg_panel.outline_width;
    app.wireframe.update_camera(&app.ctx.queue, &vp);

    let mut encoder = app.ctx.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Main Encoder"),
    });

    // 3D pass
    let clear = app.clear_color();
    {
        let depth_view = &app.ctx.depth_view;
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("3D Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(clear), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        app.grid.draw(&mut rpass);
        if app.bg_panel.anime_shading {
            app.mesh.draw_outline(&mut rpass);
        }
        app.mesh.draw(&mut rpass);
        app.wireframe.draw(&mut rpass);
    }

    // egui pass
    let screen_descriptor = egui_wgpu::ScreenDescriptor {
        size_in_pixels: [app.ctx.surface_config.width, app.ctx.surface_config.height],
        pixels_per_point: window.scale_factor() as f32,
    };

    for (id, delta) in &egui_output.textures_delta.set {
        app.egui_renderer.update_texture(&app.ctx.device, &app.ctx.queue, *id, delta);
    }

    let tris = app.egui_ctx.tessellate(egui_output.shapes, egui_output.pixels_per_point);
    app.egui_renderer.update_buffers(
        &app.ctx.device, &app.ctx.queue, &mut encoder, &tris, &screen_descriptor,
    );

    {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("egui Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        // egui-wgpu 0.29 requires RenderPass<'static>
        let mut rpass = rpass.forget_lifetime();
        app.egui_renderer.render(&mut rpass, &tris, &screen_descriptor);
    }

    for id in &egui_output.textures_delta.free {
        app.egui_renderer.free_texture(id);
    }

    app.egui_state.handle_platform_output(window, egui_output.platform_output);

    app.ctx.queue.submit(std::iter::once(encoder.finish()));
    surface_texture.present();
}

#[allow(clippy::too_many_arguments)]
fn build_ui(
    ctx: &egui::Context,
    characters: &[app::CharacterEntry],
    animations: &[app::AnimationEntry],
    backgrounds: &[app::BackgroundOption],
    char_panel: &mut ui::CharSelectPanel,
    anim_panel: &mut ui::AnimPanel,
    bg_panel: &mut ui::BgSelectPanel,
    has_bounce: bool,
    is_lit: bool,
    voice_panel: &mut ui::VoicePanel,
    voice_clips: &[voices::Clip],
    light_names: &[String],
    status_msg: &str,
    actions: &mut UiActions,
) {
    char_panel.show(ctx, characters, &mut |ci, costume| {
        actions.load_char = Some((ci, costume));
    });

    anim_panel.show(ctx, animations, &mut |ai| {
        actions.selected_anim = Some(ai);
    });

    bg_panel.show(ctx, backgrounds, has_bounce, is_lit, light_names);
    voice_panel.show(ctx, voice_clips, &mut |entry| actions.play_voice = Some(entry));

    egui::TopBottomPanel::bottom("status")
        .frame(egui::Frame::none().fill(egui::Color32::from_rgb(30, 30, 30)))
        .show(ctx, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(status_msg)
                        .color(egui::Color32::LIGHT_GRAY)
                        .size(12.0),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new("LMB: Orbit  |  RMB: Pan  |  Scroll: Zoom")
                            .color(egui::Color32::GRAY)
                            .size(11.0),
                    );
                });
            });
            ui.add_space(2.0);
        });
}

/// `doa_player --load "Jann Lee" 0 [motion] [yaw degrees] [distance] [target height]`
/// opens with a DOA3 character loaded, optionally paused with the camera placed.
/// `--doa2u` looks the name up among DOA2U characters instead.
/// `--doax` does so among DOAX's.
/// `--install-icon` writes the icon and a desktop entry to the user's folders, and exits.
/// `--export-all` writes every costume of all games, without `--load`, and exits. With `--doax`,
/// only that game's.
/// `--export` writes that costume to `exports/` and exits, with the chosen motion or, with
/// `--all-motions`, all of them.
/// Also takes `--anime`, `--no-textures`, `--mesh-only`, `--wind`, `--bounce <scale>`, `--motion-id <id>` and
/// `--frame <frame>`, which holds the motion on that frame.
fn load_from_arguments(app: &mut App) {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--install-icon") {
        match app.install_desktop_entry() {
            Ok(path) => println!("Wrote {}", path.display()),
            Err(error) => println!("Could not install the icon: {error:#}"),
        }
        std::process::exit(0);
    }
    if args.iter().any(|a| a == "--export-all") {
        let all_motions = args.iter().any(|a| a == "--all-motions");
        let mut written = 0;
        // With `--doax` or `--doax2`, only that game, whose suits take far longer than the
        // other two games. DOAX2 is left out of the default run.
        let only_doax = args.iter().any(|a| a == "--doax");
        let only_doax2 = args.iter().any(|a| a == "--doax2");
        for source in [app::GameSource::Doa2U, app::GameSource::Doa3, app::GameSource::Doax, app::GameSource::Doax2] {
            if (only_doax && source != app::GameSource::Doax) || (only_doax2 && source != app::GameSource::Doax2) {
                continue;
            }
            if source == app::GameSource::Doax2 && !only_doax2 {
                continue;
            }
            app.char_panel.selected_source = source;
            let count = match source {
                app::GameSource::Doa2U => app.doa2u_characters.len(),
                app::GameSource::Doa3 => app.doa3_characters.len(),
                app::GameSource::Doax => app.doax_characters.len(),
                app::GameSource::Doax2 => app.doax2_characters.len(),
                app::GameSource::Doa4 => app.doa4_characters.len(),
            };
            for index in 0..count {
                written += app.export_character(index, all_motions);
            }
        }
        println!("Exported {written} costumes to exports/");
        std::process::exit(0);
    }
    let [flag, name, rest @ ..] = args.as_slice() else { return };
    if flag != "--load" {
        return;
    }
    let number = |i: usize| rest.get(i).and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
    let doa2u = args.iter().any(|a| a == "--doa2u");
    let doax = args.iter().any(|a| a == "--doax");
    let doax2 = args.iter().any(|a| a == "--doax2");
    let doa4 = args.iter().any(|a| a == "--doa4");
    let characters = if doa4 { &app.doa4_characters } else if doax2 { &app.doax2_characters } else if doax { &app.doax_characters } else if doa2u { &app.doa2u_characters } else { &app.doa3_characters };
    let Some(index) = characters.iter().position(|c| c.display_name.eq_ignore_ascii_case(name)) else {
        log::warn!("Unknown character: {}", name);
        // Without a costume an export has nothing to write, and its hidden window would wait.
        if args.iter().any(|a| a == "--export") {
            eprintln!("Export failed: unknown character {name}");
            std::process::exit(1);
        }
        return;
    };
    app.char_panel.selected_source = if doa4 { app::GameSource::Doa4 } else if doax2 { app::GameSource::Doax2 } else if doax { app::GameSource::Doax } else if doa2u { app::GameSource::Doa2U } else { app::GameSource::Doa3 };
    app.char_panel.selected_char_idx = Some(index);
    app.char_panel.selected_costume_idx = number(0);
    if let Some(scale) = args.iter().position(|a| a == "--bounce").and_then(|i| args.get(i + 1)?.parse().ok()) {
        app.bg_panel.bounce_scale = scale;
    }
    app.load_character(index, number(0));
    app.select_motion(number(1));
    let value_of = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)?.parse::<usize>().ok());
    if let Some(id) = value_of("--motion-id") {
        if let Some(index) = app.current_animations.iter().position(|a| a.doa3_motion_id == id) {
            app.select_motion(index);
        }
    }

    app.bg_panel.anime_shading = args.iter().any(|a| a == "--anime");
    app.bg_panel.game_lights = !args.iter().any(|a| a == "--no-game-lights");
    app.bg_panel.no_textures = args.iter().any(|a| a == "--no-textures");
    app.bg_panel.mesh_only = args.iter().any(|a| a == "--mesh-only");
    // Arguments set the camera themselves, unless they ask for a part.
    app.bg_panel.frame_request = None;
    if let Some(part) = args.iter().position(|a| a == "--view").and_then(|i| args.get(i + 1)) {
        use renderer::mesh::FramePart;
        app.bg_panel.frame_request = match part.as_str() {
            "full" => Some(FramePart::Full),
            "head" => Some(FramePart::Head),
            "hands" => Some(FramePart::Hands),
            "feet" => Some(FramePart::Feet),
            _ => None,
        };
    }
    if let Some(relief) = value_of("--relief") {
        app.bg_panel.relief = relief as f32 / 100.0;
    }
    if let Some(lights) = value_of("--lights") {
        app.bg_panel.lights = lights.min(app.stage_light_list().len().saturating_sub(1));
    }
    if args.iter().any(|a| a == "--export") {
        let all_motions = args.iter().any(|a| a == "--all-motions");
        match app.export_current(all_motions) {
            Ok(path) => println!("Exported {}", path.display()),
            Err(error) => eprintln!("Export failed: {error:#}"),
        }
        std::process::exit(0);
    }
    app.anim_panel.wind = args.iter().any(|a| a == "--wind");
    let float = |i: usize| rest.get(i).and_then(|v| v.parse::<f32>().ok());
    if let Some(yaw) = float(2) {
        app.camera.yaw = yaw.to_radians();
        app.camera.pitch = 0.05;
        app.anim_panel.playing = false;
    }
    if let Some(distance) = float(3) {
        app.camera.distance = distance;
    }
    if let Some(height) = float(4) {
        app.camera.target.y = height;
    }
    if let Some(frame) = value_of("--frame") {
        app.anim_panel.playing = false;
        app.anim_time = frame as f32;
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("Starting DOA Player");

    let event_loop = EventLoop::new().expect("event loop creation failed");
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    let mut handler = Handler::new();
    event_loop.run_app(&mut handler).expect("event loop failed");
}
