/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2025 Shomy
*/

use anyhow::Result;
use async_trait::async_trait;
use clap::Args;
use human_bytes::human_bytes;
use log::info;
use penumbra::Device;

use crate::cli::MtkCommand;
use crate::cli::common::{CONN_DA, CommandMetadata};
use crate::cli::state::PersistedDeviceState;

#[derive(Args, Debug)]
pub struct PgptArgs;

impl CommandMetadata for PgptArgs {
    fn visible_aliases() -> &'static [&'static str] {
        &["gpt"]
    }

    fn about() -> &'static str {
        "Display the partition table of the connected device."
    }

    fn long_about() -> &'static str {
        Self::about()
    }
}

#[async_trait]
impl MtkCommand for PgptArgs {
    async fn run(&self, dev: &mut Device, state: &mut PersistedDeviceState) -> Result<()> {
        dev.enter_da_mode().await?;

        state.connection_type = CONN_DA;
        state.flash_mode = 1;

        let partitions = dev.dev_info.partitions().await;

        info!("Partition Table:");
        for p in partitions {
            info!(
                "Name: {:<15} \t Addr: 0x{:08X} \t Size: 0x{:08X} ({})",
                p.name,
                p.address,
                p.size,
                human_bytes(p.size as f64)
            );
        }

        Ok(())
    }
}
