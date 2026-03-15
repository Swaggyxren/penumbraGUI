/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2025 Shomy
*/

use anyhow::Result;
use async_trait::async_trait;
use clap::{Args, ValueEnum};
use log::info;
use penumbra::Device;
use penumbra::core::seccfg::LockFlag;

use crate::cli::MtkCommand;
use crate::cli::common::{CONN_DA, CommandMetadata};
use crate::cli::state::PersistedDeviceState;

#[derive(Debug, ValueEnum, Clone)]
pub enum SeccfgAction {
    Unlock,
    Lock,
}

#[derive(Args, Debug)]
pub struct SeccfgArgs {
    pub action: SeccfgAction,
}

impl CommandMetadata for SeccfgArgs {
    fn about() -> &'static str {
        "Lock or unlock the seccfg partition on the device."
    }

    fn long_about() -> &'static str {
        "Lock or unlock the seccfg partition on the device.
        This command only work when the device is in DA mode and vulnerable to an exploit or unfused,
        because it requires DA extensions to be loaded."
    }
}

#[async_trait]
impl MtkCommand for SeccfgArgs {
    async fn run(&self, dev: &mut Device, state: &mut PersistedDeviceState) -> Result<()> {
        dev.enter_da_mode().await?;

        state.connection_type = CONN_DA;
        state.flash_mode = 1;

        match self.action {
            SeccfgAction::Unlock => {
                info!("Unlocking seccfg...");
                match dev.set_seccfg_lock_state(LockFlag::Unlock).await {
                    Some(_) => (),
                    None => {
                        info!("Failed to unlock seccfg or already unlocked.");
                        return Ok(());
                    }
                }
                info!("Unlocked seccfg!");
            }
            SeccfgAction::Lock => {
                info!("Locking seccfg partition...");
                match dev.set_seccfg_lock_state(LockFlag::Lock).await {
                    Some(_) => (),
                    None => {
                        info!("Failed to lock seccfg or already locked.");
                        return Ok(());
                    }
                }
                info!("Locked seccfg!");
            }
        }

        Ok(())
    }
}
