/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2025 Shomy
*/
use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;
use clap::Args;
use penumbra::Device;
use tokio::fs::{File, metadata};
use tokio::io::BufReader;

use crate::cli::MtkCommand;
use crate::cli::common::{CONN_DA, CommandMetadata};
use crate::cli::helpers::AntumbraProgress;
use crate::cli::state::PersistedDeviceState;

#[derive(Args, Debug)]
pub struct WriteArgs {
    /// The partition to flash
    pub partition: String,
    /// The file to download
    pub file: PathBuf,
}

impl CommandMetadata for WriteArgs {
    fn visible_aliases() -> &'static [&'static str] {
        &["wf"]
    }

    fn about() -> &'static str {
        "Write a file to a specified partition on the device."
    }

    fn long_about() -> &'static str {
        "Write (flash) a file to a specificed partition on the device.
        If this command fails, use `download` instead."
    }
}

#[async_trait]
impl MtkCommand for WriteArgs {
    async fn run(&self, dev: &mut Device, state: &mut PersistedDeviceState) -> Result<()> {
        dev.enter_da_mode().await?;

        state.connection_type = CONN_DA;
        state.flash_mode = 1;

        let file = File::open(&self.file).await?;
        let mut reader = BufReader::new(file);

        let file_size = metadata(&self.file).await?.len();

        let part_size = match dev.dev_info.get_partition(&self.partition).await {
            Some(p) => p.size as u64,
            None => {
                return Err(anyhow::anyhow!("Partition '{}' not found on device.", self.partition));
            }
        };

        let total_size = file_size.min(part_size);
        let pb = AntumbraProgress::new(total_size);

        let mut progress_callback = {
            let pb = &pb;
            move |written: usize, total: usize| {
                pb.update(written as u64, "Writing flash");

                if written >= total {
                    pb.finish("Write complete!");
                }
            }
        };

        match dev.write_partition(&self.partition, &mut reader, &mut progress_callback).await {
            Ok(_) => {}
            Err(e) => {
                pb.abandon("Write failed!");
                return Err(e)?;
            }
        }

        Ok(())
    }
}
