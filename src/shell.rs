//! The window side of the viewer: panels, the status bar, the process window, file dialogs and
//! jobs. Nothing slow runs on the event loop, or GNOME shows the window as not responding:
//! loading a costume, exporting and opening the games' files run on a worker thread that is
//! handed the viewer (`App`) and gives it back when done. File dialogs run on threads of their
//! own. Short jobs are waited for up to `JOB_WAIT`, so that quick costume changes do not flicker.

use crate::app::App;
use crate::ui::layers_panel::{layers_panel, LayersUi};
use crate::ui::view_panel::{detach_button, view_panel, Action, Panel};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};
use std::time::{Duration, Instant};

/// How long a new job is waited for before the window goes back to drawing.
const JOB_WAIT: Duration = Duration::from_millis(100);

enum JobMessage {
    Line(String),
    /// The viewer back, and what to show in the status bar (nothing keeps the last).
    Done(Box<App>, anyhow::Result<String>),
}

/// A long task on a worker thread, holding the viewer.
pub struct Job {
    title: String,
    receiver: Receiver<JobMessage>,
    started: Instant,
}

/// File dialogs the panels ask for.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Request {
    /// The loaded costume, to a `.glb` the user names.
    ExportAs { all_motions: bool },
}

/// A file dialog open on its own thread: the desktop portal takes as long as the user does.
pub struct Dialog {
    request: Request,
    receiver: Receiver<Option<PathBuf>>,
}

impl Dialog {
    fn open(request: Request, suggested: String, repaint: egui::Context) -> Self {
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let path = match request {
                Request::ExportAs { .. } => rfd::FileDialog::new()
                    .set_title("Exportar el traje")
                    .add_filter("glTF binario (Blender)", &["glb"])
                    .set_file_name(format!("{suggested}.glb"))
                    .save_file(),
            };
            let _ = sender.send(path);
            repaint.request_repaint();
        });
        Dialog { request, receiver }
    }
}

/// What every window's panels draw from and write to.
pub struct Shared {
    /// The viewer, or `None` while a job has it.
    pub viewer: Option<Box<App>>,
    pub layers: LayersUi,
    /// Panels in a window of their own, by `Panel::index`.
    pub detached: [bool; 2],
    /// Panels to take out (true) or bring back, handled where the event loop is at hand.
    pub panel_requests: Vec<(Panel, bool)>,
    job: Option<Job>,
    pub job_log: Vec<String>,
    pub show_job_window: bool,
    pub status: String,
    dialog: Option<Dialog>,
    pub actions: Vec<Action>,
    pub egui_ctx: egui::Context,
    /// `--no-panels`: only the 3D view and the status bar, for `shot.sh`.
    pub panels_hidden: bool,
}

impl Shared {
    pub fn new(egui_ctx: egui::Context) -> Self {
        Self {
            viewer: None,
            layers: LayersUi::default(),
            detached: [false; 2],
            panel_requests: Vec::new(),
            job: None,
            job_log: Vec::new(),
            show_job_window: false,
            status: String::new(),
            dialog: None,
            actions: Vec::new(),
            egui_ctx,
            panels_hidden: std::env::args().any(|a| a == "--no-panels"),
        }
    }

    /// The running job's title and seconds.
    pub fn job_running(&self) -> Option<(&str, f32)> {
        self.job.as_ref().map(|j| (j.title.as_str(), j.started.elapsed().as_secs_f32()))
    }

    /// Runs `work` on a worker thread. It makes the viewer, or takes it from `self` when
    /// `work` is given one. A panic becomes an error, and the viewer comes back as it was left.
    fn spawn(&mut self, title: String, work: impl FnOnce(&dyn Fn(String)) -> Option<(Box<App>, anyhow::Result<String>)> + Send + 'static) {
        let (sender, receiver) = std::sync::mpsc::channel();
        let repaint = self.egui_ctx.clone();
        std::thread::spawn(move || {
            let lines = sender.clone();
            let progress = move |line: String| {
                let _ = lines.send(JobMessage::Line(line));
                repaint.request_repaint();
            };
            if let Some((app, result)) = work(&progress) {
                let _ = sender.send(JobMessage::Done(app, result));
            }
        });
        self.status = format!("{title}…");
        self.job_log = vec![title.clone()];
        self.job = Some(Job { title, receiver, started: Instant::now() });
        self.wait_job(JOB_WAIT);
    }

