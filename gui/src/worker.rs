/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2026 Shomy
*/

//! Background worker thread that owns the connected [`penumbra::Device`] and
//! serves [`Command`]s one at a time.
//!
//! The worker runs on a dedicated `std::thread` (no tokio runtime): every
//! device method in `penumbra` is synchronous / blocking, so a plain thread is
//! the simplest fit. Cancellation is cooperative — the UI sets the shared
//! `cancel` flag and the next progress tick aborts the in-flight operation.

use std::fs::{File, OpenOptions, create_dir_all, metadata};
use std::io::{BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::thread;

use anyhow::{Result, anyhow};
use log::{error, info, warn};
use penumbra::core::seccfg::LockFlag;
use penumbra::da::protocol::BootMode;
use penumbra::{Device, DeviceBuilder, find_mtk_port};

use crate::error_format::friendly;
use crate::messages::{Command, ConnStatus, Event, LockAction};

/// Handle kept by the UI thread for talking to the worker.
pub struct WorkerHandle {
    pub cmd_tx: Sender<Command>,
    pub cancel: Arc<AtomicBool>,
}

pub fn spawn(evt_tx: Sender<Event>) -> WorkerHandle {
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<Command>();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_worker = cancel.clone();

    thread::Builder::new()
        .name("penumbra-gui-worker".into())
        .spawn(move || run(cmd_rx, evt_tx, cancel_worker))
        .expect("failed to spawn penumbra-gui worker thread");

    WorkerHandle { cmd_tx, cancel }
}

struct Worker {
    device: Option<Device>,
    evt_tx: Sender<Event>,
    cancel: Arc<AtomicBool>,
}

impl Worker {
    fn send(&self, evt: Event) {
        let _ = self.evt_tx.send(evt);
    }

    fn set_status(&self, status: ConnStatus) {
        self.send(Event::StatusChanged(status));
    }

    fn set_input(&self, enabled: bool) {
        self.send(Event::InputEnabled(enabled));
    }

    fn fail(&self, err: impl ToString) {
        let msg = err.to_string();
        error!("{msg}");
        self.send(Event::Error(msg));
    }

    fn make_progress_callback(
        evt_tx: Sender<Event>,
        label: &'static str,
    ) -> impl FnMut(usize, usize) {
        let mut last_pct: i64 = -1;
        move |written: usize, total: usize| {
            let pct = (written as u64)
                .checked_mul(100)
                .and_then(|p| p.checked_div(total as u64))
                .unwrap_or(0) as i64;
            if pct != last_pct {
                last_pct = pct;
                let _ = evt_tx.send(Event::ProgressUpdate {
                    written: written as u64,
                    message: Some(format!("{label} ({pct}%)")),
                });
            } else {
                let _ =
                    evt_tx.send(Event::ProgressUpdate { written: written as u64, message: None });
            }
        }
    }

    /// Connect to the first MTK port we see, load DA data, and run `init`.
    fn connect(
        &mut self,
        da_path: Option<PathBuf>,
        preloader_path: Option<PathBuf>,
        auth_path: Option<PathBuf>,
    ) -> Result<()> {
        self.set_status(ConnStatus::Connecting);

        let da_data = match &da_path {
            Some(p) => Some(std::fs::read(p)?),
            None => None,
        };
        let pl_data = match &preloader_path {
            Some(p) => Some(std::fs::read(p)?),
            None => None,
        };
        let auth_data = match &auth_path {
            Some(p) => Some(std::fs::read(p)?),
            None => None,
        };

        info!("Searching for MTK port...");
        let mut tries: u32 = 0;
        let port = loop {
            if let Some(port) = find_mtk_port() {
                break port;
            }
            if self.cancel.swap(false, Ordering::SeqCst) {
                self.set_status(ConnStatus::Disconnected);
                return Err(anyhow!("Connection cancelled"));
            }
            tries += 1;
            if tries.is_multiple_of(20) {
                info!("Still waiting for a MediaTek device...");
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
            if tries > 240 {
                self.set_status(ConnStatus::Disconnected);
                return Err(anyhow!("Timed out waiting for a MediaTek device."));
            }
        };

        info!("Found MTK port: {}", port.get_port_name());

        let mut builder = DeviceBuilder::default().with_mtk_port(port);
        if let Some(da) = da_data {
            builder = builder.with_da_data(da);
        }
        if let Some(pl) = pl_data {
            builder = builder.with_preloader(pl);
        }
        if let Some(auth) = auth_data {
            builder = builder.with_auth(auth);
        }

        let mut device = builder.build().map_err(|e| anyhow!("{}", friendly(&e)))?;
        device.init().map_err(|e| anyhow!("{}", friendly(&e)))?;

        let chip = device.chip();
        let chip_name = chip.name().to_string();
        let hw_code = chip.hw_code();

        let tgt = device.dev_info.target_config();
        info!(
            "Connected. chip={}, hw=0x{:04X}, SBC={}, SLA={}, DAA={}",
            chip_name,
            hw_code,
            (tgt & 0x1) != 0,
            (tgt & 0x2) != 0,
            (tgt & 0x4) != 0
        );

        self.device = Some(device);
        self.set_status(ConnStatus::Connected { chip_name, hw_code });
        Ok(())
    }

    fn load_pgpt(&mut self) -> Result<()> {
        let device = self.device.as_mut().ok_or_else(|| anyhow!("No device connected"))?;
        info!("Entering DA mode...");
        device.enter_da_mode().map_err(|e| anyhow!("{}", friendly(&e)))?;
        let partitions = device.get_partitions();
        info!("Loaded {} partitions from the PGPT.", partitions.len());
        self.send(Event::PartitionsLoaded { partitions });
        Ok(())
    }

    fn read_partitions(&mut self, names: Vec<String>, output_dir: PathBuf) -> Result<()> {
        let evt_tx = self.evt_tx.clone();
        let cancel = self.cancel.clone();
        let device = self.device.as_mut().ok_or_else(|| anyhow!("No device connected"))?;
        device.enter_da_mode().map_err(|e| anyhow!("{}", friendly(&e)))?;
        create_dir_all(&output_dir)?;

        for name in names {
            if cancel.swap(false, Ordering::SeqCst) {
                warn!("Read cancelled before '{name}'");
                return Err(anyhow!("Cancelled by user"));
            }

            let Some(part) = device.dev_info.get_partition(&name) else {
                let msg = format!("Partition '{name}' not found, skipping.");
                error!("{msg}");
                let _ = evt_tx.send(Event::Error(msg));
                continue;
            };

            let out_path = output_dir.join(format!("{}.bin", part.name));
            info!(
                "Reading '{}' (size={} bytes, addr=0x{:X}) -> {}",
                part.name,
                part.size,
                part.address,
                out_path.display()
            );

            let _ = evt_tx.send(Event::ProgressStart {
                total_bytes: part.size as u64,
                message: format!("Reading {}", part.name),
            });

            let file = File::create(&out_path)?;
            let mut writer = BufWriter::new(file);
            let mut callback = Self::make_progress_callback(evt_tx.clone(), "Reading flash");

            match device.read_partition(&part.name, &mut writer, &mut callback) {
                Ok(_) => {
                    writer.flush().ok();
                    let _ = evt_tx.send(Event::ProgressFinish {
                        message: format!("Read '{}' OK", part.name),
                    });
                }
                Err(e) => {
                    let _ = evt_tx.send(Event::ProgressFinish {
                        message: format!("Read '{}' FAILED", part.name),
                    });
                    return Err(anyhow!("{}", friendly(&e)));
                }
            }
        }

        Ok(())
    }

    fn write_assigned(&mut self, assignments: Vec<(String, PathBuf)>) -> Result<()> {
        let evt_tx = self.evt_tx.clone();
        let cancel = self.cancel.clone();
        let device = self.device.as_mut().ok_or_else(|| anyhow!("No device connected"))?;
        device.enter_da_mode().map_err(|e| anyhow!("{}", friendly(&e)))?;

        for (name, path) in assignments {
            if cancel.swap(false, Ordering::SeqCst) {
                warn!("Write cancelled before '{name}'");
                return Err(anyhow!("Cancelled by user"));
            }

            let Some(part) = device.dev_info.get_partition(&name) else {
                let msg = format!("Partition '{name}' not found, skipping.");
                error!("{msg}");
                let _ = evt_tx.send(Event::Error(msg));
                continue;
            };

            let file_size = metadata(&path)?.len();
            let total = file_size.min(part.size as u64);

            info!("Writing '{}' <- {} ({} bytes)", part.name, path.display(), total,);

            let _ = evt_tx.send(Event::ProgressStart {
                total_bytes: total,
                message: format!("Writing {}", part.name),
            });

            let mut reader = BufReader::new(OpenOptions::new().read(true).open(&path)?);
            let mut callback = Self::make_progress_callback(evt_tx.clone(), "Writing flash");

            // Use the WRITE-PARTITION (download) path rather than WRITE-FLASH:
            // SP Flash Tool flashes firmware files via this command, and on
            // locked / hardened bootloaders (e.g. Transsion: Tecno / Infinix /
            // Itel) it is the only write path that survives the on-device
            // security checks. WRITE-FLASH targets a raw region and the DA
            // rejects the per-partition handshake before any data flows.
            match device.download(&part.name, total as usize, &mut reader, &mut callback) {
                Ok(_) => {
                    let _ = evt_tx.send(Event::ProgressFinish {
                        message: format!("Wrote '{}' OK", part.name),
                    });
                }
                Err(e) => {
                    let _ = evt_tx.send(Event::ProgressFinish {
                        message: format!("Write '{}' FAILED", part.name),
                    });
                    return Err(anyhow!("{}", friendly(&e)));
                }
            }
        }

        Ok(())
    }

    fn seccfg(&mut self, action: LockAction) -> Result<()> {
        let label = match action {
            LockAction::Lock => "Locking bootloader",
            LockAction::Unlock => "Unlocking bootloader",
        };
        info!("{label}...");
        self.send(Event::ProgressStart { total_bytes: 1, message: label.to_string() });

        let ok = {
            let device = self.device.as_mut().ok_or_else(|| anyhow!("No device connected"))?;
            device.enter_da_mode().map_err(|e| anyhow!("{}", friendly(&e)))?;
            let flag = match action {
                LockAction::Lock => LockFlag::Lock,
                LockAction::Unlock => LockFlag::Unlock,
            };
            device.set_seccfg_lock_state(flag).is_some()
        };

        self.send(Event::ProgressFinish {
            message: if ok {
                format!("{label} OK")
            } else {
                format!("{label} failed or already in that state")
            },
        });

        if ok {
            info!("{label} OK");
        } else {
            warn!("{label} failed or already in that state");
        }
        Ok(())
    }

    /// Erase the standard "factory reset" partitions: userdata, metadata, persist.
    ///
    /// Uses `Device::format` (which sends `ErasePartition` xmlcmd, the
    /// name-based path) rather than `Device::erase_partition` (which sends
    /// `EraseFlash` for a raw region). The name-based path is in the same
    /// family as `WritePartition` / `download`, so it goes through on
    /// hardened bootloaders (e.g. Transsion: Tecno / Infinix / Itel) that
    /// reject the raw-region erase path with a security error.
    ///
    /// Skips partitions that aren't present in the PGPT (some devices don't
    /// have `metadata` or `persist`). Stops on the first hard error so the
    /// user sees exactly which one the DA refused.
    fn wipe_data(&mut self) -> Result<()> {
        let evt_tx = self.evt_tx.clone();
        let cancel = self.cancel.clone();
        let device = self.device.as_mut().ok_or_else(|| anyhow!("No device connected"))?;
        device.enter_da_mode().map_err(|e| anyhow!("{}", friendly(&e)))?;

        const TARGETS: &[&str] = &["userdata", "metadata", "persist"];
        let present: Vec<&str> =
            TARGETS.iter().copied().filter(|n| device.dev_info.get_partition(n).is_some()).collect();

        if present.is_empty() {
            return Err(anyhow!(
                "None of userdata / metadata / persist were found in the device's partition table."
            ));
        }

        info!("Wiping data: {}", present.join(", "));

        for name in present {
            if cancel.swap(false, Ordering::SeqCst) {
                warn!("Wipe cancelled before '{name}'");
                return Err(anyhow!("Cancelled by user"));
            }

            let _ = evt_tx.send(Event::ProgressStart {
                total_bytes: 1,
                message: format!("Erasing {name}"),
            });

            let mut callback = Self::make_progress_callback(evt_tx.clone(), "Erasing");
            match device.format(name, &mut callback) {
                Ok(_) => {
                    info!("Erased '{name}' OK");
                    let _ =
                        evt_tx.send(Event::ProgressFinish { message: format!("Erased '{name}' OK") });
                }
                Err(e) => {
                    let _ = evt_tx.send(Event::ProgressFinish {
                        message: format!("Erase '{name}' FAILED"),
                    });
                    return Err(anyhow!("{}", friendly(&e)));
                }
            }
        }

        Ok(())
    }

    fn reboot(&mut self, mode: BootMode) -> Result<()> {
        let device = self.device.as_mut().ok_or_else(|| anyhow!("No device connected"))?;
        info!("Rebooting (mode={:?})...", mode);
        device.reboot(mode).map_err(|e| anyhow!("{}", friendly(&e)))?;
        self.device = None;
        self.set_status(ConnStatus::Disconnected);
        Ok(())
    }

    fn shutdown(&mut self) -> Result<()> {
        let device = self.device.as_mut().ok_or_else(|| anyhow!("No device connected"))?;
        info!("Shutting down device...");
        device.shutdown().map_err(|e| anyhow!("{}", friendly(&e)))?;
        self.device = None;
        self.set_status(ConnStatus::Disconnected);
        Ok(())
    }
}

fn run(cmd_rx: Receiver<Command>, evt_tx: Sender<Event>, cancel: Arc<AtomicBool>) {
    let mut worker = Worker { device: None, evt_tx, cancel };

    while let Ok(cmd) = cmd_rx.recv() {
        // Drain any stale cancel flags before starting a fresh command.
        worker.cancel.store(false, Ordering::SeqCst);

        match &cmd {
            Command::Cancel => {
                // Cancel is handled inline above via the flag; the enum variant
                // is also used as a no-op "drain the queue" signal.
                drain_pending(&cmd_rx);
                continue;
            }
            Command::Disconnect => {
                worker.device = None;
                worker.set_status(ConnStatus::Disconnected);
                continue;
            }
            _ => {}
        }

        worker.set_input(false);
        let result: Result<()> = match cmd {
            Command::Connect { da_path, preloader_path, auth_path } => {
                worker.connect(da_path, preloader_path, auth_path)
            }
            Command::LoadPgpt => worker.load_pgpt(),
            Command::ReadPartitions { names, output_dir } => {
                worker.read_partitions(names, output_dir)
            }
            Command::WriteAssigned { assignments } => worker.write_assigned(assignments),
            Command::Seccfg(action) => worker.seccfg(action),
            Command::WipeData => worker.wipe_data(),
            Command::Reboot(mode) => worker.reboot(mode),
            Command::Shutdown => worker.shutdown(),
            Command::Cancel | Command::Disconnect => Ok(()),
        };
        if let Err(e) = result {
            worker.fail(e);
        }
        worker.set_input(true);
    }
}

fn drain_pending(rx: &Receiver<Command>) {
    while let Ok(_cmd) = rx.try_recv() {
        // Intentionally drop pending commands.
    }
}
