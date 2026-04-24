/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2026 Shomy
*/

//! Root egui application: holds all UI state, pumps events from the worker,
//! and renders the main window.

use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use eframe::egui::{
    self,
    Align,
    Color32,
    Frame,
    Layout,
    Margin,
    ProgressBar,
    RichText,
    Rounding,
    ScrollArea,
    Stroke,
    TextEdit,
};
use egui_extras::{Column, TableBuilder};
use human_bytes::human_bytes;
use penumbra::core::storage::Partition;
use penumbra::da::protocol::BootMode;
use serde::{Deserialize, Serialize};

use crate::messages::{Command, ConnStatus, Event, LockAction, LogLine};
use crate::theme::{self, ThemeId};
use crate::worker::WorkerHandle;

const LOG_SCROLLBACK: usize = 4000;

/// Which main tab is visible in the content area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum Tab {
    Pgpt,
    Scatter,
    Operations,
}

impl Tab {
    fn label(self) -> &'static str {
        match self {
            Tab::Pgpt => "PGPT Manager",
            Tab::Scatter => "Scatter/XML Flasher",
            Tab::Operations => "Operations",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Tab::Pgpt => "📁",
            Tab::Scatter => "📄",
            Tab::Operations => "⚙",
        }
    }
}

/// State persisted between sessions via eframe's built-in storage.
#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Persisted {
    theme: ThemeId,
    tab: Tab,
    da_path: Option<PathBuf>,
    preloader_path: Option<PathBuf>,
    auth_path: Option<PathBuf>,
    output_dir: Option<PathBuf>,
    log_panel_width: f32,
}

impl Default for Persisted {
    fn default() -> Self {
        Self {
            theme: ThemeId::DarkPurple,
            tab: Tab::Pgpt,
            da_path: None,
            preloader_path: None,
            auth_path: None,
            output_dir: None,
            log_panel_width: 420.0,
        }
    }
}

/// Single row in the partition table.
#[derive(Clone)]
struct PartitionRow {
    partition: Partition,
    selected: bool,
    assigned_image: Option<PathBuf>,
}

/// Current long-running operation progress, if any.
#[derive(Default)]
struct Progress {
    total: u64,
    written: u64,
    message: String,
    active: bool,
}

pub struct App {
    // Persisted user preferences / file picks.
    persisted: Persisted,

    // Runtime state.
    status: ConnStatus,
    partitions: Vec<PartitionRow>,
    progress: Progress,
    input_enabled: bool,
    logs: Vec<LogLine>,
    log_filter: LogLevelFilter,
    log_autoscroll: bool,

    // Error banner.
    error: Option<String>,

    // Worker plumbing.
    handle: WorkerHandle,
    evt_rx: Receiver<Event>,
    log_rx: Receiver<LogLine>,

    // Confirm-dialog state.
    confirm: Option<ConfirmAction>,
    confirm_opened_at: Option<std::time::Instant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LogLevelFilter {
    All,
    InfoPlus,
    WarnPlus,
    ErrorOnly,
}

impl LogLevelFilter {
    fn matches(self, level: log::Level) -> bool {
        match self {
            LogLevelFilter::All => true,
            LogLevelFilter::InfoPlus => level <= log::Level::Info,
            LogLevelFilter::WarnPlus => level <= log::Level::Warn,
            LogLevelFilter::ErrorOnly => level == log::Level::Error,
        }
    }

    fn label(self) -> &'static str {
        match self {
            LogLevelFilter::All => "All",
            LogLevelFilter::InfoPlus => "Info+",
            LogLevelFilter::WarnPlus => "Warn+",
            LogLevelFilter::ErrorOnly => "Error only",
        }
    }
}

#[derive(Debug, Clone)]
enum ConfirmAction {
    UnlockBootloader,
    LockBootloader,
    WriteAssigned(Vec<(String, PathBuf)>),
    Reboot(BootMode),
    Shutdown,
}