    /// Opens the games' files and builds the viewer, then applies the command line.
    pub fn start_viewer(&mut self, ctx: crate::renderer::RenderContext) {
        let egui_ctx = self.egui_ctx.clone();
        self.spawn("Abriendo los archivos de los juegos".into(), move |_| {
            let started = Instant::now();
            let mut app = Box::new(App::new(ctx, &egui_ctx));
            log::info!("App ready in {} ms", started.elapsed().as_millis());
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::arguments::load_from_arguments(&mut app)));
            let status = match result {
                Ok(()) => Ok(app.status_msg.clone()),
                Err(panic) => Err(anyhow::anyhow!("la línea de órdenes falló: {}", panic_text(&panic))),
            };
            Some((app, status))
        });
    }

    /// Runs `work` on the viewer in a worker thread. Does nothing while another job runs.
    pub fn start_job(&mut self, title: impl Into<String>, open_window: bool, work: impl FnOnce(&mut App, &dyn Fn(String)) -> anyhow::Result<String> + Send + 'static) {
        let title = title.into();
        if self.job.is_some() {
            self.status = format!("Espera a que termine: {}", self.job.as_ref().unwrap().title);
            return;
        }
        let Some(mut app) = self.viewer.take() else { return };
        if open_window {
            self.show_job_window = true;
        }
        self.spawn(title, move |progress| {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(&mut app, progress)))
                .unwrap_or_else(|panic| Err(anyhow::anyhow!("falló de forma inesperada: {}", panic_text(&panic))));
            Some((app, result))
        });
    }

    /// Takes what the job has sent, waiting up to `wait` for it to end.
    fn wait_job(&mut self, wait: Duration) {
        let deadline = Instant::now() + wait;
        loop {
            let Some(job) = &self.job else { return };
            let (title, started) = (job.title.clone(), job.started);
            let left = deadline.saturating_duration_since(Instant::now());
            let received = if left.is_zero() {
                job.receiver.try_recv().map_err(|e| matches!(e, TryRecvError::Disconnected))
            } else {
                job.receiver.recv_timeout(left).map_err(|e| matches!(e, RecvTimeoutError::Disconnected))
            };
            let message = match received {
                Ok(message) => message,
                Err(true) => return self.job_lost(),
                Err(false) => return,
            };
            match message {
                JobMessage::Line(line) => {
                    self.status = format!("{title}: {}", line.trim());
                    self.job_log.push(line);
                }
                JobMessage::Done(app, result) => {
                    let seconds = started.elapsed().as_secs_f32();
                    self.job = None;
                    let line = match result {
                        Ok(text) if text.is_empty() => app.status_msg.clone(),
                        Ok(text) => text,
                        Err(error) => format!("{title}: error: {error:#}"),
                    };
                    log::debug!("{title}: {seconds:.2} s");
                    self.job_log.push(format!("{line} ({seconds:.1} s)"));
                    self.status = line;
                    self.viewer = Some(app);
                    return;
                }
            }
        }
    }

    /// The worker ended without giving the viewer back: only building it can do that.
    fn job_lost(&mut self) {
        let title = self.job.take().map(|j| j.title).unwrap_or_default();
        self.status = format!("{title}: falló; mira el registro de la terminal");
        self.job_log.push(self.status.clone());
        self.show_job_window = true;
    }

    /// Called once a frame: the job's messages, the dialog's answer, and the panels' actions.
    pub fn poll(&mut self) {
        self.wait_job(Duration::ZERO);
        if let Some(dialog) = &self.dialog {
            match dialog.receiver.try_recv() {
                Ok(path) => {
                    let request = dialog.request;
                    self.dialog = None;
                    if let Some(path) = path {
                        self.answer(request, path);
                    }
                }
                Err(TryRecvError::Disconnected) => self.dialog = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        for action in std::mem::take(&mut self.actions) {
            self.act(action);
        }
    }

    pub fn dialog_open(&self) -> bool {
        self.dialog.is_some()
    }

    fn answer(&mut self, request: Request, path: PathBuf) {
        match request {
            Request::ExportAs { all_motions } => self.start_job("Exportando el traje", true, move |app, progress| {
                let written = app.export_current(all_motions, true)?;
                progress(format!("Escrito {}", written.display()));
                std::fs::copy(&written, &path).map_err(|e| anyhow::anyhow!("no se pudo copiar a {}: {e}", path.display()))?;
                Ok(format!("Exportado {}", path.display()))
            }),
        }
    }

    fn act(&mut self, action: Action) {
        match action {
            Action::Load(character, costume) => {
                let name = self.viewer.as_ref().and_then(|app| crate::ui::view_panel::characters(app).get(character)).map_or(String::new(), |c| c.display_name.clone());
                self.start_job(format!("Cargando {name}"), false, move |app, _| {
                    app.load_character(character, costume);
                    Ok(String::new())
                });
            }
            Action::Motion(index) => self.start_job("Cargando la animación", false, move |app, _| {
                app.select_motion(index);
                Ok(String::new())
            }),
            Action::Voice(entry) => {
                if let Some(app) = &mut self.viewer {
                    if let Err(e) = app.voices.play(crate::app::doa2u_voice_path(), entry) {
                        self.status = format!("No se pudo reproducir la voz: {e:#}");
                    }
                }
            }
            Action::ExportCostume => {
                let Some(app) = &self.viewer else { return };
                let all_motions = app.char_panel.export_all_motions;
                self.start_job("Exportando el traje", true, move |app, _| {
                    let path = app.export_current(all_motions, true)?;
                    Ok(format!("Exportado {}", path.display()))
                });
            }
            Action::ExportCostumeAs => {
                let Some(app) = &self.viewer else { return };
                if self.dialog.is_none() {
                    let request = Request::ExportAs { all_motions: app.char_panel.export_all_motions };
                    let suggested = crate::export::safe_name(&app.loaded_name.2);
                    self.dialog = Some(Dialog::open(request, suggested, self.egui_ctx.clone()));
                }
            }
            Action::ExportCharacter => {
                let Some(app) = &self.viewer else { return };
                let (Some(character), all_motions) = (app.char_panel.selected_char_idx, app.char_panel.export_all_motions) else { return };
                self.start_job("Exportando todos los trajes", true, move |app, progress| {
                    let written = app.export_character(character, all_motions, progress);
                    Ok(format!("Exportados {written} trajes a exports/"))
                });
            }
            Action::OpenExportsFolder => {
                let folder = PathBuf::from("exports");
                let _ = std::fs::create_dir_all(&folder);
                // `xdg-open` returns at once; the file manager is its own process.
                if let Err(e) = std::process::Command::new("xdg-open").arg(&folder).spawn() {
                    self.status = format!("No se pudo abrir {}: {e}", folder.display());
                }
            }
            Action::ShowJob => self.show_job_window = true,
            Action::Detach(panel, out) => self.panel_requests.push((panel, out)),
        }
    }
}

fn panic_text(panic: &Box<dyn std::any::Any + Send>) -> String {
    panic.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| panic.downcast_ref::<String>().cloned()).unwrap_or_else(|| "pánico".into())
}

