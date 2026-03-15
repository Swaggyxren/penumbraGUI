/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2025 Shomy
*/
use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;
use clap::Args;
use log::info;
use penumbra::Device;
use tokio::fs::File;
use tokio::io::BufWriter;

use crate::cli::MtkCommand;
use crate::cli::common::{CONN_DA, CommandMetadata};
use crate::cli::helpers::AntumbraProgress;
use crate::cli::state::PersistedDeviceState;

#[derive(Args, Debug)]
pub struct UploadArgs {
    /// The partition to read
    pub partition: String,
    /// The destination file
    pub output_file: PathBuf,
}

impl CommandMetadata for UploadArgs {
    fn visible_aliases() -> &'static [&'static str] {
        &["up", "read", "r"]
    }

    fn about() -> &'static str {
        "Upload a partition from the device to the host."
    }

    fn long_about() -> &'static str {
        "Upload (readback) a specificed partition on the device to a file on the host.
        Use this command for reading back if the `read` command fails."
    }
}

#[async_trait]
impl MtkCommand for UploadArgs {
    async fn run(&self, dev: &mut Device, state: &mut PersistedDeviceState) -> Result<()> {
        dev.enter_da_mode().await?;

        state.connection_type = CONN_DA;
        state.flash_mode = 1;

        let partition = match dev.dev_info.get_partition(&self.partition).await {
            Some(p) => p,
            None => {
                info!("Partition '{}' not found on device.", self.partition);
                return Err(anyhow::anyhow!("Partition '{}' not found on device.", self.partition));
            }
        };

        let total_size = partition.size as u64;
        let pb = AntumbraProgress::new(total_size);

        let mut progress_callback = {
            let pb = &pb;
            move |written: usize, total: usize| {
                pb.update(written as u64, "Uploading...");

                if written >= total {
                    pb.finish("Upload complete!");
                }
            }
        };

        let file = File::create(&self.output_file).await?;
        let mut writer = BufWriter::new(file);

        match dev.upload(&self.partition, &mut writer, &mut progress_callback).await {
            Ok(_) => {}
            Err(e) => {
                pb.abandon("Upload failed!");
                return Err(e)?;
            }
        };

        Ok(())
    }
}
