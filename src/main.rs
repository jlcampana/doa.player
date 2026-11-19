mod app;
mod arguments;
mod export;
mod formats;
mod renderer;
mod shell;
mod ui;
mod voices;

use renderer::{RenderContext, WindowGpu};
use shell::Shared;
use std::sync::Arc;
use ui::view_panel::Panel;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

/// Window attributes with the name the desktop matches with `doa-player.desktop`, on each of
/// Wayland and X11.
fn named_window() -> winit::window::WindowAttributes {
    let attributes = winit::platform::wayland::WindowAttributesExtWayland::with_name(Window::default_attributes(), "doa-player", "doa-player");
    winit::platform::x11::WindowAttributesExtX11::with_name(attributes, "doa-player", "doa-player")
}

/// The main window: the 3D view, docked panels and the status bar.
struct Shell {
    window: Arc<Window>,
    gpu: WindowGpu,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    config: wgpu::SurfaceConfiguration,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    shared: Shared,
    /// Panels in their own windows.
    panels: Vec<PanelWindow>,
    /// Pixels the docked panels cover on the left and on the right, from the last frame.
    insets: (f32, f32),
    icon_set: bool,
}

/// A panel in a window of its own: the same GPU, its own surface and its own egui.
struct PanelWindow {
    kind: Panel,
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
}

impl PanelWindow {
    fn new(event_loop: &ActiveEventLoop, shell: &Shell, kind: Panel) -> anyhow::Result<Self> {
        let window = Arc::new(event_loop.create_window(named_window().with_title(kind.title()).with_inner_size(winit::dpi::LogicalSize::new(340u32, 860u32)))?);
        let surface = shell.gpu.instance.create_surface(window.clone())?;
        let capabilities = surface.get_capabilities(&shell.gpu.adapter);
        let format = capabilities.formats.iter().copied().find(|f| f.is_srgb()).unwrap_or(capabilities.formats[0]);
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: capabilities.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&shell.device, &config);
        let egui_ctx = egui::Context::default();
        egui_ctx.set_style(shell.shared.egui_ctx.style());
        let egui_state = egui_winit::State::new(egui_ctx.clone(), egui::ViewportId::ROOT, &*window, Some(window.scale_factor() as f32), None, Some(2048));
        let renderer = egui_wgpu::Renderer::new(&shell.device, format, None, 1, false);
        Ok(Self { kind, window, surface, config, egui_ctx, egui_state, renderer })
    }

    fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(device, &self.config);
        }
    }

    /// Draws the panel over the shared state.
    fn render(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, shared: &mut Shared) {
        let raw_input = self.egui_state.take_egui_input(&self.window);
        let ctx = self.egui_ctx.clone();
        let kind = self.kind;
        let output = ctx.run(raw_input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| shell::panel_contents(ui, kind, shared));
        });
        self.egui_state.handle_platform_output(&self.window, output.platform_output);
        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(_) => {
                self.surface.configure(device, &self.config);
                return;
            }
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("panel") });
        let screen = egui_wgpu::ScreenDescriptor { size_in_pixels: [self.config.width, self.config.height], pixels_per_point: self.window.scale_factor() as f32 };
        for (id, delta) in &output.textures_delta.set {
            self.renderer.update_texture(device, queue, *id, delta);
        }
        let tris = ctx.tessellate(output.shapes, output.pixels_per_point);
        self.renderer.update_buffers(device, queue, &mut encoder, &tris, &screen);
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("panel"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.02, g: 0.02, b: 0.02, a: 1.0 }), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            let mut pass = pass.forget_lifetime();
            self.renderer.render(&mut pass, &tris, &screen);
        }
        for id in &output.textures_delta.free {
            self.renderer.free_texture(id);
        }
        queue.submit([encoder.finish()]);
        frame.present();
    }
}

struct Handler {
    shell: Option<Shell>,
    last_frame: std::time::Instant,
    mouse_left_down: bool,
    mouse_right_down: bool,
    last_pos: (f64, f64),
}

