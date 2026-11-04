mod app;
mod formats;
mod renderer;
mod ui;

use app::App;
use renderer::RenderContext;
use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

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

        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("DOA Player")
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
                    (MouseButton::Left, ElementState::Pressed) => self.mouse_left_down = true,
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
}

fn render_frame(app: &mut App, window: &Window) {
    let raw_input = app.egui_state.take_egui_input(window);

    // Build UI and collect actions — we pass cloned data to avoid borrow conflicts
    let mut actions = UiActions::default();

    let egui_ctx = app.egui_ctx.clone();
    let characters: &[app::CharacterEntry] = match app.char_panel.selected_source {
        app::GameSource::Doa2U => &app.doa2u_characters,
        app::GameSource::Doa3  => &app.doa3_characters,
    };
    let has_bounce = app.has_chest_spring();
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
            &app.status_msg,
            &mut actions,
        );
    });

    // Apply actions after UI is built
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
    app.mesh.update(&app.ctx.queue, &vp, &app.camera.view_matrix(), app.bg_panel.anime_shading);
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
    status_msg: &str,
    actions: &mut UiActions,
) {
    char_panel.show(ctx, characters, &mut |ci, costume| {
        actions.load_char = Some((ci, costume));
    });

    anim_panel.show(ctx, animations, &mut |ai| {
        actions.selected_anim = Some(ai);
    });

    bg_panel.show(ctx, backgrounds, has_bounce);

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
/// Also takes `--anime`, `--wind`, `--bounce <scale>`, `--motion-id <id>` and
/// `--frame <frame>`, which holds the motion on that frame.
fn load_from_arguments(app: &mut App) {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [flag, name, rest @ ..] = args.as_slice() else { return };
    if flag != "--load" {
        return;
    }
    let number = |i: usize| rest.get(i).and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
    let doa2u = args.iter().any(|a| a == "--doa2u");
    let characters = if doa2u { &app.doa2u_characters } else { &app.doa3_characters };
    let Some(index) = characters.iter().position(|c| c.display_name.eq_ignore_ascii_case(name)) else {
        log::warn!("Unknown character: {}", name);
        return;
    };
    app.char_panel.selected_source = if doa2u { app::GameSource::Doa2U } else { app::GameSource::Doa3 };
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
