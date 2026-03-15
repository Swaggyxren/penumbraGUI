/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2025 Shomy
*/
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use clap::Args;
use log::info;
use penumbra::Device;
use tokio::fs::{File, create_dir_all, read_dir};
use tokio::io::{AsyncWriteExt, BufWriter};

use crate::cli::MtkCommand;
use crate::cli::common::{CONN_DA, CommandMetadata};
use crate::cli::helpers::AntumbraProgress;
use crate::cli::state::PersistedDeviceState;

#[derive(Args, Debug)]
pub struct ReadAllArgs {
    /// The partition to read
    pub output_dir: PathBuf,
    /// The destination file
    #[arg(long, short = 's', value_delimiter = ',')]
    pub skip: Vec<String>,
}

impl CommandMetadata for ReadAllArgs {
    fn visible_aliases() -> &'static [&'static str] {
        &["rl"]
    }

    fn about() -> &'static str {
        "Read all partitions from the device and save them to the specified output directory."
    }

    fn long_about() -> &'static str {
        "Read all partitions from the device and save them to the specified output directory,
        skipping any partitions listed in the skip option."
    }
}

#[async_trait]
impl MtkCommand for ReadAllArgs {
    async fn run(&self, dev: &mut Device, state: &mut PersistedDeviceState) -> Result<()> {
        let output_dir: &Path = &self.output_dir;

        if let Err(e) = create_dir_all(output_dir).await {
            return Err(anyhow!(
                "Failed to create output directory '{}': {}",
                output_dir.display(),
                e
            ));
        }

        let mut dir_entries = read_dir(output_dir).await?;
        if dir_entries.next_entry().await?.is_some() {
            return Err(anyhow!("Output directory '{}' is not empty", output_dir.display()));
        }

        dev.enter_da_mode().await?;

        state.connection_type = CONN_DA;
        state.flash_mode = 1;

        let partitions = dev.get_partitions().await;
        if partitions.is_empty() {
            info!("No partitions found on device.");
            return Ok(());
        }

        let proto = dev.get_protocol().ok_or(anyhow!("Failed to get device protocol"))?;

        for p in partitions {
            if self.skip.contains(&p.name) {
                info!("Skipping partition '{}'", p.name);
                continue;
            }

            let output_path = self.output_dir.join(format!("{}.bin", p.name));
            let mut output_file = BufWriter::new(File::create(&output_path).await?);

            let part_size = p.size as u64;
            let pb = AntumbraProgress::new(part_size);

            let mut progress_callback = {
                let pb = &pb;
                move |read: usize, total: usize| {
                    pb.update(read as u64, "Reading...");

                    if read >= total {
                        pb.finish("Read complete!");
                    }
                }
            };

            match proto
                .read_flash(p.address, p.size, p.kind, &mut progress_callback, &mut output_file)
                .await
            {
                Ok(_) => {}
                Err(_) => {
                    pb.abandon("Read failed! Skipping partition.");
                }
            }

            output_file.flush().await?;
            info!("Saved partition '{}' to '{}'", p.name, output_path.display());
        }

        info!("All partitions read successfully.");

        Ok(())
    }
}
