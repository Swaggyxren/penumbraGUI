/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2026 Shomy
*/

//! Command / Event message types exchanged between the UI thread and the worker thread.
//!
//! The UI thread sends [`Command`]s into an `mpsc` channel; the worker thread
//! consumes them, performs blocking device I/O via the `penumbra` crate, and
//! emits [`Event`]s back which the UI drains on every frame.

use std::path::PathBuf;

use penumbra::core::storage::Partition;
use penumbra::da::protocol::BootMode;

/// Which lock state to apply when toggling seccfg.
///
/// Mirrors `penumbra::core::seccfg::LockFlag` without requiring that upstream
/// enum to derive `Clone`/`Debug`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockAction {
    Unlock,
    Lock,
}

/// A request from the UI to the worker thread.
#[derive(Debug, Clone)]
pub enum Command {
    /// Connect to the first MTK device currently visible on USB.
    Connect {
        da_path: Option<PathBuf>,
        preloader_path: Option<PathBuf>,
        auth_path: Option<PathBuf>,
    },
    /// Drop any connected device and release resources.
    Disconnect,
    /// Enter DA mode and fetch the PGPT partition list.
    LoadPgpt,
    /// Read each named partition, writing <output_dir>/<name>.bin.
    ReadPartitions { names: Vec<String>, output_dir: PathBuf },
    /// For each (partition_name, image_path) pair, write the image into
    /// the partition on the device.
    WriteAssigned { assignments: Vec<(String, PathBuf)> },
    /// Flip the seccfg lock state. Requires DA extensions / a vulnerable device.
    Seccfg(LockAction),
    /// Reboot the device.
    Reboot(BootMode),
    /// Shut down the device.
    Shutdown,
    /// Set the "cancel requested" flag; the next progress tick will bail out.
    Cancel,
}

/// High-level connection lifecycle status, used to drive the status pill in the header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnStatus {
    Disconnected,
    Connecting,
    Connected { chip_name: String, hw_code: u16 },
}

/// A message emitted by the worker thread, consumed by the UI on every frame.
#[derive(Debug, Clone)]
pub enum Event {
    /// The connection lifecycle moved to a new state.
    StatusChanged(ConnStatus),
    /// The PGPT has been loaded; `partitions` is the full partition list.
    PartitionsLoaded { partitions: Vec<Partition> },
    /// A long-running operation just started; the progress bar should show `message`
    /// and span from 0 to `total_bytes`.
    ProgressStart { total_bytes: u64, message: String },
    /// Update the progress bar to `written` bytes, optionally updating the message.
    ProgressUpdate { written: u64, message: Option<String> },
    /// The current operation finished (successfully or otherwise); clear the bar.
    ProgressFinish { message: String },
    /// A human-readable error message to surface to the user.
    Error(String),
    /// Whether input controls should be enabled (false while a blocking op runs).
    InputEnabled(bool),
}

/// A single row in the execution log pane.
///
/// Log lines flow through their own `mpsc` channel (see `log_bridge.rs`) rather
/// than through the worker's [`Event`] channel, so the GUI can keep the log
/// pane live even while a command is in flight.
#[derive(Debug, Clone)]
pub struct LogLine {
    pub level: log::Level,
    #[allow(dead_code)] // Kept for future per-target filtering in the UI.
    pub target: String,
    pub message: String,
}
