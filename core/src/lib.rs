/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2025-2026 Shomy
*/
pub mod connection;
pub mod core;
pub mod da;
pub mod device;
pub mod error;
#[cfg(not(feature = "no_exploits"))]
pub mod exploit;
pub mod macros;
pub mod utilities;

pub use core::auth::{AuthManager, SignData, SignPurpose, SignRequest, Signer};
pub use core::log_buffer::{DeviceLog, OnPush};
pub use core::seccfg::LockFlag;
pub use core::storage::{
    EmmcPartition, Gpt, Partition, PartitionKind, RpmbRegion, Storage, StorageKind, StorageType,
    UfsPartition,
};

pub use connection::port::{MTKPort, find_mtk_port};
pub use da::protocol::{BootMode, DAProtocol, DownloadProtocol};
pub use da::{DA, DAEntryRegion, DAFile, DAType, XFlash, Xml};
pub use device::{Device, DeviceBuilder};

const VERSION: &str = env!("CARGO_PKG_VERSION");
