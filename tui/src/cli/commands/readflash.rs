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
use tokio::io::{AsyncWriteExt, BufWriter};

use crate::cli::MtkCommand;
use crate::cli::common::{CONN_DA, CommandMetadata};
use crate::cli::helpers::AntumbraProgress;
use crate::cli::state::PersistedDeviceState;

#[derive(Args, Debug)]
pub struct ReadArgs {
    /// The partition to read
    pub partition: String,
    /// The destination file
    pub output_file: PathBuf,
}

impl CommandMetadata for ReadArgs {
    fn visible_aliases() -> &'static [&'static str] {
        &["rf"]
    }

    fn about() -> &'static str {
        "Read a partition from the device and save it to a file."
    }

    fn long_about() -> &'static str {
        "Read a specified partition from the device and save it to a file with the given output filename."
    }
}

#[async_trait]
impl MtkCommand for ReadArgs {
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
                pb.update(written as u64, "Reading flash");

                if written >= total {
                    pb.finish("Read complete!");
                }
            }
        };

        let file = File::create(&self.output_file).await?;
        let mut writer = BufWriter::new(file);

        match dev.read_partition(&self.partition, &mut progress_callback, &mut writer).await {
            Ok(_) => {}
            Err(e) => {
                pb.abandon("Read failed!");
                return Err(e)?;
            }
        };

        writer.flush().await?;

        Ok(())
    }
}
