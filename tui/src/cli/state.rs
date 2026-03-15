/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2025 Shomy
*/
use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::fs::{metadata, read, remove_file, write};

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct PersistedDeviceState {
    pub da_file_path: Option<String>,
    pub soc_id: Vec<u8>,
    pub meid: Vec<u8>,
    pub hw_code: u16,
    pub target_config: u32,
    pub connection_type: u8,
    pub flash_mode: u8,
    pub usb_log: bool,
}

impl PersistedDeviceState {
    const STATE_FILE: &'static str = ".antumbra_state";

    /// Loads the state from the `.antumbra_state` file.
    /// Returns default state if file doesn't exist or parsing fails.
    pub async fn load() -> Self {
        match read(Self::STATE_FILE).await {
            Ok(json) => serde_json::from_slice(&json).unwrap_or_default(),
            Err(_) => PersistedDeviceState::default(),
        }
    }

    /// Saves the current state to the `.antumbra_state` file.
    pub async fn save(&self) -> Result<()> {
        let json = serde_json::to_vec_pretty(self)?;
        write(Self::STATE_FILE, json)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to write state file: {}", e))?;
        Ok(())
    }

    /// Resets the current state and deletes the persisted file if it exists.
    pub async fn reset(&mut self) -> Result<()> {
        if metadata(Self::STATE_FILE).await.is_ok() {
            remove_file(Self::STATE_FILE).await?;
        }
        *self = PersistedDeviceState::default();
        Ok(())
    }
}
