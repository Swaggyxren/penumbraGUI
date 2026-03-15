/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2025 Shomy
*/

use anyhow::Result;
use async_trait::async_trait;
use clap::Args;
use log::info;
use penumbra::Device;

use crate::cli::MtkCommand;
use crate::cli::common::{CONN_DA, CommandMetadata};
use crate::cli::helpers::AntumbraProgress;
use crate::cli::state::PersistedDeviceState;

#[derive(Args, Debug)]
pub struct FormatArgs {
    /// The partition to format
    pub partition: String,
}

impl CommandMetadata for FormatArgs {
    fn visible_aliases() -> &'static [&'static str] {
        &["ft"]
    }

    fn about() -> &'static str {
        "Format a partition on the device."
    }

    fn long_about() -> &'static str {
        "Format (erase) the specified partition on the device."
    }
}

#[async_trait]
impl MtkCommand for FormatArgs {
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

        let pb = AntumbraProgress::new(partition.size as u64);

        let mut progress_callback = {
            let pb = &pb;
            move |written: usize, total: usize| {
                pb.update(written as u64, "Formatting...");

                if written >= total {
                    pb.finish("Format complete!");
                }
            }
        };

        match dev.format(&self.partition, &mut progress_callback).await {
            Ok(_) => {}
            Err(e) => {
                pb.abandon("Format failed!");
                return Err(e)?;
            }
        }

        Ok(())
    }
}