/// A panel's contents, in the main window or in its own.
pub fn panel_contents(ui: &mut egui::Ui, kind: Panel, shared: &mut Shared) {
    detach_button(ui, shared.detached[kind.index()], kind, &mut shared.actions);
    match kind {
        Panel::View => ui.heading("Vista"),
        Panel::Layers => ui.heading("Capas"),
    };
    let dialog_open = shared.dialog_open();
    let last_job = !shared.job_log.is_empty();
    let Shared { viewer, layers, actions, job, .. } = shared;
    match viewer {
        Some(app) => {
            // With a dialog open no other can be asked for.
            ui.add_enabled_ui(!dialog_open, |ui| match kind {
                Panel::View => egui::ScrollArea::vertical().id_salt("view panel").show(ui, |ui| view_panel(ui, app, actions, last_job)).inner,
                Panel::Layers => layers_panel(ui, app, layers),
            });
        }
        None => {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.spinner();
                let title = job.as_ref().map_or("Trabajando", |j| j.title.as_str());
                ui.label(title);
            });
            if ui.button("Ver el proceso").clicked() {
                actions.push(Action::ShowJob);
            }
        }
    }
}

/// The process window: what the job is doing, and its log.
pub fn job_window(ctx: &egui::Context, shared: &mut Shared) {
    let mut open = shared.show_job_window;
    egui::Window::new("Proceso").open(&mut open).default_width(480.0).show(ctx, |ui| {
        if let Some((title, seconds)) = shared.job_running() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(format!("{title} ({seconds:.0} s)"));
            });
        }
        egui::ScrollArea::vertical().max_height(260.0).stick_to_bottom(true).show(ui, |ui| {
            for line in &shared.job_log {
                ui.monospace(line);
            }
        });
    });
    shared.show_job_window = open;
}

/// The status bar: what the app is doing.
pub fn status_bar(ctx: &egui::Context, shared: &Shared) {
    egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
        ui.horizontal(|ui| {
            if let Some((title, seconds)) = shared.job_running() {
                ui.spinner();
                if seconds >= 1.0 {
                    ui.label(format!("{title} ({seconds:.0} s)"));
                } else {
                    ui.label(title);
                }
                if let Some(line) = shared.job_log.last().filter(|_| shared.job_log.len() > 1) {
                    ui.weak(line);
                }
            } else {
                ui.label(&shared.status);
            }
        });
    });
}