impl ConfirmAction {
    fn title(&self) -> &'static str {
        match self {
            ConfirmAction::UnlockBootloader => "Unlock bootloader?",
            ConfirmAction::LockBootloader => "Lock bootloader?",
            ConfirmAction::WriteAssigned(_) => "Flash assigned images?",
            ConfirmAction::Reboot(_) => "Reboot device?",
            ConfirmAction::Shutdown => "Shut down device?",
        }
    }

    fn body(&self) -> String {
        match self {
            ConfirmAction::UnlockBootloader => {
                "You are about to clear the seccfg partition via DA extensions.\n\n\
                 READ THIS BEFORE PROCEEDING:\n\n\
                 - Unlocking will WIPE userdata on the next boot. Back up anything you care \
                   about first.\n\
                 - After unlocking, the device boots with a tamper warning until re-locked.\n\
                 - Make sure the battery is sufficiently charged and the USB cable is \
                   reliable; an interrupted seccfg write can leave the device unbootable.\n\n\
                 Do you want to continue?"
                    .into()
            }
            ConfirmAction::LockBootloader => {
                "You are about to RE-LOCK the bootloader by restoring seccfg.\n\n\
                 READ THIS BEFORE PROCEEDING:\n\n\
                 - Locking while the device is running a port ROM, custom ROM, or any \
                   modified image (boot, vbmeta, super, recovery, dtbo) is the #1 way to \
                   HARD-BRICK a MediaTek phone.\n\
                 - Lock ONLY after you have flashed full, unmodified STOCK firmware for \
                   your exact model and region. If you are not 100% sure every partition \
                   is stock, do NOT lock.\n\
                 - Relocking will usually wipe userdata on the next boot.\n\
                 - There is no guaranteed recovery path if the device refuses to boot \
                   after locking on a modified image.\n\n\
                 Flash unmodified stock firmware first, verify the device boots cleanly, \
                 THEN come back and lock.\n\n\
                 Do you want to continue?"
                    .into()
            }
            ConfirmAction::WriteAssigned(list) => {
                let mut s = String::from("The following partitions will be OVERWRITTEN:\n\n");
                for (p, img) in list {
                    s.push_str(&format!(
                        "  • {p}  ←  {}\n",
                        img.file_name().and_then(|n| n.to_str()).unwrap_or("?")
                    ));
                }
                s.push_str("\nThis cannot be undone. Continue?");
                s
            }
            ConfirmAction::Reboot(_) => "The device will reboot and disconnect.".into(),
            ConfirmAction::Shutdown => "The device will power off and disconnect.".into(),
        }
    }
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        handle: WorkerHandle,
        evt_rx: Receiver<Event>,
        log_rx: Receiver<LogLine>,
    ) -> Self {
        let persisted: Persisted =
            cc.storage.and_then(|s| eframe::get_value(s, "penumbra-gui")).unwrap_or_default();

        theme::apply(persisted.theme.palette(), &cc.egui_ctx);

        App {
            persisted,
            status: ConnStatus::Disconnected,
            partitions: Vec::new(),
            progress: Progress::default(),
            input_enabled: true,
            logs: Vec::new(),
            log_filter: LogLevelFilter::All,
            log_autoscroll: true,
            error: None,
            handle,
            evt_rx,
            log_rx,
            confirm: None,
            confirm_opened_at: None,
        }
    }

    fn drain_events(&mut self, ctx: &egui::Context) {
        while let Ok(evt) = self.evt_rx.try_recv() {
            self.apply_event(evt);
            ctx.request_repaint();
        }
        while let Ok(line) = self.log_rx.try_recv() {
            self.push_log(line);
            ctx.request_repaint();
        }
    }

    fn apply_event(&mut self, evt: Event) {
        match evt {
            Event::StatusChanged(s) => {
                if matches!(s, ConnStatus::Disconnected) {
                    self.partitions.clear();
                    self.progress = Progress::default();
                }
                self.status = s;
            }
            Event::PartitionsLoaded { partitions } => {
                let existing: std::collections::HashMap<String, PartitionRow> =
                    self.partitions.drain(..).map(|r| (r.partition.name.clone(), r)).collect();
                self.partitions = partitions
                    .into_iter()
                    .map(|p| {
                        let prev = existing.get(&p.name);
                        PartitionRow {
                            partition: p.clone(),
                            selected: prev.map(|r| r.selected).unwrap_or(false),
                            assigned_image: prev.and_then(|r| r.assigned_image.clone()),
                        }
                    })
                    .collect();
            }
            Event::ProgressStart { total_bytes, message } => {
                self.progress = Progress { total: total_bytes, written: 0, message, active: true };
            }
            Event::ProgressUpdate { written, message } => {
                self.progress.written = written;
                if let Some(m) = message {
                    self.progress.message = m;
                }
            }
            Event::ProgressFinish { message } => {
                self.progress.message = message;
                self.progress.active = false;
                self.progress.written = self.progress.total;
            }
            Event::Error(msg) => {
                self.error = Some(msg.clone());
                self.push_log(LogLine {
                    level: log::Level::Error,
                    target: "penumbra_gui".into(),
                    message: msg,
                });
            }
            Event::InputEnabled(enabled) => {
                self.input_enabled = enabled;
            }
        }
    }

    fn push_log(&mut self, line: LogLine) {
        self.logs.push(line);
        if self.logs.len() > LOG_SCROLLBACK {
            let excess = self.logs.len() - LOG_SCROLLBACK;
            self.logs.drain(0..excess);
        }
    }

    fn send(&self, cmd: Command) {
        if let Err(e) = self.handle.cmd_tx.send(cmd) {
            log::error!("worker channel closed: {e}");
        }
    }

    fn cancel(&self) {
        self.handle.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
        let _ = self.handle.cmd_tx.send(Command::Cancel);
    }
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "penumbra-gui", &self.persisted);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events(ctx);
        theme::apply(self.persisted.theme.palette(), ctx);

        // Paint the root background explicitly so themes feel "full-bleed".
        let palette = self.persisted.theme.palette();

        egui::TopBottomPanel::top("header")
            .exact_height(64.0)
            .frame(panel_frame(palette.panel, palette.border, 0.0))
            .show(ctx, |ui| self.draw_header(ui));

        egui::TopBottomPanel::top("file_row")
            .exact_height(90.0)
            .frame(panel_frame(palette.panel, palette.border, 0.0))
            .show(ctx, |ui| self.draw_file_row(ui));

        egui::TopBottomPanel::top("tabs")
            .exact_height(40.0)
            .frame(panel_frame(palette.panel, palette.border, 0.0))
            .show(ctx, |ui| self.draw_tab_bar(ui));

        egui::TopBottomPanel::bottom("status")
            .exact_height(46.0)
            .frame(panel_frame(palette.panel_alt, palette.border, 0.0))
            .show(ctx, |ui| self.draw_status_bar(ui));

        if self.progress.active || self.progress.total > 0 {
            egui::TopBottomPanel::bottom("progress")
                .exact_height(38.0)
                .frame(panel_frame(palette.panel, palette.border, 0.0))
                .show(ctx, |ui| self.draw_progress_bar(ui));
        }

        let log_panel = egui::SidePanel::right("execution_log")
            .resizable(true)
            .default_width(self.persisted.log_panel_width)
            .width_range(180.0..=900.0)
            .frame(panel_frame(palette.panel, palette.border, 0.0))
            .show(ctx, |ui| self.draw_exec_log(ui, palette));
        let new_log_width = log_panel.response.rect.width();
        if (new_log_width - self.persisted.log_panel_width).abs() > 0.5 {
            self.persisted.log_panel_width = new_log_width;
        }

        egui::CentralPanel::default()
            .frame(panel_frame(palette.background, palette.border, 0.0))
            .show(ctx, |ui| {
                self.draw_error_banner(ui, palette);
                match self.persisted.tab {
                    Tab::Pgpt => self.draw_pgpt_tab(ui, palette),
                    Tab::Scatter => self.draw_scatter_tab(ui, palette),
                    Tab::Operations => self.draw_operations_tab(ui, palette),
                }
            });

        if let Some(action) = self.confirm.clone() {
            self.draw_confirm_dialog(ctx, palette, action);
        }
    }
}

