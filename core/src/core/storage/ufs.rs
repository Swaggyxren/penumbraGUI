/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2025 Shomy
*/
use async_trait::async_trait;

use crate::core::storage::{PartitionKind, Storage, StorageType};
use crate::error::{Error, Result};
use crate::utilities::xml::{get_tag, get_tag_usize};

#[derive(Debug)]
pub struct UfsInfo {
    pub kind: u32,
    pub block_size: u32,
    pub lu0_size: u64,
    pub lu1_size: u64,
    pub lu2_size: u64,
    pub lu3_size: u64,
    pub cid: Vec<u8>,
    pub fwver: Vec<u8>,
    pub serial: Vec<u8>,
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UfsPartition {
    /// Fallback case, should not be used
    Unknown = 0,
    /// Logical Unit 0, usually preloader
    Lu0 = 1,
    /// Logical Unit 1, usually preloader backup
    Lu1 = 2,
    /// Logical Unit 2, same as USER from EMMC
    Lu2 = 3,
    /// Logical Unit 3, RPMB
    Lu3 = 4,
    Lu4 = 5,
    Lu5 = 6,
    Lu6 = 7,
    Lu7 = 8,
    /// Both Logical Unit 0 and Logical Unit 1
    Lu0Lu1 = 9,
}

impl UfsPartition {
    pub fn as_str(&self) -> &'static str {
        match self {
            UfsPartition::Lu0 => "UFS-LUA0",
            UfsPartition::Lu1 => "UFS-LUA1",
            UfsPartition::Lu2 => "UFS-LUA2",
            UfsPartition::Lu3 => "UFS-LUA3",
            UfsPartition::Lu4 => "UFS-LUA4",
            UfsPartition::Lu5 => "UFS-LUA5",
            UfsPartition::Lu6 => "UFS-LUA6",
            UfsPartition::Lu7 => "UFS-LUA7",
            UfsPartition::Lu0Lu1 => "UFS-LUA0LUA1",
            UfsPartition::Unknown => "UFS-UNKNOWN", // Assumed to be unreachable
        }
    }
}

pub struct UfsStorage {
    pub info: UfsInfo,
}

#[async_trait]
impl Storage for UfsStorage {
    fn kind(&self) -> StorageType {
        StorageType::Ufs
    }

    fn block_size(&self) -> u32 {
        self.info.block_size
    }

    fn total_size(&self) -> u64 {
        self.info.lu2_size
    }

    fn get_user_part(&self) -> PartitionKind {
        PartitionKind::Ufs(UfsPartition::Lu2)
    }

    fn get_pl_part1(&self) -> PartitionKind {
        PartitionKind::Ufs(UfsPartition::Lu0)
    }

    fn get_pl_part2(&self) -> PartitionKind {
        PartitionKind::Ufs(UfsPartition::Lu1)
    }

    fn get_pl1_size(&self) -> u64 {
        self.info.lu0_size
    }

    fn get_pl2_size(&self) -> u64 {
        self.info.lu1_size
    }

    fn get_user_size(&self) -> u64 {
        self.info.lu2_size
    }

    fn get_rpmb_size(&self) -> u64 {
        self.info.lu3_size
    }
}

impl UfsStorage {
    pub fn from_response(data: &[u8]) -> Result<Self> {
        if data.len() < 0xA8 {
            return Err(Error::io("UFS response data too short"));
        }

        let mut pos = 0;

        // 0x30 == UFS
        let kind = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap());
        let block_size = u32::from_le_bytes(data[pos + 4..pos + 8].try_into().unwrap());
        pos += 8;

        let lu0_size = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;
        let lu1_size = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;
        let lu2_size = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;
        let lu3_size = 0u64; // Thank you MTK

        let cid = data[pos..pos + 16].to_vec();
        pos += 16;

        let fwver = data[pos + 0x16..pos + 0x1A].to_vec();
        let serial = data[pos + 0x1E..pos + 0x2A].to_vec();

        Ok(UfsStorage {
            info: UfsInfo {
                kind,
                block_size,
                lu0_size,
                lu1_size,
                lu2_size,
                lu3_size,
                cid,
                fwver,
                serial,
            },
        })
    }

    pub fn from_xml_response(xml: &str) -> Result<Self> {
        let block_size = get_tag_usize(xml, "ufs/block_size")? as u32;
        let lu0_size = get_tag_usize(xml, "ufs/lua0_size")? as u64;
        let lu1_size = get_tag_usize(xml, "ufs/lua1_size")? as u64;
        let lu2_size = get_tag_usize(xml, "ufs/lua2_size")? as u64;
        let lu3_size = get_tag_usize(xml, "ufs/lua3_size").unwrap_or(0) as u64;

        // Older devices use ufs_cid, newer ones use id
        let cid_str: String = get_tag(xml, "ufs/ufs_cid").or_else(|_| get_tag(xml, "ufs/id"))?;
        let cid = hex::decode(cid_str.trim_start_matches("0x"))
            .map_err(|_| Error::io("Failed to parse UFS CID from XML"))?;

        Ok(UfsStorage {
            info: UfsInfo {
                kind: 0x30,
                block_size,
                lu0_size,
                lu1_size,
                lu2_size,
                lu3_size,
                cid,
                fwver: Vec::new(),
                serial: Vec::new(),
            },
        })
    }
}
