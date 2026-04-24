/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2026 Shomy
*/

//! Entry point for the Penumbra GUI.
//!
//! On Windows, release builds use the `windows` subsystem so a console window
//! does not flash up. Debug builds and non-Windows builds keep the default
//! subsystem so `log` output goes to the terminal.

#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

mod app;
mod log_bridge;
mod messages;
mod scatter;
mod theme;
mod worker;

use std::sync::mpsc;

use anyhow::Result;
use eframe::NativeOptions;
use eframe::egui::ViewportBuilder;

use crate::messages::{Event, LogLine};

fn main() -> Result<()> {
    let (log_tx, log_rx) = mpsc::channel::<LogLine>();
    let _ = log_bridge::init(log_tx, std::env::var("PENUMBRA_VERBOSE").is_ok());

    let (evt_tx, evt_rx) = mpsc::channel::<Event>();
    let handle = worker::spawn(evt_tx);

    let viewport = ViewportBuilder::default()
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size([960.0, 600.0])
        .with_title("Penumbra Flash Tool");

    let native_options = NativeOptions { viewport, ..Default::default() };

    eframe::run_native(
        "Penumbra Flash Tool",
        native_options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, handle, evt_rx, log_rx)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {e}"))
}
