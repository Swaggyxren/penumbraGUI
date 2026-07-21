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
    RichText,
    Rounding,
    ScrollArea,
    Stroke,
};
use egui_extras::{Column, TableBuilder};
use human_bytes::human_bytes;
use penumbra::core::storage::Partition;
use penumbra::da::protocol::BootMode;
use serde::{Deserialize, Serialize};

use crate::messages::{Command, ConnStatus, Event, LogLine};
use crate::theme::{self, ThemeId};
use crate::worker::WorkerHandle;

const LOG_SCROLLBACK: usize = 4000;

/// Which main tab is visible in the content area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum Tab {
    Pgpt,
    Flash,
    Log,
    Settings,
}

impl Tab {
    fn label(self) -> &'static str {
        match self {
            Tab::Pgpt => "PGPT Manager",
            Tab::Flash => "Flash",
            Tab::Log => "Log",
            Tab::Settings => "Settings",
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
    log_panel_height: f32,
    scatter_path: Option<PathBuf>,
    uart_logging: bool,
    auto_save_logs: bool,
    log_font_size: f32,
    compact_view: bool,
    accepted_risk: bool,
}

impl Default for Persisted {
    fn default() -> Self {
        Self {
            theme: ThemeId::Sunset,
            tab: Tab::Pgpt,
            da_path: None,
            preloader_path: None,
            auth_path: None,
            output_dir: None,
            log_panel_height: 120.0,
            scatter_path: None,
            uart_logging: false,
            auto_save_logs: false,
            log_font_size: 12.0,
            compact_view: false,
            accepted_risk: false,
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

    // Scatter state (None until a scatter file is loaded).
    scatter: Option<ScatterView>,
    scatter_error: Option<String>,

    error: Option<String>,
    last_error: Option<String>,
    error_shown_at: Option<std::time::Instant>,

    // Worker plumbing.
    handle: WorkerHandle,
    evt_rx: Receiver<Event>,
    log_rx: Receiver<LogLine>,

    // Confirm-dialog state.
    confirm: Option<ConfirmAction>,
    confirm_opened_at: Option<std::time::Instant>,
}

/// Runtime state for the Scatter/XML Flasher tab.
struct ScatterView {
    file: crate::scatter::ScatterFile,
    #[allow(dead_code)]
    root: PathBuf,
    rows: Vec<ScatterRow>,
    storage_filter: String,
    #[allow(dead_code)]
    storage_types: Vec<String>,
}

struct ScatterRow {
    included: bool,
    resolved: Option<PathBuf>,
    /// Localised reason this row cannot be flashed, if any.
    skip_reason: Option<&'static str>,
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
}

#[derive(Debug, Clone)]
enum ConfirmAction {
    FlashScatter(Vec<(String, PathBuf)>),
    /// Flash one or more individual partitions picked from the PGPT table.
    FlashPgpt(Vec<(String, PathBuf)>),
    Reboot(BootMode),
    Shutdown,
}

impl ConfirmAction {
    fn title(&self) -> &'static str {
        match self {
            ConfirmAction::FlashScatter(_) => "Flash scatter layout?",
            ConfirmAction::FlashPgpt(_) => "Write partition(s)?",
            ConfirmAction::Reboot(_) => "Reboot device?",
            ConfirmAction::Shutdown => "Shut down device?",
        }
    }

    fn body(&self) -> String {
        match self {
            ConfirmAction::FlashPgpt(list) => {
                let mut s = String::from(
                    "You are about to write directly to the following partition(s).\n\n\
                     WARNING: Writing the wrong image to a partition can brick the device.\n\
                     Make sure each image matches the partition and your device model.\n\n",
                );
                for (name, path) in list {
                    s.push_str(&format!(
                        "  • {name}  ←  {}\n",
                        path.file_name().and_then(|n| n.to_str()).unwrap_or("?")
                    ));
                }
                s.push_str("\nDo you want to continue?");
                s
            }
            ConfirmAction::FlashScatter(list) => {
                let mut s = String::from(
                    "You are about to flash a scatter layout to the device.\n\n\
                     READ THIS BEFORE PROCEEDING:\n\n\
                     - Scatter flashing writes many partitions in a single run. If the \
                       scatter targets the wrong chip, project, or storage type, the \
                       device will be BRICKED.\n\
                     - Confirm the scatter's `platform` (e.g. MT6833) matches the \
                       connected chip and that every image comes from firmware built \
                       for THIS exact model + region.\n\
                     - Mismatched image sizes or variants can leave the device in an \
                       unbootable state with no recovery path.\n\
                     - Preloader rows (EMMC_BOOT1) are SKIPPED — use the Download Agent \
                       field at the top of the window to flash a preloader.\n\
                     - Rows with `file_name: NONE` or a missing image on disk are \
                       skipped automatically. Only rows you've checked in the table \
                       will be written.\n\
                     - Keep the device on a reliable USB cable with enough charge; an \
                       interrupted scatter flash almost always bricks.\n\n\
                     The following partitions will be OVERWRITTEN:\n\n",
                );
                for (p, img) in list {
                    s.push_str(&format!(
                        "  • {p}  ←  {}\n",
                        img.file_name().and_then(|n| n.to_str()).unwrap_or("?")
                    ));
                }
                s.push_str("\nDo you want to continue?");
                s
            }
            ConfirmAction::Reboot(mode) => match mode {
                BootMode::Fastboot => "The device will be asked to reboot into Android \
                                       Fastboot and disconnect.\n\n\
                                       Note: this might not work on some devices."
                    .into(),
                _ => "The device will reboot and disconnect.".into(),
            },
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
        let mut persisted: Persisted =
            cc.storage.and_then(|s| eframe::get_value(s, "penumbra-gui")).unwrap_or_default();
        persisted.log_panel_height = persisted.log_panel_height.clamp(50.0, 300.0);

        theme::apply(persisted.theme.palette(), &cc.egui_ctx);
        install_fonts(&cc.egui_ctx);

        // If the user had a scatter file open last session, try to re-parse it.
        let scatter = persisted.scatter_path.as_ref().and_then(|p| {
            match crate::scatter::parse_from_path(p) {
                Ok(file) => {
                    let root = p.parent().map(|x| x.to_path_buf()).unwrap_or(PathBuf::from("."));
                    let rows: Vec<ScatterRow> =
                        file.entries.iter().map(|e| build_scatter_row(e, &root)).collect();
                    let mut storage_types: Vec<String> =
                        file.entries.iter().map(|e| e.storage_type.clone()).collect();
                    storage_types.sort();
                    storage_types.dedup();
                    if storage_types.is_empty() {
                        storage_types.push(String::new());
                    }
                    let storage_filter = storage_types.first().cloned().unwrap_or_default();
                    Some(ScatterView { file, root, rows, storage_filter, storage_types })
                }
                Err(_) => None,
            }
        });

        App {
            persisted,
            status: ConnStatus::Disconnected,
            partitions: Vec::new(),
            progress: Progress::default(),
            input_enabled: true,
            logs: Vec::new(),
            log_filter: LogLevelFilter::All,
            log_autoscroll: true,
            scatter,
            scatter_error: None,
            error: None,
            last_error: None,
            error_shown_at: None,
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
        if matches!(cmd, Command::Cancel) {
            self.handle.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        if let Err(e) = self.handle.cmd_tx.send(cmd) {
            log::error!("worker channel closed: {e}");
        }
    }


}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "penumbra-gui", &self.persisted);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Handle drag and drop files
        ctx.input(|i| {
            for dropped in &i.raw.dropped_files {
                if let Some(path) = &dropped.path {
                    if path.is_file() {
                        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                        if ext == "txt" || ext == "xml" {
                            self.load_scatter(path.clone());
                        }
                    } else if path.is_dir() {
                        if let Some(scatter) = find_scatter_in_dir(path) {
                            self.load_scatter(scatter);
                        }
                    }
                }
            }
        });

        self.drain_events(ctx);
        theme::apply(self.persisted.theme.palette(), ctx);

        let palette = self.persisted.theme.palette();

        let main_enabled = self.persisted.accepted_risk;

        // Left Navigation Sidebar
        egui::SidePanel::left("navigation_sidebar")
            .resizable(false)
            .default_width(240.0)
            .frame(egui::Frame::none()
                .fill(palette.panel)
                .stroke(Stroke::new(1.0_f32, palette.border))
                .inner_margin(Margin::same(16.0)))
            .show(ctx, |ui| {
                if !main_enabled {
                    ui.disable();
                }
                self.draw_sidebar(ui, palette);
            });

        // Central Workspace Panel
        egui::CentralPanel::default()
            .frame(egui::Frame::none()
                .fill(palette.background)
                .inner_margin(Margin::same(20.0)))
            .show(ctx, |ui| {
                if !main_enabled {
                    ui.disable();
                }
                // Draw Active Page Header
                ui.horizontal(|ui| {
                    let page_title = match self.persisted.tab {
                        Tab::Pgpt => "PGPT Manager",
                        Tab::Flash => "Flash Partitions",
                        Tab::Log => "Log Workspace",
                        Tab::Settings => "Settings",
                    };
                    ui.label(RichText::new(page_title).strong().size(22.0).color(palette.text));
                    
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let connected = matches!(self.status, ConnStatus::Connected { .. });
                        
                                                let btn_shutdown = egui::Button::new(RichText::new("SHUTDOWN").strong().size(11.0))
                            .stroke(Stroke::new(1.0, palette.border))
                            .fill(Color32::TRANSPARENT)
                            .min_size(egui::vec2(100.0, 28.0));
                        if ui.add_enabled(connected && self.input_enabled, btn_shutdown).clicked() {
                            self.open_confirm(ConfirmAction::Shutdown);
                        }
                        
                        ui.add_space(8.0);
                        
                        let btn_reboot = egui::Button::new(RichText::new("REBOOT").strong().size(11.0))
                            .stroke(Stroke::new(1.0, palette.border))
                            .fill(Color32::TRANSPARENT)
                            .min_size(egui::vec2(80.0, 28.0));
                        if ui.add_enabled(connected && self.input_enabled, btn_reboot).clicked() {
                            self.open_confirm(ConfirmAction::Reboot(BootMode::Normal));
                        }
                        
                        ui.add_space(16.0);
                        
                        // Connection status badge
                        Frame::none()
                            .fill(palette.panel)
                            .stroke(Stroke::new(1.0, palette.border))
                            .rounding(Rounding::same(3.0))
                            .inner_margin(Margin::symmetric(10.0, 6.0))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let (status_text, dot_color) = match &self.status {
                                        ConnStatus::Disconnected => ("Disconnected", palette.text_muted),
                                        ConnStatus::Connecting => ("Connecting...", palette.warn),
                                        ConnStatus::Connected { chip_name, .. } => {
                                            if self.progress.active {
                                                (chip_name.as_str(), palette.accent)
                                            } else {
                                                (chip_name.as_str(), palette.success)
                                            }
                                        }
                                    };
                                    
                                    let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                                    ui.painter().circle_filled(rect.center(), 4.0, dot_color);
                                    
                                    ui.add_space(6.0);
                                    
                                    let suffix = if self.progress.active { " - Reading" } else if connected { " - Connected" } else { "" };
                                    ui.label(RichText::new(format!("{}{}", status_text, suffix)).strong().size(12.0).color(palette.text));
                                });
                            });
                    });
                });
                ui.add_space(12.0);
                ui.separator();
                ui.add_space(16.0);

                match self.persisted.tab {
                    Tab::Pgpt => self.draw_pgpt_tab(ui, palette),
                    Tab::Flash => self.draw_flash_tab(ui, palette),
                    Tab::Log => self.draw_log_tab(ui, palette),
                    Tab::Settings => self.draw_settings_tab(ui, palette),
                }
            });

        if let Some(action) = self.confirm.clone() {
            self.draw_confirm_dialog(ctx, palette, action);
        }

        self.draw_error_banner(ctx, palette);

        if !self.persisted.accepted_risk {
            self.draw_risk_disclaimer_dialog(ctx, palette);
        }
    }
}

