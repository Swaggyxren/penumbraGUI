/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2025 Shomy
*/

use anyhow::Result;
use async_trait::async_trait;
use clap::{Args, ValueEnum};
use penumbra::Device;
use penumbra::da::protocol::BootMode;

use crate::cli::MtkCommand;
use crate::cli::common::{CONN_DA, CommandMetadata};
use crate::cli::state::PersistedDeviceState;

#[derive(Debug, ValueEnum, Clone)]
pub enum RebootAction {
    Normal,
    HomeScreen,
    Fastboot,
    Meta,
    Test,
}

impl CommandMetadata for RebootArgs {
    fn about() -> &'static str {
        "Reboot the device into a specified mode."
    }

    fn long_about() -> &'static str {
        "Reboot the device into a specified mode. On XFlash and Legacy, only Normal, HomeScreen
        and Fastboot modes are supported, the rest will default to Normal.
        On XML, also the Meta and Test modes are available."
    }
}

impl From<RebootAction> for BootMode {
    fn from(action: RebootAction) -> Self {
        match action {
            RebootAction::Normal => BootMode::Normal,
            RebootAction::HomeScreen => BootMode::HomeScreen,
            RebootAction::Fastboot => BootMode::Fastboot,
            RebootAction::Test => BootMode::Test,
            RebootAction::Meta => BootMode::Meta,
        }
    }
}

#[derive(Args, Debug)]
pub struct RebootArgs {
    #[arg(value_enum, default_value_t = RebootAction::Normal)]
    pub action: RebootAction,
}

#[async_trait]
impl MtkCommand for RebootArgs {
    async fn run(&self, dev: &mut Device, state: &mut PersistedDeviceState) -> Result<()> {
        dev.enter_da_mode().await?;

        state.connection_type = CONN_DA;
        state.flash_mode = 1;

        let bootmode: BootMode = self.action.clone().into();
        dev.reboot(bootmode).await?;

        Ok(())
    }
}