// -------------------------------------------------------------------
// Drawing helpers
// -------------------------------------------------------------------

fn timestamp_stamp() -> String {
    // UNIX seconds formatted as `YYYYMMDD-HHMMSS` (UTC). Pure std; avoids
    // pulling in another dependency just for folder names.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    // Days since 1970-01-01, then civil-date from days (Hinnant's algorithm).
    let days = now.div_euclid(86_400);
    let secs_of_day = now.rem_euclid(86_400);
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day / 60) % 60;
    let second = secs_of_day % 60;

    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };

    format!("{year:04}{m:02}{d:02}-{hour:02}{minute:02}{second:02}")
}

fn status_dot(ui: &mut egui::Ui, color: Color32) {
    // Paint the status circle directly so it doesn't depend on a font glyph.
    let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, color);
}

fn panel_frame(fill: Color32, border: Color32, radius: f32) -> egui::Frame {
    egui::Frame::none()
        .fill(fill)
        .stroke(Stroke::new(1.0_f32, border))
        .inner_margin(Margin::same(10.0))
        .rounding(Rounding::same(radius))
}

fn badge(ui: &mut egui::Ui, text: &str, fill: Color32, fg: Color32) {
    Frame::none()
        .fill(fill)
        .rounding(Rounding::same(4.0))
        .inner_margin(Margin::symmetric(10.0, 4.0))
        .show(ui, |ui| {
            ui.label(RichText::new(text).strong().color(fg));
        });
}

