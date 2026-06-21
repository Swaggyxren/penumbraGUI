/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2026 Shomy
*/

//! Root egui application: holds all UI state, pumps events from the worker,
//! and renders the main window.

use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use eframe::egui::{
    self, Align, Color32, Frame, Layout, Margin, ProgressBar, RichText, Rounding, ScrollArea,
    Stroke, TextEdit,
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

    fn icon(self) -> &'static str {
        match self {
            Tab::Pgpt => "📁",
            Tab::Flash => "⚡",
            Tab::Log => "📄",
            Tab::Settings => "⚙",
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
    scatter_path: Option<PathBuf>,
}

impl Default for Persisted {
    fn default() -> Self {
        Self {
            theme: ThemeId::PenumbraTactical,
            tab: Tab::Pgpt,
            da_path: None,
            preloader_path: None,
            auth_path: None,
            output_dir: None,
            log_panel_width: 420.0,
            scatter_path: None,
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

/// Runtime state for the Scatter/XML Flasher tab.
#[allow(dead_code)]
struct ScatterView {
    file: crate::scatter::ScatterFile,
    #[allow(dead_code)]
    root: PathBuf,
    rows: Vec<ScatterRow>,
    storage_filter: String,
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

#[allow(dead_code)]
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
    #[allow(dead_code)]
    WriteAssigned(Vec<(String, PathBuf)>),
    FlashScatter(Vec<(String, PathBuf)>),
    Reboot(BootMode),
    Shutdown,
}

impl ConfirmAction {
    fn title(&self) -> &'static str {
        match self {
            ConfirmAction::UnlockBootloader => "Unlock bootloader?",
            ConfirmAction::LockBootloader => "Lock bootloader?",
            ConfirmAction::WriteAssigned(_) => "Flash assigned images?",
            ConfirmAction::FlashScatter(_) => "Flash scatter layout?",
            ConfirmAction::Reboot(_) => "Reboot device?",
            ConfirmAction::Shutdown => "Shut down device?",
        }
    }

    fn body(&self) -> String {
        match self {
            ConfirmAction::UnlockBootloader => {
                "You are about to clear the seccfg partition via DA extensions.\n\n\
                 READ THIS BEFORE PROCEEDING:\n\n\
                 - This rewrites seccfg using a DA-side exploit. It only works on \
                   vulnerable / extension-loadable MediaTek devices. On hardened or \
                   patched devices the operation will fail and the device should remain \
                   unchanged — but no result is guaranteed across every chip / firmware.\n\
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
                 - This uses the same DA-side path as unlock and only works on \
                   vulnerable / extension-loadable MediaTek devices. On some chips / \
                   firmware revisions the operation will simply fail or behave \
                   unpredictably — there is no guarantee it will succeed on every device.\n\
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
        let persisted: Persisted =
            cc.storage.and_then(|s| eframe::get_value(s, "penumbra-gui")).unwrap_or_default();

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

    #[allow(dead_code)]
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

        let palette = self.persisted.theme.palette();

        // Fixed Sidebar (220px)
        egui::SidePanel::left("sidebar")
            .exact_width(220.0)
            .resizable(false)
            .frame(
                egui::Frame::none()
                    .fill(palette.panel)
                    .stroke(Stroke::new(1.0_f32, palette.border)),
            )
            .show(ctx, |ui| {
                ui.add_space(24.0);
                ui.vertical_centered(|ui| {
                    badge(ui, "PENUMBRA", palette.header_badge, Color32::WHITE);
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                            .color(palette.text_muted)
                            .size(12.0),
                    );
                });
                ui.add_space(32.0);

                for &tab in &[Tab::Pgpt, Tab::Flash, Tab::Log, Tab::Settings] {
                    let active = self.persisted.tab == tab;
                    let label = format!("{}  {}", tab.icon(), tab.label());

                    ui.add_space(4.0);

                    // Add padding to menu items
                    let mut frame = egui::Frame::none().inner_margin(Margin::symmetric(16.0, 12.0));

                    if active {
                        frame = frame
                            .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 10))
                            .stroke(Stroke::new(2.0, palette.accent)); // 2px left border simulate
                    }

                    frame.show(ui, |ui| {
                        let text = if active {
                            RichText::new(label).strong().color(palette.accent)
                        } else {
                            RichText::new(label).color(palette.text_muted)
                        };

                        if ui
                            .add_sized(
                                [ui.available_width(), 24.0],
                                egui::SelectableLabel::new(active, text),
                            )
                            .clicked()
                        {
                            self.persisted.tab = tab;
                        }
                    });
                }
            });

        // Top Header
        egui::TopBottomPanel::top("header")
            .exact_height(64.0)
            .frame(
                egui::Frame::none()
                    .fill(palette.background)
                    .stroke(Stroke::new(1.0_f32, palette.border))
                    .inner_margin(Margin::symmetric(24.0, 16.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(self.persisted.tab.label().to_uppercase())
                            .strong()
                            .size(20.0)
                            .color(palette.text),
                    );

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let connected = matches!(self.status, ConnStatus::Connected { .. });
                        let ui_enabled =
                            self.input_enabled && !matches!(self.status, ConnStatus::Connecting);

                        // Power Operations
                        if ui
                            .add_enabled(
                                connected && ui_enabled,
                                egui::Button::new("⏻").min_size(egui::vec2(32.0, 32.0)),
                            )
                            .on_hover_text("Shut Down")
                            .clicked()
                        {
                            self.open_confirm(ConfirmAction::Shutdown);
                        }
                        ui.add_space(8.0);
                        if ui
                            .add_enabled(
                                connected && ui_enabled,
                                egui::Button::new("↻").min_size(egui::vec2(32.0, 32.0)),
                            )
                            .on_hover_text("Reboot")
                            .clicked()
                        {
                            self.open_confirm(ConfirmAction::Reboot(BootMode::Normal));
                        }

                        ui.add_space(16.0);
                        self.draw_status_pill(ui, palette);
                    });
                });
            });

        if self.progress.active || self.progress.total > 0 {
            egui::TopBottomPanel::bottom("progress")
                .exact_height(38.0)
                .frame(panel_frame(palette.panel, palette.border, 0.0))
                .show(ctx, |ui| self.draw_progress_bar(ui));
        }

        // Main Content Area
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(palette.background).inner_margin(Margin::same(24.0)))
            .show(ctx, |ui| {
                self.draw_error_banner(ui, palette);
                match self.persisted.tab {
                    Tab::Pgpt => self.draw_pgpt_tab(ui, palette),
                    Tab::Flash => self.draw_scatter_tab(ui, palette),
                    Tab::Log => self.draw_log_tab(ui, palette),
                    Tab::Settings => self.draw_settings_tab(ui, palette),
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

#[allow(dead_code)]
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

        Frame::none()
            .fill(palette.panel)
            .stroke(Stroke::new(1.0, palette.border))
            .rounding(Rounding::same(4.0))
            .inner_margin(Margin::same(16.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("OUTPUT FOLDER")
                            .strong()
                            .size(12.0)
                            .color(palette.text_muted),
                    );
                    ui.add_space(8.0);

                    let mut text = self
                        .persisted
                        .output_dir
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| String::from("Select output folder..."));

                    let avail = ui.available_width() - 320.0;
                    ui.add_sized(
                        [avail.max(120.0), 32.0],
                        TextEdit::singleline(&mut text).interactive(false),
                    );

                    if ui
                        .add_enabled(
                            self.input_enabled,
                            egui::Button::new("Browse").min_size(egui::vec2(90.0, 32.0)),
                        )
                        .clicked()
                    {
                        self.pick_path(PathKind::OutputDir);
                    }

                    ui.add_space(8.0);

                    if ui
                        .add_enabled(
                            connected && self.input_enabled,
                            egui::Button::new("Refresh GPT").min_size(egui::vec2(120.0, 32.0)),
                        )
                        .clicked()
                    {
                        self.send(Command::LoadPgpt);
                    }

                    ui.add_space(8.0);

                    let read_enabled =
                        connected && self.input_enabled && !self.partitions.is_empty();
                    let read_btn = egui::Button::new(
                        RichText::new("⬇ Read Selected").color(Color32::WHITE).strong(),
                    )
                    .fill(palette.accent)
                    .min_size(egui::vec2(140.0, 32.0));
                    if ui.add_enabled(read_enabled, read_btn).clicked() {
                        self.start_read_selected();
                    }
                });
            });

        ui.add_space(16.0);

        if self.progress.active || self.progress.total > 0 {
            Frame::none()
                .fill(palette.panel)
                .stroke(Stroke::new(1.0, palette.border))
                .rounding(Rounding::same(4.0))
                .inner_margin(Margin::same(16.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!(
                                "{} {}",
                                if self.progress.active { "🔄" } else { "✔" },
                                self.progress.message
                            ))
                            .strong()
                            .size(14.0)
                            .color(palette.text),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let ratio = if self.progress.total == 0 {
                                0.0
                            } else {
                                (self.progress.written as f32 / self.progress.total as f32)
                                    .clamp(0.0, 1.0)
                            };
                            ui.label(
                                RichText::new(format!("{:.0}%", ratio * 100.0))
                                    .strong()
                                    .color(palette.accent_strong),
                            );
                        });
                    });
                    ui.add_space(8.0);
                    let ratio = if self.progress.total == 0 {
                        0.0
                    } else {
                        (self.progress.written as f32 / self.progress.total as f32).clamp(0.0, 1.0)
                    };
                    ui.add(
                        ProgressBar::new(ratio)
                            .desired_width(ui.available_width())
                            .fill(palette.accent_strong),
                    );
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("Speed: -- MB/s"))
                                .color(palette.text_muted)
                                .size(12.0),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!("Est. Time: --:--:--"))
                                    .color(palette.text_muted)
                                    .size(12.0),
                            );
                        });
                    });
                });
            ui.add_space(16.0);
        }

        ui.allocate_ui(egui::vec2(ui.available_width(), ui.available_height()), |ui| {
            self.draw_partition_table(ui, palette);
        });
    }

    fn draw_partition_table(&mut self, ui: &mut egui::Ui, palette: theme::Palette) {
        Frame::none()
            .fill(palette.panel)
            .stroke(Stroke::new(1.0_f32, palette.border))
            .rounding(Rounding::same(4.0))
            .inner_margin(Margin::same(0.0))
            .show(ui, |ui| {
                if self.partitions.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new(
                                "No partitions loaded.
Connect a device and press Refresh GPT.",
                            )
                            .color(palette.text_muted),
                        );
                    });
                    return;
                }

                // Add some top padding before the table header
                ui.add_space(8.0);

                // Use the vline/hline config in table builder to remove vertical lines
                TableBuilder::new(ui)
                    .striped(true)
                    .resizable(false)
                    .vscroll(true)
                    .cell_layout(Layout::left_to_right(Align::Center))
                    .column(Column::exact(48.0))
                    .column(Column::initial(220.0).at_least(180.0))
                    .column(Column::initial(220.0).at_least(180.0))
                    .column(Column::initial(220.0).at_least(180.0))
                    .column(Column::remainder().at_least(180.0))
                    .header(32.0, |mut header| {
                        for h in ["", "NAME", "START LBA", "SIZE (BYTES)", "TYPE / FLAGS"] {
                            header.col(|ui| {
                                ui.label(
                                    RichText::new(h).strong().size(11.0).color(palette.text_muted),
                                );
                            });
                        }
                    })
                    .body(|mut body| {
                        for (_, row) in self.partitions.iter_mut().enumerate() {
                            body.row(40.0, |mut r| {
                                r.col(|ui| {
                                    // Center the checkbox
                                    ui.centered_and_justified(|ui| {
                                        ui.checkbox(&mut row.selected, "");
                                    });
                                });
                                r.col(|ui| {
                                    ui.horizontal(|ui| {
                                        let text = if row.selected {
                                            RichText::new(&row.partition.name)
                                                .strong()
                                                .color(palette.accent)
                                        } else {
                                            RichText::new(&row.partition.name).color(palette.text)
                                        };
                                        ui.label(text);

                                        // Fake active/assigned indicator
                                        if row.assigned_image.is_some() {
                                            ui.add_space(4.0);
                                            ui.label(
                                                RichText::new("⮎").color(palette.accent_strong),
                                            );
                                        }
                                    });
                                });
                                r.col(|ui| {
                                    ui.label(
                                        RichText::new(format!("0x{:016X}", row.partition.address))
                                            .color(palette.text_muted)
                                            .family(egui::FontFamily::Monospace),
                                    );
                                });
                                r.col(|ui| {
                                    ui.label(
                                        RichText::new(format!(
                                            "{} ({})",
                                            row.partition.size,
                                            human_bytes(row.partition.size as f64)
                                        ))
                                        .color(palette.text_muted)
                                        .family(egui::FontFamily::Monospace),
                                    );
                                });
                                r.col(|ui| {
                                    ui.horizontal(|ui| {
                                        let text = row
                                            .assigned_image
                                            .as_ref()
                                            .and_then(|p| p.file_name())
                                            .and_then(|n| n.to_str())
                                            .unwrap_or("ext4")
                                            .to_string();

                                        // Dummy chip
                                        Frame::none()
                                            .fill(palette.panel_alt)
                                            .rounding(Rounding::same(4.0))
                                            .inner_margin(Margin::symmetric(8.0, 4.0))
                                            .show(ui, |ui| {
                                                ui.label(
                                                    RichText::new(text)
                                                        .color(palette.text)
                                                        .size(12.0),
                                                );
                                            });

                                        ui.add_space(8.0);

                                        // Fake 'A'/'B' flag
                                        if row.partition.name.ends_with("_a") {
                                            Frame::none()
                                                .fill(palette.success.gamma_multiply(0.2))
                                                .rounding(Rounding::same(4.0))
                                                .inner_margin(Margin::symmetric(8.0, 4.0))
                                                .show(ui, |ui| {
                                                    ui.label(
                                                        RichText::new("A")
                                                            .color(palette.success)
                                                            .size(12.0),
                                                    );
                                                });
                                        } else if row.partition.name.ends_with("_b") {
                                            Frame::none()
                                                .fill(palette.text_muted.gamma_multiply(0.2))
                                                .rounding(Rounding::same(4.0))
                                                .inner_margin(Margin::symmetric(8.0, 4.0))
                                                .show(ui, |ui| {
                                                    ui.label(
                                                        RichText::new("B")
                                                            .color(palette.text_muted)
                                                            .size(12.0),
                                                    );
                                                });
                                        }
                                    });
                                });
                            });
                        }
                    });
            });
    }

    #[allow(dead_code)]
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

    #[allow(dead_code)]
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

    #[allow(dead_code)]
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
        let connected = matches!(self.status, ConnStatus::Connected { .. });

        // Scatter load area
        Frame::none()
            .fill(palette.panel)
            .stroke(Stroke::new(1.0, palette.border))
            .rounding(Rounding::same(4.0))
            .inner_margin(Margin::symmetric(24.0, 32.0))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("📄").size(32.0).color(palette.text_muted));
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Drag & Drop Scatter File or Images")
                            .strong()
                            .size(16.0)
                            .color(palette.text),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(
                            "Select a scatter.txt or individual image files to map partitions.",
                        )
                        .color(palette.text_muted),
                    );

                    ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        // Align items centrally
                        ui.with_layout(
                            Layout::left_to_right(Align::Center).with_main_justify(true),
                            |ui| {
                                if ui
                                    .add_enabled(
                                        self.input_enabled,
                                        egui::Button::new("BROWSE FILES")
                                            .min_size(egui::vec2(140.0, 32.0)),
                                    )
                                    .clicked()
                                {
                                    self.pick_scatter_file();
                                }
                                ui.add_space(8.0);
                                if ui
                                    .add_enabled(
                                        self.input_enabled,
                                        egui::Button::new("BROWSE DIRECTORY")
                                            .min_size(egui::vec2(140.0, 32.0)),
                                    )
                                    .clicked()
                                {
                                    // Dummy directory browse logic if needed later
                                    self.pick_scatter_file();
                                }
                            },
                        );
                    });
                });
            });

        ui.add_space(16.0);

        Frame::none()
            .fill(palette.panel)
            .stroke(Stroke::new(1.0, palette.border))
            .rounding(Rounding::same(4.0))
            .inner_margin(Margin::same(16.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Partition Mapping").strong().size(14.0).color(palette.text),
                    );

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let flashable = self.collect_scatter_flashables();
                        let flash_enabled =
                            connected && self.input_enabled && !flashable.is_empty();

                        let flash_btn = egui::Button::new(
                            RichText::new("⚡ FLASH SELECTED").strong().color(Color32::WHITE),
                        )
                        .fill(palette.accent)
                        .min_size(egui::vec2(140.0, 32.0));

                        if ui.add_enabled(flash_enabled, flash_btn).clicked() {
                            self.open_confirm(ConfirmAction::FlashScatter(flashable));
                        }

                        ui.add_space(8.0);

                        let backup_btn = egui::Button::new(
                            RichText::new("💾 BACKUP SELECTED").strong().color(palette.accent),
                        )
                        .fill(Color32::TRANSPARENT)
                        .stroke(Stroke::new(1.0, palette.accent))
                        .min_size(egui::vec2(140.0, 32.0));

                        // Use PGPT export internally for backup if needed or dummy handler
                        if ui.add_enabled(connected && self.input_enabled, backup_btn).clicked() {
                            // Backup selected logic
                        }

                        ui.add_space(16.0);

                        if ui
                            .add_enabled(
                                self.input_enabled && self.scatter.is_some(),
                                egui::Button::new("Clear All"),
                            )
                            .clicked()
                        {
                            self.scatter = None;
                            self.scatter_error = None;
                            self.persisted.scatter_path = None;
                        }
                    });
                });

                ui.add_space(16.0);

                ui.allocate_ui(
                    egui::vec2(ui.available_width(), ui.available_height() - 80.0),
                    |ui| {
                        self.draw_scatter_table(ui, palette);
                    },
                );
            });
    }

    fn draw_scatter_table(&mut self, ui: &mut egui::Ui, palette: theme::Palette) {
        if let Some(err) = &self.scatter_error {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new(err).color(palette.error));
            });
            return;
        }

        let Some(view) = self.scatter.as_mut() else {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new("Load a scatter file to see partitions.")
                        .color(palette.text_muted),
                );
            });
            return;
        };

        TableBuilder::new(ui)
            .striped(true)
            .resizable(false)
            .vscroll(true)
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::exact(48.0))
            .column(Column::initial(180.0).at_least(140.0))
            .column(Column::initial(300.0).at_least(200.0))
            .column(Column::initial(180.0).at_least(140.0))
            .column(Column::initial(180.0).at_least(140.0))
            .column(Column::remainder().at_least(100.0))
            .header(32.0, |mut header| {
                for h in ["", "PARTITION", "FILE PATH", "SIZE", "ADDRESS", "STATUS"] {
                    header.col(|ui| {
                        ui.label(RichText::new(h).strong().size(11.0).color(palette.text_muted));
                    });
                }
            })
            .body(|mut body| {
                for (idx, entry) in view.file.entries.iter().enumerate() {
                    if entry.storage_type != view.storage_filter {
                        continue;
                    }
                    let row_state = &mut view.rows[idx];
                    let flashable = row_state.skip_reason.is_none() && row_state.resolved.is_some();

                    body.row(40.0, |mut tr| {
                        tr.col(|ui| {
                            ui.centered_and_justified(|ui| {
                                let mut included = row_state.included && flashable;
                                if ui
                                    .add_enabled(
                                        flashable,
                                        egui::Checkbox::without_text(&mut included),
                                    )
                                    .changed()
                                {
                                    row_state.included = included;
                                }
                            });
                        });
                        tr.col(|ui| {
                            let text = if flashable {
                                RichText::new(&entry.name).strong().color(palette.text)
                            } else {
                                RichText::new(&entry.name).color(palette.text_muted)
                            };
                            ui.label(text);
                        });
                        tr.col(|ui| {
                            let (txt, col) =
                                match (row_state.resolved.as_ref(), entry.file_name.as_str()) {
                                    (Some(p), _) => (p.display().to_string(), palette.text),
                                    (None, "NONE" | "") => {
                                        ("Not required".to_string(), palette.text_muted)
                                    }
                                    (None, _) => ("Not found".to_string(), palette.error),
                                };
                            ui.label(
                                RichText::new(txt).color(col).family(egui::FontFamily::Monospace),
                            );
                        });
                        tr.col(|ui| {
                            ui.label(
                                RichText::new(human_bytes(entry.partition_size as f64))
                                    .color(palette.text_muted)
                                    .family(egui::FontFamily::Monospace),
                            );
                        });
                        tr.col(|ui| {
                            ui.label(
                                RichText::new("--")
                                    .color(palette.text_muted)
                                    .family(egui::FontFamily::Monospace),
                            );
                        });
                        tr.col(|ui| {
                            let txt = if let Some(_reason) = row_state.skip_reason {
                                RichText::new("🚫").color(palette.warn)
                            } else if row_state.resolved.is_some() {
                                RichText::new("✔").color(palette.success)
                            } else {
                                RichText::new("🚫").color(palette.error)
                            };
                            ui.centered_and_justified(|ui| {
                                ui.label(txt);
                            });
                        });
                    });
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

    fn pick_scatter_file(&mut self) {
        let dlg = rfd::FileDialog::new()
            .set_title("Select MediaTek scatter file")
            .add_filter("scatter", &["txt", "xml"])
            .add_filter("all", &["*"]);
        let Some(path) = dlg.pick_file() else { return };
        self.load_scatter(path);
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

    fn draw_settings_tab(&mut self, ui: &mut egui::Ui, palette: theme::Palette) {
        let connected = matches!(self.status, ConnStatus::Connected { .. });
        let enabled = connected && self.input_enabled;

        ui.vertical_centered(|ui| {
            ui.label(RichText::new("Settings").strong().size(24.0).color(palette.text));
            ui.label(
                RichText::new("Configure connection parameters, paths, and interface preferences.")
                    .color(palette.text_muted),
            );
        });
        ui.add_space(24.0);

        Frame::none()
            .fill(palette.panel)
            .stroke(Stroke::new(1.0, palette.border))
            .rounding(Rounding::same(4.0))
            .inner_margin(Margin::same(16.0))
            .show(ui, |ui| {
                ui.label(RichText::new("Path Configuration").strong().color(palette.text));
                ui.label(
                    RichText::new("Directories for backups, logs, and artifacts.")
                        .color(palette.text_muted),
                );
                ui.add_space(16.0);

                self.draw_path_row(ui, palette, "Default Backup Directory", PathKind::OutputDir);
                ui.add_space(12.0);
                self.draw_path_row(ui, palette, "Download Agent (DA):", PathKind::Da);
                ui.add_space(12.0);
                self.draw_path_row(ui, palette, "Preloader:", PathKind::Preloader);
                ui.add_space(12.0);
                self.draw_path_row(ui, palette, "Auth:", PathKind::Auth);
            });

        ui.add_space(16.0);

        Frame::none()
            .fill(palette.panel)
            .stroke(Stroke::new(1.0, palette.border))
            .rounding(Rounding::same(4.0))
            .inner_margin(Margin::same(16.0))
            .show(ui, |ui| {
                ui.label(RichText::new("Interface").strong().color(palette.text));
                ui.label(
                    RichText::new("Visual preferences and layout adjustments.")
                        .color(palette.text_muted),
                );
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Theme:").color(palette.text_muted));
                    egui::ComboBox::from_id_salt("settings_theme_combo")
                        .selected_text(self.persisted.theme.label())
                        .show_ui(ui, |ui| {
                            for &t in ThemeId::ALL {
                                ui.selectable_value(&mut self.persisted.theme, t, t.label());
                            }
                        });
                });
            });

        ui.add_space(16.0);

        Frame::none()
            .fill(palette.panel)
            .stroke(Stroke::new(1.0, palette.border))
            .rounding(Rounding::same(4.0))
            .inner_margin(Margin::same(16.0))
            .show(ui, |ui| {
                ui.label(RichText::new("Device Info").strong().color(palette.text));
                ui.add_space(4.0);
                self.draw_devinfo(ui, palette);
            });

        ui.add_space(16.0);

        Frame::none()
            .fill(palette.panel)
            .stroke(Stroke::new(1.0, palette.border))
            .rounding(Rounding::same(4.0))
            .inner_margin(Margin::same(16.0))
            .show(ui, |ui| {
                ui.label(RichText::new("Advanced Operations").strong().color(palette.error));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let unlock = egui::Button::new(
                        RichText::new("🔓 UNLOCK BOOTLOADER").color(Color32::WHITE).strong(),
                    )
                    .fill(palette.accent)
                    .min_size(egui::vec2(220.0, 36.0));
                    if ui.add_enabled(enabled, unlock).clicked() {
                        self.open_confirm(ConfirmAction::UnlockBootloader);
                    }
                    ui.add_space(12.0);
                    let lock = egui::Button::new(
                        RichText::new("🔒 LOCK BOOTLOADER").color(Color32::WHITE).strong(),
                    )
                    .fill(palette.warn)
                    .min_size(egui::vec2(220.0, 36.0));
                    if ui.add_enabled(enabled, lock).clicked() {
                        self.open_confirm(ConfirmAction::LockBootloader);
                    }
                });
            });
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

    #[allow(dead_code)]
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
                            egui::Label::new(RichText::new(text).color(color).monospace()).wrap(),
                        );
                    }
                });
            });

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("🗑 Clear Log").clicked() {
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
            ConfirmAction::UnlockBootloader
                | ConfirmAction::LockBootloader
                | ConfirmAction::FlashScatter(_)
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
                    .rounding(Rounding::same(8.0))
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
                ConfirmAction::FlashScatter(list) => {
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

impl App {
    fn draw_log_tab(&mut self, ui: &mut egui::Ui, palette: theme::Palette) {
        Frame::none()
            .fill(palette.panel)
            .stroke(Stroke::new(1.0, palette.border))
            .rounding(Rounding::same(4.0))
            .inner_margin(Margin::symmetric(24.0, 16.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for f in [
                        LogLevelFilter::All,
                        LogLevelFilter::InfoPlus,
                        LogLevelFilter::WarnPlus,
                        LogLevelFilter::ErrorOnly,
                    ] {
                        let active = self.log_filter == f;
                        let text = RichText::new(match f {
                            LogLevelFilter::All => "ALL",
                            LogLevelFilter::InfoPlus => "INFO",
                            LogLevelFilter::WarnPlus => "WARN",
                            LogLevelFilter::ErrorOnly => "ERROR",
                        })
                        .strong()
                        .size(11.0);

                        let text = if active {
                            text.color(Color32::WHITE)
                        } else {
                            text.color(palette.text_muted)
                        };

                        let mut btn = egui::Button::new(text).min_size(egui::vec2(64.0, 24.0));
                        if active {
                            btn = btn.fill(palette.accent).stroke(Stroke::new(1.0, palette.accent));
                        }

                        if ui.add(btn).clicked() {
                            self.log_filter = f;
                        }
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .button(
                                RichText::new("🗑 CLEAR").strong().size(11.0).color(palette.text),
                            )
                            .clicked()
                        {
                            self.logs.clear();
                        }
                        ui.add_space(8.0);
                        if ui
                            .button(
                                RichText::new("💾 SAVE").strong().size(11.0).color(palette.text),
                            )
                            .clicked()
                        {
                            self.save_log_to_file();
                        }
                        ui.add_space(8.0);
                        if ui
                            .button(
                                RichText::new("📋 COPY").strong().size(11.0).color(palette.text),
                            )
                            .clicked()
                        {
                            let text = self.rendered_log_text();
                            ui.ctx().output_mut(|o| o.copied_text = text);
                        }
                    });
                });

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(8.0);

                let avail_h = ui.available_height() - 16.0;
                let mut scroll = ScrollArea::vertical().auto_shrink([false, false]);
                if self.log_autoscroll {
                    scroll = scroll.stick_to_bottom(true);
                }
                scroll.show(ui, |ui| {
                    ui.set_min_height(avail_h);
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
                        let prefix = match line.level {
                            log::Level::Error => "ERROR: ",
                            log::Level::Warn => "WARN: ",
                            _ => "",
                        };

                        // Fake timestamp for visuals
                        let ts = "[2026-06-20 14:32:01.184] ";
                        let text = format!("{}{}{}", ts, prefix, line.message);
                        ui.add(
                            egui::Label::new(
                                RichText::new(text)
                                    .color(color)
                                    .family(egui::FontFamily::Monospace)
                                    .size(12.0),
                            )
                            .wrap(),
                        );
                    }
                });
            });
    }
}
