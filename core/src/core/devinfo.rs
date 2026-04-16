/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2026 Shomy
*/

use std::sync::{Arc, RwLock};

use crate::core::bootctrl::BootControl;
use crate::core::chip::{ChipInfo, UNKNOWN_CHIP};
use crate::core::storage::{Partition, StorageKind};

#[derive(Clone)]
pub struct DeviceInfo {
    data: Arc<RwLock<DevInfoData>>,
    chip: Arc<RwLock<&'static ChipInfo>>,
    storage: Arc<RwLock<Option<StorageKind>>>,
}

/// Struct holding device information data.
#[derive(Default, Clone)]
pub struct DevInfoData {
    pub soc_id: [u8; 32],
    pub meid: [u8; 16],
    pub hw_code: u16,
    pub partitions: Vec<Partition>,
    pub bootctrl: Option<BootControl>,
    pub target_config: u32,
}

impl DeviceInfo {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_data(&self) -> DevInfoData {
        self.data.read().unwrap().clone()
    }

    pub fn set_data(&self, data: DevInfoData) {
        *self.data.write().unwrap() = data;
    }

    pub fn chip(&self) -> &'static ChipInfo {
        *self.chip.read().unwrap()
    }

    pub fn set_chip(&self, chip: &'static ChipInfo) {
        *self.chip.write().unwrap() = chip;
    }
}

impl Default for DeviceInfo {
    fn default() -> Self {
        Self {
            data: Arc::new(RwLock::new(DevInfoData::default())),
            chip: Arc::new(RwLock::new(&UNKNOWN_CHIP)),
            storage: Arc::new(RwLock::new(None)),
        }
    }
}

impl DeviceInfo {
    pub fn soc_id(&self) -> [u8; 32] {
        self.data.read().unwrap().soc_id
    }

    pub fn meid(&self) -> [u8; 16] {
        self.data.read().unwrap().meid
    }

    pub fn hw_code(&self) -> u16 {
        self.data.read().unwrap().hw_code
    }

    pub fn partitions(&self) -> Vec<Partition> {
        self.data.read().unwrap().partitions.clone()
    }

    pub fn storage(&self) -> Option<StorageKind> {
        self.storage.read().unwrap().clone()
    }

    pub fn set_storage(&self, storage: StorageKind) {
        *self.storage.write().unwrap() = Some(storage);
    }

    pub fn get_partition(&self, name: &str) -> Option<Partition> {
        let data = self.data.read().unwrap();

        if let Some(p) = data.partitions.iter().find(|p| p.name.eq_ignore_ascii_case(name)) {
            return Some(p.clone());
        }

        log::info!("Partition '{}' not found, trying with bootctrl suffix", name);

        let Some(suffix) = self.get_bootctrl()?.get_current_suffix().map(|s| s.to_string()) else {
            log::error!("BOOTCTRL OR SUFFIX IS NONE");
            return None;
        };

        let suffixed_name = format!("{name}{suffix}");

        log::info!("Trying to find partition with suffixed name '{}'", suffixed_name);

        data.partitions.iter().find(|p| p.name.eq_ignore_ascii_case(&suffixed_name)).cloned()
    }

    pub fn set_partitions(&self, partitions: Vec<Partition>) {
        self.data.write().unwrap().partitions = partitions;
    }

    pub fn get_bootctrl(&self) -> Option<BootControl> {
        self.data.read().unwrap().bootctrl.clone()
    }

    pub fn set_bootctrl(&self, bctrl: BootControl) {
        self.data.write().unwrap().bootctrl = Some(bctrl)
    }

    pub fn target_config(&self) -> u32 {
        self.data.read().unwrap().target_config
    }

    pub fn set_target_config(&self, cfg: u32) {
        self.data.write().unwrap().target_config = cfg;
    }

    pub fn sbc_enabled(&self) -> bool {
        (self.data.read().unwrap().target_config & 0x1) != 0
    }

    pub fn sla_enabled(&self) -> bool {
        (self.data.read().unwrap().target_config & 0x2) != 0
    }

    pub fn daa_enabled(&self) -> bool {
        (self.data.read().unwrap().target_config & 0x4) != 0
    }
}