impl Handler {
    /// Takes out or brings back the panels asked for.
    fn update_panels(&mut self, event_loop: &ActiveEventLoop) {
        let Some(shell) = &mut self.shell else { return };
        for (kind, detach) in std::mem::take(&mut shell.shared.panel_requests) {
            let open = shell.panels.iter().position(|p| p.kind == kind);
            match (detach, open) {
                (true, None) => match PanelWindow::new(event_loop, shell, kind) {
                    Ok(panel) => {
                        shell.panels.push(panel);
                        shell.shared.detached[kind.index()] = true;
                    }
                    Err(error) => shell.shared.status = format!("No se pudo abrir la ventana del panel: {error:#}"),
                },
                (false, Some(i)) => {
                    shell.panels.remove(i);
                    shell.shared.detached[kind.index()] = false;
                }
                _ => {}
            }
        }
    }
}

impl ApplicationHandler for Handler {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.shell.is_some() {
            return;
        }
        let window = Arc::new(
            event_loop
                .create_window(
                    named_window()
                        // `shot.sh` gives its windows a title of their own, so as not to capture
                        // another viewer that is open.
                        .with_title(std::env::var("DOA_PLAYER_TITLE").unwrap_or_else(|_| "DOA Player".to_string()))
                        // Exporting from the command line needs the GPU, not a window on the desktop.
                        .with_visible(!std::env::args().any(|a| a == "--export" || a == "--export-all" || a == "--install-icon"))
                        .with_inner_size(winit::dpi::LogicalSize::new(1600u32, 900u32)),
                )
                .expect("window creation failed"),
        );
        let (ctx, gpu) = pollster::block_on(RenderContext::new(window.clone())).expect("render context creation failed");
        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(egui_ctx.clone(), egui::ViewportId::ROOT, &*window, Some(window.scale_factor() as f32), None, Some(2048));
        let egui_renderer = egui_wgpu::Renderer::new(&ctx.device, ctx.surface_format(), None, 1, false);
        let mut shared = Shared::new(egui_ctx);
        let (device, queue, config) = (ctx.device.clone(), ctx.queue.clone(), ctx.surface_config.clone());
        // The games' files open on a worker thread; the window draws meanwhile.
        shared.start_viewer(ctx);
        // `--detach vista|capas` starts with that panel in its own window.
        let args: Vec<String> = std::env::args().collect();
        for value in args.windows(2).filter(|w| w[0] == "--detach").map(|w| w[1].as_str()) {
            match value {
                "personaje" | "character" => shared.panel_requests.push((Panel::Character, true)),
                "vista" | "view" => shared.panel_requests.push((Panel::View, true)),
                "animacion" | "animación" | "fisica" | "física" | "motion" => shared.panel_requests.push((Panel::Motion, true)),
                "capas" | "layers" => shared.panel_requests.push((Panel::Layers, true)),
                other => log::warn!("Unknown panel for --detach: {other}"),
            }
        }
        self.shell = Some(Shell { window, gpu, device, queue, config, egui_state, egui_renderer, shared, panels: Vec::new(), insets: (0.0, 0.0), icon_set: false });
        self.last_frame = std::time::Instant::now();
        log::info!("DOA Player initialized");
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(shell) = &mut self.shell else { return };
        // A panel's window.
        if let Some(i) = shell.panels.iter().position(|p| p.window.id() == id) {
            let panel = &mut shell.panels[i];
            let _ = panel.egui_state.on_window_event(&panel.window, &event);
            match &event {
                // Closing the window brings the panel back.
                WindowEvent::CloseRequested => shell.shared.panel_requests.push((panel.kind, false)),
                WindowEvent::Resized(size) => panel.resize(&shell.device, size.width, size.height),
                WindowEvent::RedrawRequested => {
                    panel.render(&shell.device, &shell.queue, &mut shell.shared);
                    shell.shared.poll();
                }
                _ => {}
            }
            self.update_panels(event_loop);
            return;
        }