impl App {
    fn draw_header(&mut self, ui: &mut egui::Ui) {
        let palette = self.persisted.theme.palette();
        ui.horizontal(|ui| {
            badge(ui, "PENUMBRA TOOL", palette.header_badge, Color32::WHITE);
            ui.add_space(10.0);
            ui.label(RichText::new("Penumbra Flash Tool").strong().color(palette.text).size(16.0));
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                    .color(palette.text_muted)
                    .size(12.0),
            );

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("theme_combo")
                        .selected_text(self.persisted.theme.label())
                        .show_ui(ui, |ui| {
                            for &t in ThemeId::ALL {
                                ui.selectable_value(&mut self.persisted.theme, t, t.label());
                            }
                        });
                    ui.label(RichText::new("Theme:").color(palette.text_muted));
                });
                ui.add_space(12.0);
                self.draw_status_pill(ui, palette);
            });
        });
    }

    fn draw_status_pill(&self, ui: &mut egui::Ui, palette: theme::Palette) {
        let (label, color) = match &self.status {
            ConnStatus::Disconnected => ("Disconnected".to_string(), palette.text_muted),
            ConnStatus::Connecting => ("Connecting...".to_string(), palette.warn),
            ConnStatus::Connected { chip_name, hw_code } => {
                (format!("Connected · {chip_name} (0x{hw_code:04X})"), palette.success)
            }
        };
        Frame::none()
            .fill(palette.panel_alt)
            .stroke(Stroke::new(1.0_f32, color))
            .rounding(Rounding::same(6.0))
            .inner_margin(Margin::symmetric(10.0, 4.0))
            .show(ui, |ui| {
                status_dot(ui, color);
                ui.label(RichText::new(label).color(palette.text));
            });
    }

    fn draw_file_row(&mut self, ui: &mut egui::Ui) {
        let palette = self.persisted.theme.palette();
        ui.vertical(|ui| {
            self.draw_path_row(ui, palette, "Download Agent (DA):", PathKind::Da);
            ui.add_space(4.0);
            self.draw_path_row(ui, palette, "Output/Backup Folder:", PathKind::OutputDir);
        });
    }

    fn draw_path_row(
        &mut self,
        ui: &mut egui::Ui,
        palette: theme::Palette,
        label: &str,
        kind: PathKind,
    ) {
        ui.horizontal(|ui| {
            ui.add_sized(
                [180.0, 24.0],
                egui::Label::new(RichText::new(label).color(palette.text_muted)),
            );

            let mut text = match kind {
                PathKind::Da => self.persisted.da_path.as_ref(),
                PathKind::Preloader => self.persisted.preloader_path.as_ref(),
                PathKind::Auth => self.persisted.auth_path.as_ref(),
                PathKind::OutputDir => self.persisted.output_dir.as_ref(),
            }
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| match kind {
                PathKind::OutputDir => String::from("Select output folder..."),
                _ => String::from("Select file..."),
            });

            let avail = ui.available_width() - 110.0;
            ui.add_sized(
                [avail.max(120.0), 24.0],
                TextEdit::singleline(&mut text).interactive(false),
            );

            let btn_label = match kind {
                PathKind::OutputDir => "📁 Select",
                _ => "📂 Browse",
            };
            if ui
                .add_enabled(
                    self.input_enabled,
                    egui::Button::new(btn_label).min_size(egui::vec2(94.0, 24.0)),
                )
                .clicked()
            {
                self.pick_path(kind);
            }
        });
    }

    fn pick_path(&mut self, kind: PathKind) {
        match kind {
            PathKind::OutputDir => {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    self.persisted.output_dir = Some(dir);
                }
            }
            other => {
                let dlg = rfd::FileDialog::new()
                    .set_title(other.dialog_title())
                    .add_filter("bin", &["bin"])
                    .add_filter("all", &["*"]);
                if let Some(file) = dlg.pick_file() {
                    match other {
                        PathKind::Da => self.persisted.da_path = Some(file),
                        PathKind::Preloader => self.persisted.preloader_path = Some(file),
                        PathKind::Auth => self.persisted.auth_path = Some(file),
                        PathKind::OutputDir => {}
                    }
                }
            }
        }
    }

    fn draw_tab_bar(&mut self, ui: &mut egui::Ui) {
        let palette = self.persisted.theme.palette();
        ui.horizontal(|ui| {
            for &tab in &[Tab::Pgpt, Tab::Scatter, Tab::Operations] {
                let active = self.persisted.tab == tab;
                let label = format!("{} {}", tab.icon(), tab.label());
                let text = if active {
                    RichText::new(label).strong().color(Color32::WHITE)
                } else {
                    RichText::new(label).color(palette.text)
                };
                let mut btn = egui::Button::new(text).min_size(egui::vec2(180.0, 26.0));
                if active {
                    btn = btn
                        .fill(palette.accent)
                        .stroke(Stroke::new(1.0_f32, palette.accent_strong));
                }
                if ui.add(btn).clicked() {
                    self.persisted.tab = tab;
                }
                ui.add_space(4.0);
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let connected = matches!(self.status, ConnStatus::Connected { .. });
                let ui_enabled =
                    self.input_enabled && !matches!(self.status, ConnStatus::Connecting);

                let label = if connected { "⏻ Disconnect" } else { "🔌 Connect" };
                let btn = egui::Button::new(RichText::new(label).color(Color32::WHITE))
                    .fill(if connected { palette.error } else { palette.accent })
                    .stroke(Stroke::new(1.0_f32, palette.border))
                    .min_size(egui::vec2(130.0, 26.0));
                if ui.add_enabled(ui_enabled, btn).clicked() {
                    if connected {
                        self.send(Command::Disconnect);
                    } else {
                        self.send(Command::Connect {
                            da_path: self.persisted.da_path.clone(),
                            preloader_path: self.persisted.preloader_path.clone(),
                            auth_path: self.persisted.auth_path.clone(),
                        });
                    }
                }

                ui.add_space(8.0);

                let preloader_loaded = self.persisted.preloader_path.is_some();
                let pl_btn = egui::Button::new(
                    RichText::new(if preloader_loaded {
                        "⚡ Preloader ✓"
                    } else {
                        "⚡ Preloader"
                    })
                    .color(palette.text),
                )
                .min_size(egui::vec2(120.0, 26.0));
                if ui.add_enabled(self.input_enabled, pl_btn).clicked() {
                    self.pick_path(PathKind::Preloader);
                }

                let auth_loaded = self.persisted.auth_path.is_some();
                let auth_btn = egui::Button::new(
                    RichText::new(if auth_loaded { "🔑 Auth ✓" } else { "🔑 Auth" })
                        .color(palette.text),
                )
                .min_size(egui::vec2(100.0, 26.0));
                if ui.add_enabled(self.input_enabled, auth_btn).clicked() {
                    self.pick_path(PathKind::Auth);
                }
            });
        });
    }

    fn draw_status_bar(&self, ui: &mut egui::Ui) {
        let palette = self.persisted.theme.palette();
        ui.horizontal(|ui| {
            let (label, color) = match &self.status {
                ConnStatus::Disconnected => ("System Ready", palette.success),
                ConnStatus::Connecting => ("Connecting...", palette.warn),
                ConnStatus::Connected { .. } => ("Device Connected", palette.success),
            };
            status_dot(ui, color);
            ui.label(RichText::new(label).color(color).strong());

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.input_enabled {
                    ui.label(RichText::new("Idle").color(palette.text_muted));
                } else {
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("■ STOP OPERATION").color(Color32::WHITE),
                            )
                            .fill(palette.error),
                        )
                        .clicked()
                    {
                        self.cancel();
                    }
                    ui.label(RichText::new("Busy...").color(palette.warn));
                }
            });
        });
    }

    fn draw_progress_bar(&self, ui: &mut egui::Ui) {
        let palette = self.persisted.theme.palette();
        let ratio = if self.progress.total == 0 {
            0.0
        } else {
            (self.progress.written as f32 / self.progress.total as f32).clamp(0.0, 1.0)
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new(&self.progress.message).color(palette.text_muted));
            ui.add(
                ProgressBar::new(ratio)
                    .desired_width(ui.available_width() - 200.0)
                    .fill(palette.accent_strong),
            );
            ui.label(
                RichText::new(format!(
                    "{} / {}",
                    human_bytes(self.progress.written as f64),
                    human_bytes(self.progress.total as f64)
                ))
                .color(palette.text),
            );
        });
    }

    fn draw_error_banner(&mut self, ui: &mut egui::Ui, palette: theme::Palette) {
        let Some(err) = self.error.clone() else { return };
        Frame::none()
            .fill(palette.error.gamma_multiply(0.15))
            .stroke(Stroke::new(1.0_f32, palette.error))
            .rounding(Rounding::same(6.0))
            .inner_margin(Margin::same(8.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("✖").color(palette.error).strong());
                    ui.label(RichText::new(&err).color(palette.text));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("Dismiss").clicked() {
                            self.error = None;
                        }
                    });
                });
            });
        ui.add_space(6.0);
    }

    fn draw_pgpt_tab(&mut self, ui: &mut egui::Ui, palette: theme::Palette) {
        let connected = matches!(self.status, ConnStatus::Connected { .. });
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Partition List (Double-click a row to assign an image):")
                    .color(palette.text_muted),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{} partitions", self.partitions.len()))
                        .color(palette.text_muted),
                );
            });
        });

        ui.add_space(4.0);

        // Reserve room for the two action-button rows below the table so
        // nothing gets clipped when the central pane is short.
        const ACTION_ROWS_HEIGHT: f32 = 120.0;
        let table_height = (ui.available_height() - ACTION_ROWS_HEIGHT).max(160.0);
        ui.allocate_ui(egui::vec2(ui.available_width(), table_height), |ui| {
            self.draw_partition_table(ui, palette);
        });
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            let enabled = connected && self.input_enabled;
            if ui
                .add_enabled(
                    enabled,
                    egui::Button::new("📥 LOAD PGPT").min_size(egui::vec2(160.0, 28.0)),
                )
                .clicked()
            {
                self.send(Command::LoadPgpt);
            }
            ui.add_space(6.0);
            if ui
                .add_enabled(
                    enabled && !self.partitions.is_empty(),
                    egui::Button::new("⬇ READ SELECTED").min_size(egui::vec2(160.0, 28.0)),
                )
                .clicked()
            {
                self.start_read_selected();
            }
            ui.add_space(6.0);
            if ui
                .add_enabled(
                    self.input_enabled
                        && self.persisted.output_dir.is_some()
                        && !self.partitions.is_empty(),
                    egui::Button::new("✨ AUTO-ASSIGN").min_size(egui::vec2(160.0, 28.0)),
                )
                .clicked()
            {
                self.auto_assign_images();
            }
        });

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let assignments = self.collect_assignments();
            let write_enabled = connected && self.input_enabled && !assignments.is_empty();
            let write_btn = egui::Button::new(
                RichText::new("🔥 WRITE ASSIGNED").color(Color32::WHITE).strong(),
            )
            .fill(palette.accent_strong)
            .min_size(egui::vec2(260.0, 32.0));
            if ui.add_enabled(write_enabled, write_btn).clicked() {
                self.open_confirm(ConfirmAction::WriteAssigned(assignments));
            }

            ui.add_space(6.0);

            let smart_btn = egui::Button::new(
                RichText::new("💾 SMART BACKUP (NVRAM / EFS / NVCFG)")
                    .color(Color32::WHITE)
                    .strong(),
            )
            .fill(palette.smart_backup)
            .min_size(egui::vec2(340.0, 32.0));
            if ui
                .add_enabled(
                    connected && self.input_enabled && self.persisted.output_dir.is_some(),
                    smart_btn,
                )
                .clicked()
            {
                self.start_smart_backup();
            }
        });
    }

    fn draw_partition_table(&mut self, ui: &mut egui::Ui, palette: theme::Palette) {
        // Fills whatever height the parent `allocate_ui` gave us. The frame's
        // inner_margin eats 12 px of vertical space; leave a little headroom.
        let inner_min_height = (ui.available_height() - 12.0).max(160.0);
        Frame::none()
            .fill(palette.panel)
            .stroke(Stroke::new(1.0_f32, palette.border))
            .rounding(Rounding::same(6.0))
            .inner_margin(Margin::same(6.0))
            .show(ui, |ui| {
                ui.set_min_height(inner_min_height);
                if self.partitions.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new(
                                "No partitions loaded.\nConnect a device and press LOAD PGPT.",
                            )
                            .color(palette.text_muted),
                        );
                    });
                    return;
                }

                let mut assign_target: Option<usize> = None;
                TableBuilder::new(ui)
                    .striped(true)
                    .resizable(true)
                    .cell_layout(Layout::left_to_right(Align::Center))
                    .column(Column::exact(28.0))
                    .column(Column::auto().at_least(48.0))
                    .column(Column::initial(220.0).at_least(120.0))
                    .column(Column::auto().at_least(100.0))
                    .column(Column::auto().at_least(120.0))
                    .column(Column::remainder().at_least(180.0))
                    .header(22.0, |mut header| {
                        for h in ["", "#", "Name", "Size", "Address", "Assigned Image"] {
                            header.col(|ui| {
                                ui.label(RichText::new(h).strong().color(palette.text_muted));
                            });
                        }
                    })
                    .body(|mut body| {
                        for (i, row) in self.partitions.iter_mut().enumerate() {
                            body.row(22.0, |mut r| {
                                r.col(|ui| {
                                    ui.checkbox(&mut row.selected, "");
                                });
                                r.col(|ui| {
                                    ui.label(
                                        RichText::new(format!("{i}")).color(palette.text_muted),
                                    );
                                });
                                r.col(|ui| {
                                    let resp = ui.add(
                                        egui::Label::new(
                                            RichText::new(&row.partition.name).color(palette.text),
                                        )
                                        .sense(egui::Sense::click()),
                                    );
                                    if resp.double_clicked() {
                                        assign_target = Some(i);
                                    }
                                });
                                r.col(|ui| {
                                    ui.label(
                                        RichText::new(human_bytes(row.partition.size as f64))
                                            .color(palette.text),
                                    );
                                });
                                r.col(|ui| {
                                    ui.label(
                                        RichText::new(format!("0x{:X}", row.partition.address))
                                            .color(palette.text_muted),
                                    );
                                });
                                r.col(|ui| {
                                    let text = row
                                        .assigned_image
                                        .as_ref()
                                        .and_then(|p| p.file_name())
                                        .and_then(|n| n.to_str())
                                        .unwrap_or("—")
                                        .to_string();
                                    let resp = ui.add(
                                        egui::Label::new(RichText::new(text).color(
                                            if row.assigned_image.is_some() {
                                                palette.accent_strong
                                            } else {
                                                palette.text_muted
                                            },
                                        ))
                                        .sense(egui::Sense::click()),
                                    );
                                    if resp.clicked() && row.assigned_image.is_some() {
                                        row.assigned_image = None;
                                    } else if resp.double_clicked() {
                                        assign_target = Some(i);
                                    }
                                });
                            });
                        }
                    });

                if let Some(idx) = assign_target
                    && let Some(file) = rfd::FileDialog::new()
                        .set_title(format!(
                            "Assign image for '{}'",
                            self.partitions[idx].partition.name
                        ))
                        .add_filter("images", &["img", "bin", "mbn"])
                        .add_filter("all", &["*"])
                        .pick_file()
                {
                    self.partitions[idx].assigned_image = Some(file);
                }
            });
    }

    fn collect_assignments(&self) -> Vec<(String, PathBuf)> {
        self.partitions
            .iter()
            .filter_map(|r| {
                r.assigned_image.as_ref().map(|p| (r.partition.name.clone(), p.clone()))
            })
            .collect()
    }

    fn start_read_selected(&self) {
        let Some(out) = self.persisted.output_dir.clone() else {
            log::warn!("Pick an output folder first.");
            return;
        };
        let names: Vec<String> = self
            .partitions
            .iter()
            .filter(|r| r.selected)
            .map(|r| r.partition.name.clone())
            .collect();
        if names.is_empty() {
            log::warn!("No partitions selected.");
            return;
        }
        self.send(Command::ReadPartitions { names, output_dir: out });
    }

    fn start_smart_backup(&self) {
        let Some(out) = self.persisted.output_dir.clone() else { return };
        let wanted = [
            "nvram",
            "nvdata",
            "nvcfg",
            "proinfo",
            "protect1",
            "protect2",
            "persist",
            "persistent",
            "efs",
            "frp",
            "md1img",
            "md_udc",
        ];
        let names: Vec<String> = self
            .partitions
            .iter()
            .map(|r| r.partition.name.clone())
            .filter(|n| wanted.iter().any(|w| n.eq_ignore_ascii_case(w)))
            .collect();
        if names.is_empty() {
            log::warn!("No NVRAM/EFS/NVCFG-style partitions found in this PGPT.");
            return;
        }
        let dir = out.join(format!("smart-backup-{}", timestamp_stamp()));
        if let Err(e) = std::fs::create_dir_all(&dir) {
            log::error!("Failed to create Smart Backup folder {}: {e}", dir.display());
            return;
        }
        log::info!("Smart Backup: {} partitions → {}", names.len(), dir.display());
        self.send(Command::ReadPartitions { names, output_dir: dir });
    }

    fn auto_assign_images(&mut self) {
        let Some(dir) = self.persisted.output_dir.clone() else { return };
        let mut assigned = 0usize;
        for row in &mut self.partitions {
            for ext in ["img", "bin", "mbn"] {
                let candidate = dir.join(format!("{}.{ext}", row.partition.name));
                if candidate.is_file() {
                    row.assigned_image = Some(candidate);
                    assigned += 1;
                    break;
                }
            }
        }
        log::info!("Auto-assigned {assigned} partition image(s) from {}.", dir.display());
    }

    fn draw_scatter_tab(&mut self, ui: &mut egui::Ui, palette: theme::Palette) {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.label(
                RichText::new("Scatter / XML Flasher").strong().size(18.0).color(palette.text),
            );
            ui.add_space(10.0);
            ui.label(
                RichText::new(
                    "Load a MediaTek scatter file and map images to partitions automatically.\n\
                     Planned for PR #2. For now, use PGPT Manager's Auto-Assign and Write.",
                )
                .color(palette.text_muted),
            );
        });
    }

    fn draw_operations_tab(&mut self, ui: &mut egui::Ui, palette: theme::Palette) {
        let connected = matches!(self.status, ConnStatus::Connected { .. });
        let enabled = connected && self.input_enabled;

        ui.label(RichText::new("Bootloader").color(palette.text_muted).strong());
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let unlock = egui::Button::new(
                RichText::new("🔓 UNLOCK BOOTLOADER").color(Color32::WHITE).strong(),
            )
            .fill(palette.accent)
            .min_size(egui::vec2(220.0, 36.0));
            if ui.add_enabled(enabled, unlock).clicked() {
                self.open_confirm(ConfirmAction::UnlockBootloader);
            }
            let lock = egui::Button::new(
                RichText::new("🔒 LOCK BOOTLOADER").color(Color32::WHITE).strong(),
            )
            .fill(palette.warn)
            .min_size(egui::vec2(220.0, 36.0));
            if ui.add_enabled(enabled, lock).clicked() {
                self.open_confirm(ConfirmAction::LockBootloader);
            }
        });

        ui.add_space(16.0);
        ui.label(RichText::new("Power").color(palette.text_muted).strong());
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let normal = egui::Button::new("↻ Reboot (Normal)").min_size(egui::vec2(200.0, 32.0));
            if ui.add_enabled(enabled, normal).clicked() {
                self.open_confirm(ConfirmAction::Reboot(BootMode::Normal));
            }
            let fastboot =
                egui::Button::new("⚡ Reboot Fastboot").min_size(egui::vec2(200.0, 32.0));
            if ui.add_enabled(enabled, fastboot).clicked() {
                self.open_confirm(ConfirmAction::Reboot(BootMode::Fastboot));
            }
            let shutdown_btn =
                egui::Button::new(RichText::new("⏻ Shut Down").color(Color32::WHITE))
                    .fill(palette.error)
                    .min_size(egui::vec2(160.0, 32.0));
            if ui.add_enabled(enabled, shutdown_btn).clicked() {
                self.open_confirm(ConfirmAction::Shutdown);
            }
        });

        ui.add_space(16.0);
        ui.label(RichText::new("Device Info").color(palette.text_muted).strong());
        ui.add_space(4.0);
        self.draw_devinfo(ui, palette);
    }

    fn draw_devinfo(&self, ui: &mut egui::Ui, palette: theme::Palette) {
        Frame::none()
            .fill(palette.panel)
            .stroke(Stroke::new(1.0_f32, palette.border))
            .rounding(Rounding::same(6.0))
            .inner_margin(Margin::same(10.0))
            .show(ui, |ui| match &self.status {
                ConnStatus::Connected { chip_name, hw_code } => {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Chip:").color(palette.text_muted));
                        ui.label(RichText::new(chip_name).color(palette.text).strong());
                    });
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("HW code:").color(palette.text_muted));
                        ui.label(RichText::new(format!("0x{hw_code:04X}")).color(palette.text));
                    });
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Partitions:").color(palette.text_muted));
                        ui.label(
                            RichText::new(format!("{}", self.partitions.len())).color(palette.text),
                        );
                    });
                }
                _ => {
                    ui.label(RichText::new("No device connected.").color(palette.text_muted));
                }
            });
    }

    fn draw_exec_log(&mut self, ui: &mut egui::Ui, palette: theme::Palette) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("📃 EXECUTION LOG").strong().color(palette.text));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                egui::ComboBox::from_id_salt("log_filter_combo")
                    .selected_text(self.log_filter.label())
                    .show_ui(ui, |ui| {
                        for f in [
                            LogLevelFilter::All,
                            LogLevelFilter::InfoPlus,
                            LogLevelFilter::WarnPlus,
                            LogLevelFilter::ErrorOnly,
                        ] {
                            ui.selectable_value(&mut self.log_filter, f, f.label());
                        }
                    });
                ui.checkbox(&mut self.log_autoscroll, "Autoscroll");
            });
        });
        ui.separator();

        let avail_h = ui.available_height() - 46.0;
        Frame::none()
            .fill(palette.panel_alt)
            .stroke(Stroke::new(1.0_f32, palette.border))
            .rounding(Rounding::same(6.0))
            .inner_margin(Margin::same(6.0))
            .show(ui, |ui| {
                ui.set_min_height(avail_h.max(100.0));
                let mut scroll = ScrollArea::vertical().auto_shrink([false, false]);
                if self.log_autoscroll {
                    scroll = scroll.stick_to_bottom(true);
                }
                scroll.show(ui, |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                    for line in &self.logs {
                        if !self.log_filter.matches(line.level) {
                            continue;
                        }
                        let color = match line.level {
                            log::Level::Error => palette.error,
                            log::Level::Warn => palette.warn,
                            log::Level::Info => palette.text,
                            log::Level::Debug | log::Level::Trace => palette.text_muted,
                        };
                        let text = format!("[{}] {}", line.level, line.message);
                        ui.add(
                            egui::Label::new(
                                RichText::new(text).color(color).monospace(),
                            )
                            .wrap(),
                        );
                    }
                });
            });

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("🧹 Clear Log").clicked() {
                self.logs.clear();
            }
            if ui.button("💾 Save Log").clicked() {
                self.save_log_to_file();
            }
            if ui.button("📋 Copy Log").clicked() {
                let text = self.rendered_log_text();
                ui.ctx().output_mut(|o| o.copied_text = text);
            }
        });
    }

    fn rendered_log_text(&self) -> String {
        let mut out = String::new();
        for line in &self.logs {
            if !self.log_filter.matches(line.level) {
                continue;
            }
            out.push_str(&format!("[{}] {}\n", line.level, line.message));
        }
        out
    }

    fn save_log_to_file(&self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("log", &["log", "txt"])
            .set_file_name("penumbra-gui.log")
            .save_file()
        else {
            return;
        };
        let text = self.rendered_log_text();
        if let Err(e) = std::fs::write(&path, text) {
            log::error!("Failed to save log: {e}");
        } else {
            log::info!("Log saved to {}", path.display());
        }
    }

    fn draw_confirm_dialog(
        &mut self,
        ctx: &egui::Context,
        palette: theme::Palette,
        action: ConfirmAction,
    ) {
        let mut close = false;
        let mut accept = false;

        // Bootloader lock/unlock get a mandatory 15 s read-the-warning delay
        // before the Proceed button becomes clickable.
        const BOOTLOADER_DELAY_SECS: f32 = 15.0;
        let delayed = matches!(
            action,
            ConfirmAction::UnlockBootloader | ConfirmAction::LockBootloader
        );
        let remaining = if delayed {
            let elapsed = self
                .confirm_opened_at
                .map(|t| t.elapsed().as_secs_f32())
                .unwrap_or(0.0);
            (BOOTLOADER_DELAY_SECS - elapsed).max(0.0)
        } else {
            0.0
        };
        let proceed_enabled = !delayed || remaining <= 0.0;
        if delayed && remaining > 0.0 {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }

        egui::Window::new(RichText::new(action.title()).strong().color(palette.text))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .frame(
                Frame::none()
                    .fill(palette.panel)
                    .stroke(Stroke::new(1.0_f32, palette.border))
                    .rounding(Rounding::same(8.0))
                    .inner_margin(Margin::same(16.0)),
            )
            .show(ctx, |ui| {
                ui.set_min_width(if delayed { 520.0 } else { 420.0 });
                ui.set_max_width(if delayed { 520.0 } else { 420.0 });
                if delayed {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                }
                ui.label(RichText::new(action.body()).color(palette.text));
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(egui::Button::new("Cancel").min_size(egui::vec2(100.0, 28.0)))
                        .clicked()
                    {
                        close = true;
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let btn_text = if delayed && remaining > 0.0 {
                            format!("Proceed in {}s", remaining.ceil() as u32)
                        } else {
                            "Proceed".to_string()
                        };
                        let btn = egui::Button::new(
                            RichText::new(btn_text).color(Color32::WHITE).strong(),
                        )
                        .fill(palette.error)
                        .min_size(egui::vec2(160.0, 28.0));
                        if ui.add_enabled(proceed_enabled, btn).clicked() {
                            accept = true;
                        }
                    });
                });
            });

        if accept {
            match action {
                ConfirmAction::UnlockBootloader => self.send(Command::Seccfg(LockAction::Unlock)),
                ConfirmAction::LockBootloader => self.send(Command::Seccfg(LockAction::Lock)),
                ConfirmAction::WriteAssigned(list) => {
                    self.send(Command::WriteAssigned { assignments: list })
                }
                ConfirmAction::Reboot(mode) => self.send(Command::Reboot(mode)),
                ConfirmAction::Shutdown => self.send(Command::Shutdown),
            }
            close = true;
        }
        if close {
            self.confirm = None;
            self.confirm_opened_at = None;
        }
    }

    fn open_confirm(&mut self, action: ConfirmAction) {
        self.confirm = Some(action);
        self.confirm_opened_at = Some(std::time::Instant::now());
    }
}

#[derive(Copy, Clone)]
enum PathKind {
    Da,
    Preloader,
    Auth,
    OutputDir,
}

impl PathKind {
    fn dialog_title(self) -> &'static str {
        match self {
            PathKind::Da => "Select DA file",
            PathKind::Preloader => "Select Preloader file",
            PathKind::Auth => "Select Auth file",
            PathKind::OutputDir => "Select output folder",
        }
    }
}