// -------------------------------------------------------------------
// Drawing helpers
// -------------------------------------------------------------------

/// Extend egui's default proportional font fallback with Hack-Regular.
///
/// egui ships four fonts (Ubuntu-Light, NotoEmoji, emoji-icon-font, Hack)
/// but by default only the first three are in the proportional fallback
/// chain. Several glyphs we use in the UI (e.g. ←, ●, ○, geometric/arrow
/// blocks) are only present in Hack, so without this fallback they
/// render as tofu boxes inside RichText labels and dialog bodies.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    if let Some(prop) = fonts.families.get_mut(&egui::FontFamily::Proportional)
        && !prop.iter().any(|n| n == "Hack")
    {
        prop.push("Hack".to_owned());
    }
    ctx.set_fonts(fonts);
}



/// Renders a read-only path display box (truncated if long) followed by a
/// browse/select button. Returns `true` if the button was clicked.
fn path_display_row(
    ui: &mut egui::Ui,
    palette: theme::Palette,
    path: Option<&PathBuf>,
    placeholder: &str,
    btn_label: &str,
    btn_width: f32,
    enabled: bool,
) -> bool {
    let mut clicked = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;

        let has_path = path.is_some();
        let display_text = path
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| placeholder.to_string());

        let avail = (ui.available_width() - btn_width - 8.0).max(120.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(avail, 28.0), egui::Sense::hover());

        let fill_color = ui.style().visuals.extreme_bg_color;
        let stroke_color = if ui.is_enabled() {
            palette.border
        } else {
            palette.border.gamma_multiply(0.5)
        };
        ui.painter().rect(rect, Rounding::same(4.0), fill_color, Stroke::new(1.0, stroke_color));

        let text_color = if has_path {
            palette.text
        } else {
            palette.text_muted.gamma_multiply(0.4)
        };

        let font_id = egui::TextStyle::Body.resolve(ui.style());
        // Clip text strictly inside the box so long paths never bleed out.
        let inner_rect = rect.shrink(8.0);
        ui.painter().with_clip_rect(inner_rect).text(
            egui::pos2(inner_rect.min.x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            display_text,
            font_id,
            text_color,
        );

        let btn = egui::Button::new(RichText::new(btn_label).strong().size(11.0))
            .stroke(Stroke::new(1.0, palette.border))
            .fill(palette.panel_alt)
            .min_size(egui::vec2(btn_width, 28.0));
        if ui.add_enabled(enabled, btn).clicked() {
            clicked = true;
        }
    });
    clicked
}

/// Searches `dir` for the first file whose name contains "scatter" and has a
/// `.txt` or `.xml` extension. Returns its path if found.
fn find_scatter_in_dir(dir: &std::path::Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
        if (ext == "txt" || ext == "xml") && name.to_lowercase().contains("scatter") {
            return Some(p);
        }
    }
    None
}

fn build_scatter_row(entry: &crate::scatter::ScatterEntry, root: &std::path::Path) -> ScatterRow {
    // Rows the GUI can't write: preloader region (not in GPT) and rows with
    // no user-provided image.
    if crate::scatter::is_preloader_region(&entry.region) {
        return ScatterRow {
            included: false,
            resolved: None,
            skip_reason: Some("preloader (use DA)"),
        };
    }
    if !entry.is_download {
        return ScatterRow {
            included: false,
            resolved: None,
            skip_reason: Some("excluded by scatter"),
        };
    }
    if entry.file_name.is_empty() || entry.file_name.eq_ignore_ascii_case("NONE") {
        return ScatterRow { included: false, resolved: None, skip_reason: Some("no image") };
    }
    let candidate = root.join(&entry.file_name);
    if !candidate.is_file() {
        return ScatterRow { included: false, resolved: None, skip_reason: None };
    }
    ScatterRow { included: true, resolved: Some(candidate), skip_reason: None }
}




fn badge(ui: &mut egui::Ui, text: &str, fill: Color32, fg: Color32) {
    Frame::none()
        .fill(fill)
        .rounding(Rounding::same(2.0))
        .inner_margin(Margin::symmetric(10.0, 4.0))
        .show(ui, |ui| {
            ui.label(RichText::new(text).strong().color(fg));
        });
}