        let window = shell.window.clone();
        let egui_consumed = shell.egui_state.on_window_event(&window, &event).consumed;
        match &event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
                return;
            }
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    shell.config.width = size.width;
                    shell.config.height = size.height;
                    shell.gpu.surface.configure(&shell.device, &shell.config);
                    if let Some(app) = &mut shell.shared.viewer {
                        app.resize(size.width, size.height);
                    }
                }
            }
            WindowEvent::MouseInput { button, state, .. } if !egui_consumed => match (button, state) {
                (MouseButton::Left, ElementState::Pressed) => {
                    self.mouse_left_down = true;
                    if let Some(app) = &mut shell.shared.viewer {
                        // Later drags turn around the point of the character under the cursor.
                        let size = window.inner_size();
                        let (width, height) = (size.width as f32, size.height as f32);
                        let (origin, direction) = app.camera.ray(self.last_pos.0 as f32, self.last_pos.1 as f32, width, height);
                        app.camera.pivot = app.mesh.pick(origin, direction, app.camera.pixel_spread(6.0, height));
                    }
                }
                (MouseButton::Left, ElementState::Released) => self.mouse_left_down = false,
                (MouseButton::Right | MouseButton::Middle, ElementState::Pressed) => self.mouse_right_down = true,
                (MouseButton::Right | MouseButton::Middle, ElementState::Released) => self.mouse_right_down = false,
                _ => {}
            },
            // A button let go over a panel still ends the drag.
            WindowEvent::MouseInput { state: ElementState::Released, .. } => {
                self.mouse_left_down = false;
                self.mouse_right_down = false;
            }
            WindowEvent::CursorMoved { position, .. } => {
                let delta = glam::Vec2::new((position.x - self.last_pos.0) as f32, (position.y - self.last_pos.1) as f32);
                if let Some(app) = &mut shell.shared.viewer {
                    if self.mouse_left_down {
                        app.camera.orbit(delta);
                    }
                    if self.mouse_right_down {
                        app.camera.pan(delta);
                    }
                }
                self.last_pos = (position.x, position.y);
            }
            WindowEvent::MouseWheel { delta, .. } if !egui_consumed => {
                let scroll = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 * 0.01,
                };
                if let Some(app) = &mut shell.shared.viewer {
                    app.camera.zoom(scroll);
                }
            }
            WindowEvent::RedrawRequested => {
                let now = std::time::Instant::now();
                let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.05);
                self.last_frame = now;
                if let Some(app) = &mut shell.shared.viewer {
                    app.tick(dt);
                }
                render_frame(shell);
                window.request_redraw();
            }
            _ => {}
        }
        self.update_panels(event_loop);
    }

    /// `run_app` drops the event loop and its Wayland connection when it returns. The clipboard
    /// inside egui's state borrows that connection, so it has to go first or its thread crashes
    /// on exit.
    fn exiting(&mut self, _: &ActiveEventLoop) {
        self.shell = None;
    }

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if let Some(shell) = &self.shell {
            shell.window.request_redraw();
            for panel in &shell.panels {
                panel.window.request_redraw();
            }
        }
    }
}

