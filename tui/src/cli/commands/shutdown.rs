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
use crate::cli::state::PersistedDeviceState;

#[derive(Args, Debug)]
pub struct ShutdownArgs {}

impl CommandMetadata for ShutdownArgs {
    fn about() -> &'static str {
        "Shutdown the device."
    }

    fn long_about() -> &'static str {
        "Shutdown the device through DA mode."
    }
}

#[async_trait]
impl MtkCommand for ShutdownArgs {
    async fn run(&self, dev: &mut Device, state: &mut PersistedDeviceState) -> Result<()> {
        dev.enter_da_mode().await?;

        state.connection_type = CONN_DA;
        state.flash_mode = 1;

        dev.shutdown().await?;
        info!("Device shutdown successfully.");

        Ok(())
    }
}