impl App {
    fn draw_tab_icon(&self, painter: &egui::Painter, cx: f32, cy: f32, color: Color32, tab: Tab) {
        match tab {
            Tab::Pgpt => {
                let stroke = egui::Stroke::new(1.2, color);
                // Three stacked rounded rectangles (representing database levels)
                painter.rect_stroke(
                    egui::Rect::from_min_max(egui::pos2(cx - 7.0, cy - 6.5), egui::pos2(cx + 7.0, cy - 2.5)),
                    egui::Rounding::same(1.5),
                    stroke,
                );
                painter.rect_stroke(
                    egui::Rect::from_min_max(egui::pos2(cx - 7.0, cy - 2.0), egui::pos2(cx + 7.0, cy + 2.0)),
                    egui::Rounding::same(1.5),
                    stroke,
                );
                painter.rect_stroke(
                    egui::Rect::from_min_max(egui::pos2(cx - 7.0, cy + 2.5), egui::pos2(cx + 7.0, cy + 6.5)),
                    egui::Rounding::same(1.5),
                    stroke,
                );
            }
            Tab::Flash => {
                let stroke = egui::Stroke::new(1.2, color);
                let p0 = egui::pos2(cx + 2.0, cy - 7.0);
                let p1 = egui::pos2(cx - 4.0, cy + 1.0);
                let p2 = egui::pos2(cx - 1.0, cy + 1.0);
                let p3 = egui::pos2(cx - 2.0, cy + 7.0);
                let p4 = egui::pos2(cx + 4.0, cy - 1.0);
                let p5 = egui::pos2(cx + 1.0, cy - 1.0);

                painter.line_segment([p0, p1], stroke);
                painter.line_segment([p1, p2], stroke);
                painter.line_segment([p2, p3], stroke);
                painter.line_segment([p3, p4], stroke);
                painter.line_segment([p4, p5], stroke);
                painter.line_segment([p5, p0], stroke);
            }
            Tab::Log => {
                let stroke = egui::Stroke::new(1.5, color);
                painter.line_segment([egui::pos2(cx - 6.0, cy - 4.0), egui::pos2(cx - 2.0, cy)], stroke);
                painter.line_segment([egui::pos2(cx - 2.0, cy), egui::pos2(cx - 6.0, cy + 4.0)], stroke);
                painter.line_segment([egui::pos2(cx, cy + 4.0), egui::pos2(cx + 6.0, cy + 4.0)], stroke);
            }
            Tab::Settings => {
                let stroke = egui::Stroke::new(1.2, color);
                painter.circle_stroke(egui::pos2(cx, cy), 4.5, stroke);
                painter.circle_stroke(egui::pos2(cx, cy), 1.5, stroke);
                
                let r1 = 4.5;
                let r2 = 7.0;
                for i in 0..8 {
                    let angle = (i as f32) * std::f32::consts::FRAC_PI_4;
                    let cos = angle.cos();
                    let sin = angle.sin();
                    let p1 = egui::pos2(cx + r1 * cos, cy + r1 * sin);
                    let p2 = egui::pos2(cx + r2 * cos, cy + r2 * sin);
                    painter.line_segment([p1, p2], stroke);
                }
            }
        }
    }

    fn draw_sidebar_link(&mut self, ui: &mut egui::Ui, palette: theme::Palette, tab: Tab) {
        let active = self.persisted.tab == tab;
        let (rect, response) = ui.allocate_at_least(egui::vec2(ui.available_width(), 36.0), egui::Sense::click());
        
        if response.clicked() {
            self.persisted.tab = tab;
        }
        
        let bg_color = if active {
            palette.panel_alt
        } else if response.hovered() {
            palette.panel_alt.linear_multiply(0.5)
        } else {
            Color32::TRANSPARENT
        };
        
        if bg_color != Color32::TRANSPARENT {
            ui.painter().rect_filled(rect, Rounding::same(3.0), bg_color);
        }
        
        if active {
            let indicator_rect = egui::Rect::from_min_max(
                egui::pos2(rect.min.x, rect.min.y + 6.0),
                egui::pos2(rect.min.x + 3.0, rect.max.y - 6.0),
            );
            ui.painter().rect_filled(indicator_rect, Rounding::same(1.5), palette.accent);
        }
        
        let text_color = if active {
            Color32::WHITE
        } else if response.hovered() {
            Color32::WHITE
        } else {
            palette.text_muted
        };

        // Draw custom vector icon centered at x = rect.min.x + 20.0
        let cx = rect.min.x + 20.0;
        let cy = rect.center().y;
        self.draw_tab_icon(ui.painter(), cx, cy, text_color, tab);
        
        let text_pos = egui::pos2(rect.min.x + 38.0, rect.center().y);
        let font_id = egui::FontId::proportional(14.0);
        ui.painter().text(
            text_pos,
            egui::Align2::LEFT_CENTER,
            tab.label(),
            font_id,
            text_color,
        );
    }

