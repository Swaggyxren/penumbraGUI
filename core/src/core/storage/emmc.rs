/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2025 Shomy
*/
use async_trait::async_trait;

use crate::core::storage::{PartitionKind, Storage, StorageType};
use crate::error::{Error, Result};
use crate::utilities::xml::{get_tag, get_tag_usize};

/// Represents eMMC storage information.
#[derive(Debug)]
pub struct EmmcInfo {
    /// eMMC kind (EMMC or SDMMC)
    pub kind: u32,
    /// eMMC block size in bytes.
    pub block_size: u32,
    /// Size of Boot1 section in bytes.
    pub boot1_size: u64,
    /// Size of Boot2 section in bytes.
    pub boot2_size: u64,
    /// Size of RPMB section in bytes.
    pub rpmb_size: u64,
    /// Size of GP1 in bytes.,
    pub gp1_size: u64,
    /// Size of GP2 in bytes.
    pub gp2_size: u64,
    /// Size of GP3 in bytes.
    pub gp3_size: u64,
    /// Size of GP4 in bytes.
    pub gp4_size: u64,
    /// Size of User section in bytes.
    pub user_size: u64,
    /// eMMC CID (Card Identification) register value.
    pub cid: Vec<u8>,
    /// eMMC firmware version.
    pub fwver: u64,
}

/// Represents eMMC partitions types.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmmcPartition {
    /// Boot1 partition, usually contains preloader.
    Boot1 = 1,
    /// Boot2 partition, usually contains preloader backup.
    Boot2 = 2,
    /// Replay Protected Memory Block partition, used for secure data storage.
    Rpmb = 3,
    /// General Purpose partition 1.
    Gp1 = 4,
    /// General Purpose partition 2.
    Gp2 = 5,
    /// General Purpose partition 3.
    Gp3 = 6,
    /// General Purpose partition 4.
    Gp4 = 7,
    /// User data partition, ths main storage area for user data and scatter partitions.
    User = 8,
    End = 9,
    /// Both Boot1 and Boot2 partitions.
    Boot1Boot2 = 10,
}

impl EmmcPartition {
    pub fn as_str(&self) -> &'static str {
        match self {
            EmmcPartition::Boot1 => "EMMC-BOOT1",
            EmmcPartition::Boot2 => "EMMC-BOOT2",
            EmmcPartition::Rpmb => "EMMC-RPMB",
            EmmcPartition::Gp1 => "EMMC-GP1",
            EmmcPartition::Gp2 => "EMMC-GP2",
            EmmcPartition::Gp3 => "EMMC-GP3",
            EmmcPartition::Gp4 => "EMMC-GP4",
            EmmcPartition::User => "EMMC-USER",
            EmmcPartition::End => "EMMC-END",
            EmmcPartition::Boot1Boot2 => "EMMC-BOOT1BOOT2",
        }
    }
}

/// Represents eMMC storage device.
pub struct EmmcStorage {
    /// eMMC storage information.
    pub info: EmmcInfo,
}

#[async_trait]
impl Storage for EmmcStorage {
    fn kind(&self) -> StorageType {
        StorageType::Emmc
    }

    fn block_size(&self) -> u32 {
        self.info.block_size
    }

    fn total_size(&self) -> u64 {
        self.info.user_size
            + self.info.boot1_size
            + self.info.boot2_size
            + self.info.rpmb_size
            + self.info.gp1_size
            + self.info.gp2_size
            + self.info.gp3_size
            + self.info.gp4_size
    }

    fn get_user_part(&self) -> PartitionKind {
        PartitionKind::Emmc(EmmcPartition::User)
    }

    fn get_pl_part1(&self) -> PartitionKind {
        PartitionKind::Emmc(EmmcPartition::Boot1)
    }

    fn get_pl_part2(&self) -> PartitionKind {
        PartitionKind::Emmc(EmmcPartition::Boot2)
    }

    fn get_pl1_size(&self) -> u64 {
        self.info.boot1_size
    }

    fn get_pl2_size(&self) -> u64 {
        self.info.boot2_size
    }

    fn get_user_size(&self) -> u64 {
        self.info.user_size
    }

    fn get_rpmb_size(&self) -> u64 {
        self.info.rpmb_size
    }
}

impl EmmcStorage {
    pub fn from_response(data: &[u8]) -> Result<Self> {
        if data.len() < 96 {
            return Err(Error::penumbra("Emmc response data too short"));
        }

        let mut pos = 0;
        let kind = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap());
        let block_size = u32::from_le_bytes(data[pos + 4..pos + 8].try_into().unwrap());

        pos += 8;

        let boot1_size = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        let boot2_size = u64::from_le_bytes(data[pos + 8..pos + 16].try_into().unwrap());
        let rpmb_size = u64::from_le_bytes(data[pos + 16..pos + 24].try_into().unwrap());
        let gp1_size = u64::from_le_bytes(data[pos + 24..pos + 32].try_into().unwrap());
        let gp2_size = u64::from_le_bytes(data[pos + 32..pos + 40].try_into().unwrap());
        let gp3_size = u64::from_le_bytes(data[pos + 40..pos + 48].try_into().unwrap());
        let gp4_size = u64::from_le_bytes(data[pos + 48..pos + 56].try_into().unwrap());
        let user_size = u64::from_le_bytes(data[pos + 56..pos + 64].try_into().unwrap());

        pos += 64;
        let cid = data[pos..pos + 16].to_vec();

        pos += 16;
        let fwver = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());

        Ok(EmmcStorage {
            info: EmmcInfo {
                kind,
                block_size,
                boot1_size,
                boot2_size,
                rpmb_size,
                gp1_size,
                gp2_size,
                gp3_size,
                gp4_size,
                user_size,
                cid,
                fwver,
            },
        })
    }

    pub fn from_xml_response(xml: &str) -> Result<Self> {
        let block_size = get_tag_usize(xml, "emmc/block_size")? as u32;

        let boot1_size = get_tag_usize(xml, "emmc/boot1_size")? as u64;
        let boot2_size = get_tag_usize(xml, "emmc/boot2_size")? as u64;
        let rpmb_size = get_tag_usize(xml, "emmc/rpmb_size")? as u64;
        let gp1_size = get_tag_usize(xml, "emmc/gp1_size")? as u64;
        let gp2_size = get_tag_usize(xml, "emmc/gp2_size")? as u64;
        let gp3_size = get_tag_usize(xml, "emmc/gp3_size")? as u64;
        let gp4_size = get_tag_usize(xml, "emmc/gp4_size")? as u64;
        let user_size = get_tag_usize(xml, "emmc/user_size")? as u64;

        let cid_str: String = get_tag(xml, "emmc/id")?;
        let cid = hex::decode(cid_str).map_err(|_| Error::penumbra("Failed to decode EmmcCid"))?;

        Ok(EmmcStorage {
            info: EmmcInfo {
                kind: 0x1,
                block_size,
                boot1_size,
                boot2_size,
                rpmb_size,
                gp1_size,
                gp2_size,
                gp3_size,
                gp4_size,
                user_size,
                cid,
                fwver: 0,
            },
        })
    }
}
