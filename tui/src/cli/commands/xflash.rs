/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2026 Shomy
*/

use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;
use clap::{Args, Subcommand};
use log::info;
use penumbra::Device;
use penumbra::da::XFlash;
use penumbra::da::xflash::flash::set_rsc_info;
use tokio::fs::{File, metadata};
use tokio::io::BufReader;

use crate::cli::MtkCommand;
use crate::cli::common::{CONN_DA, CommandMetadata};
use crate::cli::helpers::AntumbraProgress;
use crate::cli::state::PersistedDeviceState;

#[derive(Args, Debug)]
pub struct RscFlashArgs {
    /// Partition to flash
    pub partition: String,
    /// File to flash
    pub file: PathBuf,
}

#[async_trait]
impl MtkCommand for RscFlashArgs {
    async fn run(&self, dev: &mut Device, state: &mut PersistedDeviceState) -> Result<()> {
        dev.enter_da_mode().await?;
        state.connection_type = CONN_DA;
        state.flash_mode = 1;

        info!("Flashing file {:?} to partition {} with RSC", self.file, self.partition);

        let file = File::open(&self.file).await?;
        let mut reader = BufReader::new(file);

        let file_size = metadata(&self.file).await?.len();

        let part_size = match dev.dev_info.get_partition(&self.partition).await {
            Some(p) => p.size as u64,
            None => {
                return Err(anyhow::anyhow!("Partition '{}' not found on device.", self.partition));
            }
        };

        if file_size > part_size {
            return Err(anyhow::anyhow!(
                "File size ({}) exceeds partition size ({}).",
                file_size,
                part_size
            ));
        }

        let proto = dev.get_protocol().unwrap();
        let xflash = proto
            .as_any_mut()
            .downcast_mut::<XFlash>()
            .ok_or_else(|| anyhow::anyhow!("Current protocol is not XFlash"))?;

        let pb = AntumbraProgress::new(file_size);

        let mut progress_callback = {
            let pb = &pb;
            move |written: usize, total: usize| {
                pb.update(written as u64, "Flashing...");

                if written >= total {
                    pb.finish("Flash complete!");
                }
            }
        };

        set_rsc_info(
            xflash,
            &self.partition,
            file_size as usize,
            &mut reader,
            &mut progress_callback,
        )
        .await?;

        info!("Flashing to partition '{}' completed.", self.partition);

        Ok(())
    }
}

#[derive(Debug, Subcommand)]
pub enum XFlashSubcommand {
    RscFlash(RscFlashArgs),
}

#[derive(Args, Debug)]
pub struct XFlashArgs {
    #[command(subcommand)]
    pub command: XFlashSubcommand,
}

impl CommandMetadata for XFlashArgs {
    fn visible_aliases() -> &'static [&'static str] {
        &["xf"]
    }

    fn about() -> &'static str {
        "XFlash-specific commands."
    }

    fn long_about() -> &'static str {
        "Commands specific to XFlash / V5 devices."
    }
}

#[async_trait]
impl MtkCommand for XFlashArgs {
    async fn run(&self, dev: &mut Device, state: &mut PersistedDeviceState) -> Result<()> {
        match &self.command {
            XFlashSubcommand::RscFlash(cmd) => cmd.run(dev, state).await,
        }
    }
}