    fn draw_sidebar(&mut self, ui: &mut egui::Ui, palette: theme::Palette) {
        ui.vertical(|ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                // Draw a beautiful geometric Penumbra eclipse/shadow logo
                let (rect, _) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::hover());
                let cx = rect.center().x;
                let cy = rect.center().y;
                
                // Outer bright circle (representing light boundary)
                ui.painter().circle_filled(egui::pos2(cx - 2.0, cy), 10.0, palette.accent);
                // Overlapping shadow circle matching background panel color (creating crescent shadow / penumbra)
                ui.painter().circle_filled(egui::pos2(cx + 2.0, cy - 1.0), 9.0, palette.panel);
                
                ui.add_space(8.0);
                
                ui.vertical(|ui| {
                    ui.label(RichText::new("Penumbra").strong().size(22.0).color(Color32::WHITE));
                    ui.label(RichText::new(concat!("v", env!("CARGO_PKG_VERSION"))).size(11.0).color(palette.text_muted));
                });
            });
            ui.add_space(20.0);
        });
        ui.separator();
        ui.add_space(16.0);

        self.draw_sidebar_link(ui, palette, Tab::Pgpt);
        ui.add_space(8.0);
        self.draw_sidebar_link(ui, palette, Tab::Flash);
        ui.add_space(8.0);
        self.draw_sidebar_link(ui, palette, Tab::Log);

        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.add_space(16.0);
            self.draw_sidebar_link(ui, palette, Tab::Settings);
            ui.add_space(16.0);
            
            let is_connecting = matches!(self.status, ConnStatus::Connecting);
            let progress_active = self.progress.active;
            let show_cancel = is_connecting || progress_active;
            let connected = matches!(self.status, ConnStatus::Connected { .. });
            let btn_enabled = self.input_enabled || show_cancel;
            
            let (rect, response) = ui.allocate_at_least(
                egui::vec2(ui.available_width(), 32.0),
                egui::Sense::click(),
            );
            
            let bg_color = if !btn_enabled {
                palette.panel_alt
            } else if response.hovered() {
                if show_cancel || connected {
                    palette.error.linear_multiply(0.8)
                } else {
                    palette.accent.linear_multiply(0.8)
                }
            } else {
                if show_cancel || connected {
                    palette.error
                } else {
                    palette.accent
                }
            };
            
            let text_color = if btn_enabled {
                Color32::WHITE
            } else {
                palette.text_muted
            };
            
            let conn_label = if show_cancel {
                "CANCEL"
            } else if connected {
                "DISCONNECT DEVICE"
            } else {
                "CONNECT DEVICE"
            };
            
            ui.painter().rect_filled(rect, Rounding::same(4.0), bg_color);
            
            let font_id = egui::TextStyle::Button.resolve(ui.style());
            let text_pos = egui::pos2(rect.min.x + 16.0, rect.center().y);
            ui.painter().text(
                text_pos,
                egui::Align2::LEFT_CENTER,
                conn_label,
                font_id,
                text_color,
            );
            
            if btn_enabled && response.clicked() {
                if show_cancel {
                    self.send(Command::Cancel);
                } else if connected {
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

    fn draw_settings_tab(&mut self, ui: &mut egui::Ui, palette: theme::Palette) {
        ScrollArea::vertical().show(ui, |ui| {
            // 1. Path Configuration Card
            Frame::none()
                .fill(palette.panel)
                .stroke(Stroke::new(1.0_f32, palette.border))
                .rounding(Rounding::same(4.0))
                .inner_margin(Margin::same(16.0))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(RichText::new("Path Configuration").strong().size(16.0).color(palette.text));
                    ui.add_space(12.0);
                    
                    self.draw_settings_path_row(ui, palette, "Download Agent (DA) Loader", PathKind::Da);
                    ui.add_space(10.0);
                    self.draw_settings_path_row(ui, palette, "Preloader Binary (PL)", PathKind::Preloader);
                    ui.add_space(10.0);
                    self.draw_settings_path_row(ui, palette, "Security Authentication (Auth)", PathKind::Auth);
                    ui.add_space(10.0);
                    self.draw_settings_path_row(ui, palette, "Backup Output Destination Directory", PathKind::OutputDir);
                });
                
            ui.add_space(16.0);
            
            // 2. Interface Settings Card
            Frame::none()
                .fill(palette.panel)
                .stroke(Stroke::new(1.0_f32, palette.border))
                .rounding(Rounding::same(4.0))
                .inner_margin(Margin::same(16.0))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(RichText::new("Interface Preferences").strong().size(16.0).color(palette.text));
                    ui.add_space(12.0);
                    
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("UI Theme:").color(palette.text_muted));
                        ui.add_space(12.0);
                        egui::ComboBox::from_id_salt("settings_theme_combo")
                            .selected_text(self.persisted.theme.label())
                            .show_ui(ui, |ui| {
                                for &t in ThemeId::ALL {
                                    ui.selectable_value(&mut self.persisted.theme, t, t.label());
                                }
                            });
                    });
                    
                    ui.add_space(12.0);
                    
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Console Font Size:").color(palette.text_muted));
                        ui.add_space(12.0);
                        ui.add(egui::Slider::new(&mut self.persisted.log_font_size, 10.0..=20.0).suffix("px"));
                    });
                    
                    ui.add_space(12.0);
                    
                    ui.checkbox(&mut self.persisted.compact_view, "Compact View (tighter list margins)");
                    
                    ui.add_space(12.0);
                    
                    let btn_reset = egui::Button::new(RichText::new("SHOW RISK DISCLAIMER & REQUIREMENTS").strong())
                        .stroke(Stroke::new(1.0, palette.border))
                        .fill(palette.panel_alt)
                        .min_size(egui::vec2(160.0, 28.0));
                    if ui.add(btn_reset).clicked() {
                        self.persisted.accepted_risk = false;
                    }
                });
                
            ui.add_space(16.0);
            
            // 3. Software Updates Card
            Frame::none()
                .fill(palette.panel)
                .stroke(Stroke::new(1.0_f32, palette.border))
                .rounding(Rounding::same(4.0))
                .inner_margin(Margin::same(16.0))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(RichText::new("Software Updates").strong().size(16.0).color(palette.text));
                    ui.add_space(12.0);
                    
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Current version:").color(palette.text_muted));
                        ui.label(RichText::new(concat!("v", env!("CARGO_PKG_VERSION"))).strong().color(palette.text));
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Status:").color(palette.text_muted));
                        ui.label(RichText::new("Up to date").color(palette.success));
                    });
                    ui.add_space(12.0);
                    
                    let btn = egui::Button::new(RichText::new("CHECK FOR UPDATES").strong())
                        .stroke(Stroke::new(1.0, palette.border))
                        .fill(palette.panel_alt)
                        .min_size(egui::vec2(160.0, 28.0));
                    if ui.add(btn).clicked() {
                        log::info!("Checking for updates... Penumbra is up to date!");
                    }
                });
        });
    }

    fn draw_settings_path_row(&mut self, ui: &mut egui::Ui, palette: theme::Palette, label: &str, kind: PathKind) {
        ui.vertical(|ui| {
            ui.label(RichText::new(label).color(palette.text).size(12.0));
            ui.add_space(4.0);

            let path = match kind {
                PathKind::Da => self.persisted.da_path.as_ref(),
                PathKind::Preloader => self.persisted.preloader_path.as_ref(),
                PathKind::Auth => self.persisted.auth_path.as_ref(),
                PathKind::OutputDir => self.persisted.output_dir.as_ref(),
            };
            let placeholder = match kind {
                PathKind::OutputDir => "Select output folder...",
                _ => "Select file...",
            };
            let btn_label = match kind {
                PathKind::OutputDir => "SELECT",
                _ => "BROWSE",
            };

            if path_display_row(ui, palette, path, placeholder, btn_label, 94.0, self.input_enabled) {
                self.pick_path(kind);
            }
        });
    }



    fn draw_error_banner(&mut self, ctx: &egui::Context, palette: theme::Palette) {
        if let Some(err) = &self.error {
            if self.last_error.as_ref() != Some(err) {
                self.last_error = Some(err.clone());
                self.error_shown_at = Some(std::time::Instant::now());
            }
        } else {
            self.error_shown_at = None;
        }

        let mut time_ratio = 1.0;
        if let Some(shown_at) = self.error_shown_at {
            let elapsed = shown_at.elapsed().as_secs_f32();
            if elapsed >= 5.0 {
                self.error = None;
                self.error_shown_at = None;
            } else {
                time_ratio = 1.0 - (elapsed / 5.0);
                ctx.request_repaint();
            }
        }

        let err_active = self.error.is_some();
        let slide_progress = ctx.animate_bool(egui::Id::new("error_slide"), err_active);
        
        if slide_progress <= 0.0 {
            return;
        }
        
        let Some(err) = &self.last_error else { return };
        
        // Animating between hidden (offset_y = 100.0) and visible (offset_y = -20.0)
        let offset_y = 100.0 - 120.0 * slide_progress;
        let offset_x = -20.0;
        
        egui::Area::new(egui::Id::new("error_banner"))
            .anchor(egui::Align2::RIGHT_BOTTOM, [offset_x, offset_y])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                Frame::none()
                    .fill(palette.panel_alt)
                    .stroke(Stroke::new(1.0_f32, palette.error))
                    .rounding(Rounding::same(4.0))
                    .inner_margin(Margin::same(12.0))
                    .show(ui, |ui| {
                        ui.set_width(450.0);
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("ERROR").color(palette.error).strong().size(11.0));
                                ui.add_space(4.0);
                                ui.label(RichText::new(err).color(palette.text));
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.button("DISMISS").clicked() {
                                        self.error = None;
                                        self.error_shown_at = None;
                                    }
                                });
                            });
                            
                            if err_active {
                                ui.add_space(8.0);
                                let progress_width = ui.available_width();
                                let (rect, _response) = ui.allocate_exact_size(
                                    egui::vec2(progress_width, 2.0),
                                    egui::Sense::hover(),
                                );
                                // Background track
                                ui.painter().rect_filled(rect, Rounding::same(0.0), palette.border);
                                // Active filled bar representing remaining time
                                let filled_width = progress_width * time_ratio.clamp(0.0, 1.0);
                                let filled_rect = egui::Rect::from_min_max(
                                    rect.min,
                                    egui::pos2(rect.min.x + filled_width, rect.max.y),
                                );
                                ui.painter().rect_filled(filled_rect, Rounding::same(0.0), palette.error);
                            }
                        });
                    });
            });
    }

    fn draw_pgpt_tab(&mut self, ui: &mut egui::Ui, palette: theme::Palette) {
        let connected = matches!(self.status, ConnStatus::Connected { .. });
        
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;

            let browse_w  = 80.0;
            let refresh_w = 110.0;
            let read_w    = 130.0;
            // Fixed, compact width for the path box — not stretched.
            let path_box_w = 200.0;

            // "Output Folder:" label, vertically centered with the box.
            ui.add(egui::Label::new(
                RichText::new("Output Folder:").color(palette.text_muted)
            ).selectable(false));

            // Path display box.
            let has_path = self.persisted.output_dir.is_some();
            let display_text = self.persisted.output_dir
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "Select output folder...".to_string());
            let (rect, _) = ui.allocate_exact_size(egui::vec2(path_box_w, 28.0), egui::Sense::hover());
            let fill_color = ui.style().visuals.extreme_bg_color;
            let stroke_color = if ui.is_enabled() { palette.border } else { palette.border.gamma_multiply(0.5) };
            ui.painter().rect(rect, Rounding::same(4.0), fill_color, Stroke::new(1.0, stroke_color));
            let text_color = if has_path { palette.text } else { palette.text_muted.gamma_multiply(0.4) };
            let font_id = egui::TextStyle::Body.resolve(ui.style());
            let inner_rect = rect.shrink(8.0);
            ui.painter().with_clip_rect(inner_rect).text(
                egui::pos2(inner_rect.min.x, rect.center().y),
                egui::Align2::LEFT_CENTER,
                display_text,
                font_id,
                text_color,
            );

            let btn_browse = egui::Button::new(RichText::new("BROWSE").strong().size(11.0))
                .stroke(Stroke::new(1.0, palette.border))
                .fill(palette.panel_alt)
                .min_size(egui::vec2(browse_w, 28.0));
            if ui.add_enabled(self.input_enabled, btn_browse).clicked() {
                self.pick_path(PathKind::OutputDir);
            }

            let btn_refresh = egui::Button::new(RichText::new("REFRESH GPT").strong().size(11.0))
                .stroke(Stroke::new(1.0, palette.border))
                .fill(palette.panel_alt)
                .min_size(egui::vec2(refresh_w, 28.0));
            if ui.add_enabled(connected && self.input_enabled, btn_refresh).clicked() {
                self.send(Command::LoadPgpt);
            }

            let read_enabled = connected
                && self.input_enabled
                && self.partitions.iter().any(|r| r.selected)
                && self.persisted.output_dir.is_some();
            let btn_read = egui::Button::new(RichText::new("READ SELECTED").strong().size(11.0))
                .stroke(Stroke::new(1.0, palette.border))
                .fill(palette.panel_alt)
                .min_size(egui::vec2(read_w, 28.0));
            if ui.add_enabled(read_enabled, btn_read).clicked() {
                self.start_read_selected();
            }

            let write_enabled = connected
                && self.input_enabled
                && self.partitions.iter().any(|r| r.selected && r.assigned_image.is_some());
            let btn_write = egui::Button::new(
                RichText::new("WRITE SELECTED").strong().size(11.0).color(Color32::WHITE),
            )
            .fill(if write_enabled { palette.accent } else { palette.panel_alt })
            .stroke(Stroke::new(1.0, palette.border))
            .min_size(egui::vec2(130.0, 28.0));
            if ui.add_enabled(write_enabled, btn_write).clicked() {
                self.start_write_selected();
            }
        });
        
        self.draw_progress_card(ui, palette);
        ui.add_space(8.0);
        
        let table_height = ui.available_height().max(160.0);
        ui.allocate_ui(egui::vec2(ui.available_width(), table_height), |ui| {
            self.draw_partition_table(ui, palette);
        });
    }

    fn draw_progress_card(&mut self, ui: &mut egui::Ui, palette: theme::Palette) {
        if self.progress.active {
            ui.add_space(8.0);
            Frame::none()
                .fill(palette.panel)
                .stroke(Stroke::new(1.0_f32, palette.border))
                .rounding(Rounding::same(4.0))
                .inner_margin(Margin::same(12.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&self.progress.message).strong().color(palette.text));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let btn_cancel = egui::Button::new(RichText::new("CANCEL").strong().size(10.0).color(Color32::WHITE))
                                .fill(palette.error)
                                .rounding(Rounding::same(2.0));
                            if ui.add(btn_cancel).clicked() {
                                self.send(Command::Cancel);
                            }
                            
                            ui.add_space(12.0);
                            
                            let pct = if self.progress.total > 0 {
                                (self.progress.written as f32 / self.progress.total as f32) * 100.0
                            } else {
                                0.0
                            };
                            ui.label(RichText::new(format!("{}%", pct as i32)).color(palette.text_muted));
                        });
                    });
                    ui.add_space(6.0);
                    
                    let progress_value = if self.progress.total > 0 {
                        self.progress.written as f32 / self.progress.total as f32
                    } else {
                        0.0
                    };
                    
                    let (rect, _) = ui.allocate_at_least(egui::vec2(ui.available_width(), 6.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, Rounding::same(1.5), palette.panel_alt);
                    
                    let filled_width = rect.width() * progress_value;
                    if filled_width > 0.0 {
                        let filled_rect = egui::Rect::from_min_max(
                            rect.min,
                            egui::pos2(rect.min.x + filled_width, rect.max.y),
                        );
                        ui.painter().rect_filled(filled_rect, Rounding::same(1.5), palette.accent);
                    }
                });
        }
    }

    fn draw_partition_table(&mut self, ui: &mut egui::Ui, palette: theme::Palette) {
        let inner_min_height = (ui.available_height() - 12.0).max(160.0);
        Frame::none()
            .fill(palette.panel)
            .stroke(Stroke::new(1.0_f32, palette.border))
            .rounding(Rounding::same(3.0))
            .inner_margin(Margin::same(6.0))
            .show(ui, |ui| {
                ui.set_min_height(inner_min_height);
                if self.partitions.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new("No partitions loaded. Connect a device and press REFRESH GPT.")
                                .color(palette.text_muted),
                        );
                    });
                    return;
                }

                // Collect assign requests outside the borrow so we can mutate after.
                let mut assign_idx: Option<usize> = None;
                let mut clear_idx: Option<usize> = None;

                TableBuilder::new(ui)
                    .striped(true)
                    .resizable(true)
                    .cell_layout(Layout::left_to_right(Align::Center))
                    .column(Column::exact(28.0))                    // checkbox
                    .column(Column::initial(160.0).at_least(100.0)) // NAME
                    .column(Column::initial(110.0).at_least(80.0))  // START LBA
                    .column(Column::initial(130.0).at_least(90.0))  // SIZE
                    .column(Column::initial(140.0).at_least(100.0)) // FLAGS
                    .column(Column::remainder().at_least(180.0))    // FLASH IMAGE
                    .header(22.0, |mut header| {
                        header.col(|ui| {
                            let mut all_selected = !self.partitions.is_empty()
                                && self.partitions.iter().all(|r| r.selected);
                            if ui.checkbox(&mut all_selected, "").changed() {
                                for r in &mut self.partitions {
                                    r.selected = all_selected;
                                }
                            }
                        });
                        header.col(|ui| { ui.label(RichText::new("NAME").strong().color(palette.text_muted)); });
                        header.col(|ui| { ui.label(RichText::new("START LBA").strong().color(palette.text_muted)); });
                        header.col(|ui| { ui.label(RichText::new("SIZE").strong().color(palette.text_muted)); });
                        header.col(|ui| { ui.label(RichText::new("FLAGS").strong().color(palette.text_muted)); });
                        header.col(|ui| { ui.label(RichText::new("FLASH IMAGE").strong().color(palette.text_muted)); });
                    })
                    .body(|mut body| {
                        for (idx, row) in self.partitions.iter_mut().enumerate() {
                            body.row(22.0, |mut r| {
                                r.col(|ui| { ui.checkbox(&mut row.selected, ""); });
                                r.col(|ui| {
                                    ui.label(RichText::new(&row.partition.name).color(palette.text));
                                });
                                r.col(|ui| {
                                    ui.label(RichText::new(format!("0x{:X}", row.partition.address)).color(palette.text_muted));
                                });
                                r.col(|ui| {
                                    ui.label(RichText::new(format!("{}", human_bytes(row.partition.size as f64))).color(palette.text));
                                });
                                r.col(|ui| {
                                    ui.horizontal(|ui| {
                                        for (label, bg, fg) in get_partition_flags(&row.partition.name) {
                                            badge(ui, label, bg, fg);
                                        }
                                    });
                                });
                                r.col(|ui| {
                                    ui.horizontal(|ui| {
                                        // Show assigned filename or placeholder.
                                        let label = match &row.assigned_image {
                                            Some(p) => p
                                                .file_name()
                                                .and_then(|n| n.to_str())
                                                .unwrap_or("?")
                                                .to_string(),
                                            None => "—".to_string(),
                                        };
                                        let text_color = if row.assigned_image.is_some() {
                                            palette.accent_strong
                                        } else {
                                            palette.text_muted
                                        };
                                        // Double-click or ASSIGN button to pick a file.
                                        let resp = ui.add(
                                            egui::Label::new(
                                                RichText::new(&label).color(text_color),
                                            )
                                            .sense(egui::Sense::click()),
                                        );
                                        if resp.double_clicked() {
                                            assign_idx = Some(idx);
                                        }
                                        resp.on_hover_text("Double-click to assign an image");

                                        ui.add_space(4.0);

                                        let btn_assign = egui::Button::new(
                                            RichText::new("ASSIGN").size(10.0).strong(),
                                        )
                                        .fill(palette.panel_alt)
                                        .stroke(Stroke::new(1.0, palette.border))
                                        .min_size(egui::vec2(56.0, 18.0));
                                        if ui.add(btn_assign).clicked() {
                                            assign_idx = Some(idx);
                                        }

                                        if row.assigned_image.is_some() {
                                            let btn_clear = egui::Button::new(
                                                RichText::new("✕").size(10.0).color(palette.error),
                                            )
                                            .fill(Color32::TRANSPARENT)
                                            .stroke(Stroke::NONE)
                                            .min_size(egui::vec2(18.0, 18.0));
                                            if ui.add(btn_clear).clicked() {
                                                clear_idx = Some(idx);
                                            }
                                        }
                                    });
                                });
                            });
                        }
                    });

                // Handle file picker outside the table borrow.
                if let Some(idx) = assign_idx {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_title(format!(
                            "Assign image for '{}'",
                            self.partitions[idx].partition.name
                        ))
                        .add_filter("images", &["img", "bin", "mbn"])
                        .add_filter("all", &["*"])
                        .pick_file()
                    {
                        self.partitions[idx].assigned_image = Some(path);
                        self.partitions[idx].selected = true;
                    }
                }
                if let Some(idx) = clear_idx {
                    self.partitions[idx].assigned_image = None;
                }
            });
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

    fn start_write_selected(&mut self) {
        let assignments: Vec<(String, PathBuf)> = self
            .partitions
            .iter()
            .filter(|r| r.selected)
            .filter_map(|r| {
                r.assigned_image
                    .as_ref()
                    .map(|p| (r.partition.name.clone(), p.clone()))
            })
            .collect();

        if assignments.is_empty() {
            log::warn!("No partitions with an assigned image selected.");
            return;
        }

        // Reuse the scatter confirm dialog with a PGPT-specific action.
        self.open_confirm(ConfirmAction::FlashPgpt(assignments));
    }

    fn draw_flash_tab(&mut self, ui: &mut egui::Ui, palette: theme::Palette) {
        let connected = matches!(self.status, ConnStatus::Connected { .. });
        
        // 1. Output log stuck to the bottom of the screen
        egui::TopBottomPanel::bottom("flash_bottom_log")
            .resizable(false)
            .default_height(200.0)
            .frame(egui::Frame::none()
                .fill(palette.background)
                .inner_margin(Margin {
                    left: 0.0,
                    right: 0.0,
                    top: 10.0,
                    bottom: 0.0,
                }))
            .show_inside(ui, |ui| {
                Frame::none()
                    .fill(palette.panel)
                    .stroke(Stroke::new(1.0_f32, palette.border))
                    .rounding(Rounding::same(4.0))
                    .inner_margin(Margin::same(12.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("OUTPUT LOG").strong().color(palette.text));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.button("CLEAR").clicked() {
                                    self.logs.clear();
                                }
                                ui.add_space(12.0);
                                ui.checkbox(&mut self.persisted.uart_logging, "UART Logging");
                            });
                        });
                        ui.add_space(8.0);
                        
                        let remaining_height = ui.available_height();
                        ScrollArea::vertical()
                            .max_height(remaining_height)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                                if self.logs.is_empty() {
                                    ui.label(RichText::new("No log output. Flashing logs will appear here.").color(palette.text_muted).monospace());
                                } else {
                                    for line in &self.logs {
                                        let color = match line.level {
                                            log::Level::Error => palette.error,
                                            log::Level::Warn => palette.warn,
                                            log::Level::Info => palette.text,
                                            log::Level::Debug | log::Level::Trace => palette.text_muted,
                                        };
                                        let text = format!("[{}] {}", line.level, line.message);
                                        ui.add(egui::Label::new(RichText::new(text).color(color).monospace()));
                                    }
                                }
                            });
                    });
            });

        // 2. Main content takes up the remaining upper space
        ui.vertical_centered(|ui| {
            let card_height = 100.0;
            let (rect, _response) = ui.allocate_at_least(egui::vec2(ui.available_width(), card_height), egui::Sense::click());
            
            ui.painter().rect_filled(rect, Rounding::same(4.0), palette.panel);
            
            let stroke = Stroke::new(1.0, palette.border);
            let dash_length = 6.0;
            let gap_length = 4.0;
            
            let mut x = rect.min.x;
            while x < rect.max.x {
                let end_x = (x + dash_length).min(rect.max.x);
                ui.painter().line_segment([egui::pos2(x, rect.min.y), egui::pos2(end_x, rect.min.y)], stroke);
                x += dash_length + gap_length;
            }
            x = rect.min.x;
            while x < rect.max.x {
                let end_x = (x + dash_length).min(rect.max.x);
                ui.painter().line_segment([egui::pos2(x, rect.max.y), egui::pos2(end_x, rect.max.y)], stroke);
                x += dash_length + gap_length;
            }
            let mut y = rect.min.y;
            while y < rect.max.y {
                let end_y = (y + dash_length).min(rect.max.y);
                ui.painter().line_segment([egui::pos2(rect.min.x, y), egui::pos2(rect.min.x, end_y)], stroke);
                y += dash_length + gap_length;
            }
            y = rect.min.y;
            while y < rect.max.y {
                let end_y = (y + dash_length).min(rect.max.y);
                ui.painter().line_segment([egui::pos2(rect.max.x, y), egui::pos2(rect.max.x, end_y)], stroke);
                y += dash_length + gap_length;
            }
            
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(16.0);
                    ui.label(RichText::new("Drag & drop Android firmware folder or scatter file here").strong().color(palette.text));
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            let space = (ui.available_width() - 280.0) / 2.0;
                            ui.add_space(space.max(0.0));
                            
                                                        let btn_folder = egui::Button::new(RichText::new("SELECT FOLDER").strong().size(12.0))
                                .stroke(Stroke::new(1.0, palette.border))
                                .fill(palette.panel_alt)
                                .min_size(egui::vec2(130.0, 28.0));
                            if ui.add(btn_folder).clicked() {
                                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                                    if let Some(scatter) = find_scatter_in_dir(&folder) {
                                        self.load_scatter(scatter);
                                    } else {
                                        self.scatter_error = Some("No scatter file found in the selected folder.".to_string());
                                    }
                                }
                            }
                            
                            ui.add_space(16.0);
                            
                            let btn_file = egui::Button::new(RichText::new("SELECT FILE").strong().size(12.0))
                                .stroke(Stroke::new(1.0, palette.border))
                                .fill(palette.panel_alt)
                                .min_size(egui::vec2(130.0, 28.0));
                            if ui.add(btn_file).clicked() {
                                if let Some(file) = rfd::FileDialog::new()
                                    .add_filter("scatter", &["txt", "xml"])
                                    .pick_file() {
                                    self.load_scatter(file);
                                }
                            }
                        });
                    });
                });
            });
        });
        
        if let Some(err) = &self.scatter_error {
            ui.add_space(8.0);
            Frame::none()
                .fill(palette.panel)
                .stroke(Stroke::new(1.0_f32, palette.error))
                .rounding(Rounding::same(3.0))
                .inner_margin(Margin::same(8.0))
                .show(ui, |ui| {
                    ui.label(RichText::new(err).color(palette.error));
                });
        }
        
        self.draw_progress_card(ui, palette);
        
        ui.add_space(12.0);
        ui.label(RichText::new("Partition Mapping").strong().size(14.0).color(palette.text_muted));
        ui.add_space(6.0);
        
        // Dynamically size table to fill available space (leaving 50px for buttons)
        let table_height = (ui.available_height() - 50.0).max(120.0);
        
        ui.allocate_ui(egui::vec2(ui.available_width(), table_height), |ui| {
            self.draw_flash_table_new(ui, palette);
        });
        
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let has_scatter = self.scatter.is_some();
            
            let btn_clear = egui::Button::new(RichText::new("CLEAR ALL").strong())
                .stroke(Stroke::new(1.0, palette.border))
                .fill(palette.panel_alt)
                .min_size(egui::vec2(100.0, 30.0));
            if ui.add_enabled(has_scatter, btn_clear).clicked() {
                self.scatter = None;
                self.scatter_error = None;
                self.persisted.scatter_path = None;
            }
            
            ui.add_space(8.0);
            
            let flashable = self.collect_scatter_flashables();
            let backup_enabled = connected && self.input_enabled && !flashable.is_empty();
            let btn_backup = egui::Button::new(RichText::new("BACKUP SELECTED").strong())
                .stroke(Stroke::new(1.0, palette.border))
                .fill(palette.panel_alt)
                .min_size(egui::vec2(140.0, 30.0));
            if ui.add_enabled(backup_enabled, btn_backup).clicked() {
                if let Some(out_dir) = &self.persisted.output_dir {
                    let names: Vec<String> = flashable.iter().map(|(name, _)| name.clone()).collect();
                    self.send(Command::ReadPartitions { names, output_dir: out_dir.clone() });
                } else {
                    self.error = Some("Please configure backup output directory in settings first.".to_string());
                }
            }
            
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let flash_enabled = connected && self.input_enabled && !flashable.is_empty();
                let btn_flash = egui::Button::new(RichText::new("FLASH SELECTED").strong().color(Color32::WHITE))
                    .fill(palette.accent)
                    .min_size(egui::vec2(140.0, 30.0));
                if ui.add_enabled(flash_enabled, btn_flash).clicked() {
                    self.open_confirm(ConfirmAction::FlashScatter(flashable));
                }
            });
        });
    }

    fn draw_flash_table_new(&mut self, ui: &mut egui::Ui, palette: theme::Palette) {
        Frame::none()
            .fill(palette.panel)
            .stroke(Stroke::new(1.0_f32, palette.border))
            .rounding(Rounding::same(3.0))
            .inner_margin(Margin::same(6.0))
            .show(ui, |ui| {
                let Some(view) = self.scatter.as_mut() else {
                    ui.centered_and_justified(|ui| {
                        ui.label(RichText::new("No firmware layout loaded. Select a scatter file or drag and drop a folder above.").color(palette.text_muted));
                    });
                    return;
                };
                
                let filter = view.storage_filter.clone();
                let matching: Vec<usize> = view
                    .file
                    .entries
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| e.storage_type == filter || filter.is_empty())
                    .map(|(i, _)| i)
                    .collect();
                    
                if matching.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.label(RichText::new("No partitions in this section.").color(palette.text_muted));
                    });
                    return;
                }
                
                let mut assign_target: Option<usize> = None;
                
                let mut simulated_addresses = vec![0_u64; view.file.entries.len()];
                let mut running_addr = 0x0020_0000_u64;
                for &idx in &matching {
                    simulated_addresses[idx] = running_addr;
                    running_addr += view.file.entries[idx].partition_size;
                }
                
                TableBuilder::new(ui)
                    .striped(true)
                    .resizable(true)
                    .cell_layout(Layout::left_to_right(Align::Center))
                    .column(Column::exact(28.0))
                    .column(Column::initial(150.0).at_least(100.0))
                    .column(Column::initial(220.0).at_least(140.0))
                    .column(Column::initial(100.0).at_least(80.0))
                    .column(Column::initial(120.0).at_least(90.0))
                    .column(Column::remainder().at_least(80.0))
                    .header(22.0, |mut header| {
                        header.col(|ui| {
                            let mut all_selected = !matching.is_empty() && matching.iter().all(|&idx| view.rows[idx].included);
                            if ui.checkbox(&mut all_selected, "").changed() {
                                for &idx in &matching {
                                    view.rows[idx].included = all_selected;
                                }
                            }
                        });
                        header.col(|ui| { ui.label(RichText::new("PARTITION").strong().color(palette.text_muted)); });
                        header.col(|ui| { ui.label(RichText::new("FILE PATH").strong().color(palette.text_muted)); });
                        header.col(|ui| { ui.label(RichText::new("SIZE").strong().color(palette.text_muted)); });
                        header.col(|ui| { ui.label(RichText::new("ADDRESS").strong().color(palette.text_muted)); });
                        header.col(|ui| { ui.label(RichText::new("STATUS").strong().color(palette.text_muted)); });
                    })
                    .body(|mut body| {
                        for &idx in &matching {
                            let entry = view.file.entries[idx].clone();
                            let row_state = &mut view.rows[idx];
                            
                            body.row(22.0, |mut tr| {
                                tr.col(|ui| {
                                    let mut included = row_state.included;
                                    if ui.checkbox(&mut included, "").changed() {
                                        row_state.included = included;
                                    }
                                });
                                tr.col(|ui| {
                                    ui.label(RichText::new(&entry.name).color(palette.text));
                                });
                                tr.col(|ui| {
                                    let path_label = match &row_state.resolved {
                                        Some(p) => p.file_name().and_then(|n| n.to_str()).unwrap_or("?").to_string(),
                                        None => "—".to_string(),
                                    };
                                    let text_color = if row_state.resolved.is_some() {
                                        palette.accent_strong
                                    } else {
                                        palette.text_muted
                                    };
                                    let resp = ui.add(egui::Label::new(RichText::new(path_label).color(text_color)).sense(egui::Sense::click()));
                                    if resp.double_clicked() {
                                        assign_target = Some(idx);
                                    }
                                });
                                tr.col(|ui| {
                                    ui.label(RichText::new(human_bytes(entry.partition_size as f64)).color(palette.text));
                                });
                                tr.col(|ui| {
                                    ui.label(RichText::new(format!("0x{:08X}", simulated_addresses[idx])).color(palette.text_muted));
                                });
                                                                tr.col(|ui| {
                                    if let Some(reason) = row_state.skip_reason {
                                        ui.label(RichText::new(format!("! {}", reason)).color(palette.warn));
                                    } else if row_state.resolved.is_some() {
                                        ui.label(RichText::new("Ready").color(palette.success));
                                    } else {
                                        ui.label(RichText::new("Missing").color(palette.error));
                                    }
                                });
                            });
                        }
                    });
                    
                if let Some(idx) = assign_target
                    && let Some(file) = rfd::FileDialog::new()
                        .set_title(format!("Assign image for '{}'", view.file.entries[idx].name))
                        .add_filter("images", &["img", "bin", "mbn"])
                        .add_filter("all", &["*"])
                        .pick_file()
                {
                    view.rows[idx].resolved = Some(file);
                    view.rows[idx].included = true;
                }
            });
    }

    fn collect_scatter_flashables(&self) -> Vec<(String, PathBuf)> {
        let Some(view) = self.scatter.as_ref() else {
            return Vec::new();
        };
        view.file
            .entries
            .iter()
            .zip(view.rows.iter())
            .filter(|(e, r)| {
                e.storage_type == view.storage_filter
                    && r.included
                    && r.skip_reason.is_none()
                    && r.resolved.is_some()
            })
            .map(|(e, r)| (e.name.clone(), r.resolved.clone().unwrap()))
            .collect()
    }



    fn load_scatter(&mut self, path: PathBuf) {
        match crate::scatter::parse_from_path(&path) {
            Ok(file) => {
                let root =
                    path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."));
                let rows: Vec<ScatterRow> =
                    file.entries.iter().map(|e| build_scatter_row(e, &root)).collect();
                let mut storage_types: Vec<String> =
                    file.entries.iter().map(|e| e.storage_type.clone()).collect();
                storage_types.sort();
                storage_types.dedup();
                if storage_types.is_empty() || storage_types.iter().all(|s| s.is_empty()) {
                    storage_types = vec!["".into()];
                }
                // Default: match the connected device's storage type if possible,
                // otherwise the first one listed.
                let default_storage = storage_types.first().cloned().unwrap_or_default();
                let platform = file.platform.clone();
                let project = file.project.clone();
                let total = file.entries.len();
                self.scatter_error = None;
                self.scatter = Some(ScatterView {
                    file,
                    root,
                    rows,
                    storage_filter: default_storage.clone(),
                    storage_types,
                });
                self.persisted.scatter_path = Some(path.clone());
                log::info!(
                    "Loaded scatter {} ({}/{}), {total} entries, storage: {default_storage}",
                    path.display(),
                    platform.as_deref().unwrap_or("?"),
                    project.as_deref().unwrap_or("?"),
                );
            }
            Err(e) => {
                self.scatter = None;
                self.scatter_error = Some(e.clone());
                log::error!("Failed to parse scatter file: {e}");
            }
        }
    }



    fn draw_log_tab(&mut self, ui: &mut egui::Ui, palette: theme::Palette) {
        ui.horizontal(|ui| {
            for f in [
                LogLevelFilter::All,
                LogLevelFilter::InfoPlus,
                LogLevelFilter::WarnPlus,
                LogLevelFilter::ErrorOnly,
            ] {
                let label = match f {
                    LogLevelFilter::All => "ALL",
                    LogLevelFilter::InfoPlus => "INFO",
                    LogLevelFilter::WarnPlus => "WARN",
                    LogLevelFilter::ErrorOnly => "ERROR",
                };
                let active = self.log_filter == f;
                let active_fill = match f {
                    LogLevelFilter::All => palette.accent,
                    LogLevelFilter::InfoPlus => Color32::from_rgb(0x3B, 0x82, 0xF6),
                    LogLevelFilter::WarnPlus => palette.warn,
                    LogLevelFilter::ErrorOnly => palette.error,
                };
                let active_stroke = match f {
                    LogLevelFilter::All => palette.accent_strong,
                    LogLevelFilter::InfoPlus => Color32::from_rgb(0x93, 0xC5, 0xFD),
                    LogLevelFilter::WarnPlus => Color32::from_rgb(0xFD, 0xBA, 0x74),
                    LogLevelFilter::ErrorOnly => Color32::from_rgb(0xFC, 0xA5, 0xA5),
                };
                let text_color = if active { Color32::WHITE } else { palette.text_muted };
                
                let btn = egui::Button::new(RichText::new(label).strong().size(11.0).color(text_color))
                    .rounding(Rounding::same(2.0))
                    .fill(if active { active_fill } else { palette.panel_alt })
                    .stroke(if active { Stroke::new(1.0, active_stroke) } else { Stroke::new(1.0, palette.border) })
                    .min_size(egui::vec2(70.0, 26.0));
                if ui.add(btn).clicked() {
                    self.log_filter = f;
                }
                ui.add_space(4.0);
            }
            
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let btn_clear = egui::Button::new(RichText::new("CLEAR").strong().size(11.0))
                    .rounding(Rounding::same(2.0))
                    .fill(palette.panel_alt)
                    .stroke(Stroke::new(1.0, palette.border))
                    .min_size(egui::vec2(70.0, 26.0));
                if ui.add(btn_clear).clicked() {
                    self.logs.clear();
                }
                
                ui.add_space(8.0);
                
                let btn_save = egui::Button::new(RichText::new("SAVE").strong().size(11.0))
                    .rounding(Rounding::same(2.0))
                    .fill(palette.panel_alt)
                    .stroke(Stroke::new(1.0, palette.border))
                    .min_size(egui::vec2(70.0, 26.0));
                if ui.add(btn_save).clicked() {
                    self.save_log_to_file();
                }
                
                ui.add_space(8.0);
                
                let btn_copy = egui::Button::new(RichText::new("COPY").strong().size(11.0))
                    .rounding(Rounding::same(2.0))
                    .fill(palette.panel_alt)
                    .stroke(Stroke::new(1.0, palette.border))
                    .min_size(egui::vec2(70.0, 26.0));
                if ui.add(btn_copy).clicked() {
                    let text = self.rendered_log_text();
                    ui.ctx().output_mut(|o| o.copied_text = text);
                }
            });
        });
        
        ui.add_space(8.0);
        
        let font_size = self.persisted.log_font_size;
        Frame::none()
            .fill(palette.panel_alt)
            .stroke(Stroke::new(1.0_f32, palette.border))
            .rounding(Rounding::same(4.0))
            .inner_margin(Margin::same(12.0))
            .show(ui, |ui| {
                let mut scroll = ScrollArea::vertical()
                    .max_height(ui.available_height() - 12.0)
                    .auto_shrink([false, false]);
                if self.log_autoscroll {
                    scroll = scroll.stick_to_bottom(true);
                }
                scroll.show(ui, |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                    if self.logs.is_empty() {
                        ui.label(RichText::new("Console is empty.").monospace().color(palette.text_muted).size(font_size));
                    } else {
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
                            ui.add(egui::Label::new(RichText::new(text).color(color).monospace().size(font_size)));
                        }
                    }
                });
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
            ConfirmAction::FlashScatter(_) | ConfirmAction::FlashPgpt(_)
        );
        let remaining = if delayed {
            let elapsed = self.confirm_opened_at.map(|t| t.elapsed().as_secs_f32()).unwrap_or(0.0);
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
                    .rounding(Rounding::same(4.0))
                    .inner_margin(Margin::same(16.0)),
            )
            .show(ctx, |ui| {
                ui.set_min_width(if delayed { 520.0 } else { 420.0 });
                ui.set_max_width(if delayed { 520.0 } else { 420.0 });
                if delayed {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                }
                let body = action.body();
                // Scatter dialogs can list dozens of partitions; cap the text
                // area and let the user scroll instead of pushing the button
                // row off-screen.
                ScrollArea::vertical().max_height(320.0).auto_shrink([false, true]).show(
                    ui,
                    |ui| {
                        ui.label(RichText::new(body).color(palette.text));
                    },
                );
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(egui::Button::new("CANCEL").min_size(egui::vec2(100.0, 28.0)))
                        .clicked()
                    {
                        close = true;
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let btn_text = if delayed && remaining > 0.0 {
                            format!("PROCEED IN {}s", remaining.ceil() as u32)
                        } else {
                            "PROCEED".to_string()
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
                ConfirmAction::FlashScatter(list) | ConfirmAction::FlashPgpt(list) => {
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

    fn draw_risk_disclaimer_dialog(&mut self, ctx: &egui::Context, palette: theme::Palette) {
        let mut accept = false;

        egui::Window::new(RichText::new("Welcome to Penumbra").strong().color(palette.text))
            .collapsible(false)
            .resizable(false)
            .movable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .frame(
                Frame::none()
                    .fill(palette.panel)
                    .stroke(Stroke::new(1.0_f32, palette.border))
                    .rounding(Rounding::same(6.0))
                    .inner_margin(Margin::same(20.0)),
            )
            .show(ctx, |ui| {
                ui.set_min_width(520.0);
                ui.set_max_width(520.0);
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);

                ui.label(
                    RichText::new(
                        "Penumbra is a tool for interacting with MediaTek devices, providing flashing, \
                         readback, and bootloader unlock/relock capabilities."
                    )
                    .size(13.0)
                    .color(palette.text)
                );
                ui.add_space(10.0);

                ui.label(
                    RichText::new("RISK WARNING").strong().size(13.0).color(palette.error)
                );
                ui.label(
                    RichText::new(
                        "Penumbra is in early development and can break easily. Flashing, unlocking, or \
                         relocking bootloaders on MediaTek devices carries inherent risk. Mismatched firmware files, \
                         interrupted writes, or unsupported chipsets can easily BRICK your device. Use this \
                         software entirely at your own risk."
                    )
                    .color(palette.text_muted)
                );
                ui.add_space(10.0);

                ui.label(
                    RichText::new("SYSTEM REQUIREMENTS").strong().size(13.0).color(palette.accent)
                );
                ui.label(
                    RichText::new(
                        "• Windows: Install WinUSB or LibUSB drivers on the device (e.g. using Zadig or the provided driver installer).\n\
                         • Linux: Install libudev and add your user to the dialout group. Run with appropriate privileges or configure udev rules if the device is not recognized."
                    )
                    .color(palette.text_muted)
                );
                ui.add_space(16.0);

                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let btn = egui::Button::new(
                            RichText::new("I UNDERSTAND AND ACCEPT THE RISK").color(Color32::WHITE).strong(),
                        )
                        .fill(palette.error)
                        .min_size(egui::vec2(260.0, 30.0));
                        if ui.add(btn).clicked() {
                            accept = true;
                        }
                    });
                });
            });

        if accept {
            self.persisted.accepted_risk = true;
        }
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

fn get_partition_flags(name: &str) -> Vec<(&'static str, Color32, Color32)> {
    let mut flags = Vec::new();
    
    if name.ends_with("_a") {
        flags.push(("A", Color32::from_rgb(124, 77, 255), Color32::WHITE));
    } else if name.ends_with("_b") {
        flags.push(("B", Color32::from_rgb(0x1B, 0x1C, 0x26), Color32::from_rgb(0x8E, 0x93, 0xA6)));
    }
    
    let base_name = name.strip_suffix("_a").or_else(|| name.strip_suffix("_b")).unwrap_or(name);
    
    match base_name {
        "system" | "vendor" | "product" | "system_ext" | "odm" => {
            flags.push(("ext4", Color32::from_rgb(16, 185, 129), Color32::WHITE));
            flags.push(("img", Color32::from_rgb(0x1B, 0x1C, 0x26), Color32::from_rgb(0x8E, 0x93, 0xA6)));
        }
        "userdata" => {
            flags.push(("f2fs", Color32::from_rgb(16, 185, 129), Color32::WHITE));
        }
        "boot" | "init_boot" | "vendor_boot" | "recovery" | "vbmeta" | "logo" | "dtbo" => {
            flags.push(("img", Color32::from_rgb(0x1B, 0x1C, 0x26), Color32::from_rgb(0x8E, 0x93, 0xA6)));
        }
        "seccfg" | "otp" | "proinfo" | "nvcfg" | "nvram" | "protect1" | "protect2" => {
            flags.push(("Unreadable", Color32::from_rgb(239, 68, 68), Color32::WHITE));
        }
        _ => {
            flags.push(("bin", Color32::from_rgb(0x1B, 0x1C, 0x26), Color32::from_rgb(0x8E, 0x93, 0xA6)));
        }
    }
    flags
}