fn render_frame(shell: &mut Shell) {
    shell.shared.poll();
    let window = shell.window.clone();
    if let Some(app) = &mut shell.shared.viewer {
        // The viewer was away while the window changed size.
        if (app.ctx.surface_config.width, app.ctx.surface_config.height) != (shell.config.width, shell.config.height) {
            app.resize(shell.config.width, shell.config.height);
        }
        if !shell.icon_set {
            shell.icon_set = true;
            if let Some(icon) = &app.app_icon {
                // Wayland ignores this: there the icon comes from the desktop entry.
                window.set_window_icon(winit::window::Icon::from_rgba(icon.rgba.clone(), icon.width, icon.height).ok());
            }
        }
    }

    let raw_input = shell.egui_state.take_egui_input(&window);
    let egui_ctx = shell.shared.egui_ctx.clone();
    let mut insets = (0.0, 0.0);
    let shared = &mut shell.shared;
    let egui_output = egui_ctx.run(raw_input, |ctx| {
        shell::status_bar(ctx, shared);
        // Panels in their own windows are not drawn here, and their space goes to the view.
        // Left to right: Personaje, Vista, the view, Física y animación, Capas. The first panel
        // shown on a side is the outermost.
        let side = |kind: Panel, id: &str, left: bool, width: f32, shared: &mut Shared| {
            if shared.detached[kind.index()] || shared.panels_hidden {
                return 0.0;
            }
            let panel = if left { egui::SidePanel::left(id.to_string()) } else { egui::SidePanel::right(id.to_string()) };
            panel.default_width(width).show(ctx, |ui| shell::panel_contents(ui, kind, shared)).response.rect.width()
        };
        let left = side(Panel::Character, "personaje", true, 250.0, shared) + side(Panel::View, "vista", true, 230.0, shared);
        let right = side(Panel::Layers, "capas", false, 220.0, shared) + side(Panel::Motion, "animacion", false, 230.0, shared);
        shell::job_window(ctx, shared);
        if shared.viewer.is_none() {
            egui::Area::new(egui::Id::new("busy")).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.spinner();
            });
        }
        let ppp = ctx.pixels_per_point();
        insets = (left * ppp, right * ppp);
    });
    shell.insets = insets;
    shell.shared.poll();

    let surface_texture = match shell.gpu.surface.get_current_texture() {
        Ok(t) => t,
        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
            shell.gpu.surface.configure(&shell.device, &shell.config);
            return;
        }
        Err(e) => {
            log::error!("Surface error: {}", e);
            return;
        }
    };
    let view = surface_texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = shell.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Main Encoder") });

    match &mut shell.shared.viewer {
        Some(app) => draw_3d(app, &mut encoder, &view, shell.insets, &shell.config),
        // The viewer is in a job: only the background.
        None => {
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Busy Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.08, g: 0.08, b: 0.1, a: 1.0 }), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }
    }

    // egui pass
    let screen_descriptor = egui_wgpu::ScreenDescriptor { size_in_pixels: [shell.config.width, shell.config.height], pixels_per_point: window.scale_factor() as f32 };
    for (id, delta) in &egui_output.textures_delta.set {
        shell.egui_renderer.update_texture(&shell.device, &shell.queue, *id, delta);
    }
    let tris = egui_ctx.tessellate(egui_output.shapes, egui_output.pixels_per_point);
    shell.egui_renderer.update_buffers(&shell.device, &shell.queue, &mut encoder, &tris, &screen_descriptor);
    {
        let rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
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
        shell.egui_renderer.render(&mut rpass, &tris, &screen_descriptor);
    }
    for id in &egui_output.textures_delta.free {
        shell.egui_renderer.free_texture(id);
    }
    shell.egui_state.handle_platform_output(&window, egui_output.platform_output);
    shell.queue.submit(std::iter::once(encoder.finish()));
    surface_texture.present();
}

/// The character, the grid and the skeleton lines.
fn draw_3d(app: &mut app::App, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, insets: (f32, f32), config: &wgpu::SurfaceConfiguration) {
    app.camera.set_insets(insets.0, insets.1, config.width as f32, config.height as f32);
    if let Some(part) = app.bg_panel.frame_request.take() {
        if let Some((low, high)) = app.mesh.frame_box(part) {
            app.camera.frame_box(low, high);
        }
    }
    if std::mem::take(&mut app.anim_panel.reset_requested) {
        app.reset_animation();
    }
    let vp = app.camera.view_proj();
    app.grid.update(&app.ctx.queue, &vp);
    if app.mesh.is_lit() {
        let lights = app.current_stage_lights();
        app.mesh.set_lights(Some(lights));
    }
    app.mesh.plain_lighting = !app.bg_panel.game_lights;
    app.mesh.update(&app.ctx.queue, &vp, &app.camera.view_matrix(), app.bg_panel.anime_shading, app.bg_panel.relief, app.bg_panel.no_textures.then_some(app.bg_panel.plain_color));
    app.mesh.mesh_only = app.bg_panel.mesh_only;
    app.mesh.outline_width = app.bg_panel.outline_width;
    app.wireframe.update_camera(&app.ctx.queue, &vp);

    let clear = app.clear_color();
    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("3D Pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            resolve_target: None,
            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(clear), store: wgpu::StoreOp::Store },
        })],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: &app.ctx.depth_view,
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

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,wgpu_core=warn,wgpu_hal=warn,naga=warn")).init();
    log::info!("Starting DOA Player");

    let event_loop = EventLoop::new().expect("event loop creation failed");
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    let mut handler = Handler { shell: None, last_frame: std::time::Instant::now(), mouse_left_down: false, mouse_right_down: false, last_pos: (0.0, 0.0) };
    event_loop.run_app(&mut handler).expect("event loop failed");
}
