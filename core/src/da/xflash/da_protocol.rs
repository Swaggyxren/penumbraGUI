/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2025 Shomy
*/
use std::io::{Cursor, Read, Write};
use std::sync::Arc;

use log::{debug, error, info};

use crate::connection::Connection;
use crate::connection::port::ConnectionType;
use crate::core::devinfo::DeviceInfo;
use crate::core::seccfg::LockFlag;
use crate::core::storage::{Gpt, Partition, PartitionKind, RpmbRegion, Storage, StorageType};
use crate::da::protocol::BootMode;
use crate::da::xflash::cmds::*;
#[cfg(not(feature = "no_exploits"))]
use crate::da::xflash::exts::{
    auth_rpmb,
    peek,
    poke,
    read_rpmb,
    read32_ext,
    write_rpmb,
    write32_ext,
};
use crate::da::xflash::flash;
#[cfg(not(feature = "no_exploits"))]
use crate::da::xflash::patch;
#[cfg(not(feature = "no_exploits"))]
use crate::da::xflash::sec::{parse_seccfg, write_seccfg};
use crate::da::{DA, DAEntryRegion, DownloadProtocol, XFlash};
use crate::error::{Error, Result, XFlashError};
#[cfg(not(feature = "no_exploits"))]
use crate::exploit::{Carbonara, Exploit, Kamakiri};
use crate::{exploit, le_u32};

impl DownloadProtocol for XFlash {
    fn upload_da(&mut self) -> Result<bool> {
        exploit!(Kamakiri, self);

        let da1 = self.da.get_da1().ok_or_else(|| Error::penumbra("DA1 region not found"))?;
        self.upload_stage1(da1.addr, da1.length, da1.data.clone(), da1.sig_len)
            .map_err(|e| Error::proto(format!("Failed to upload DA1: {}", e)))?;

        flash::get_packet_length(self)?;

        exploit!(Carbonara, self);

        let da2 = self.da.get_da2().ok_or_else(|| Error::penumbra("DA2 region not found"))?;
        let sig_len = da2.sig_len as usize;
        let da2data = da2.data[..da2.data.len().saturating_sub(sig_len)].to_vec();

        info!(
            "[Penumbra] Uploading DA2 to address 0x{:08X} with size 0x{:X} bytes",
            da2.addr,
            da2data.len()
        );

        match self.boot_to(da2.addr, &da2data) {
            Ok(true) => {
                info!("[Penumbra] Successfully uploaded and executed DA2");
                self.handle_sla()?;
                flash::get_packet_length(self)?; // Re-query packet length for DA loop, for faster speeds :)

                #[cfg(not(feature = "no_exploits"))]
                self.boot_extensions()?;

                Ok(true)
            }
            Ok(false) => Err(Error::proto("Failed to execute DA2")),
            Err(e) => {
                self.reboot(BootMode::Normal).ok();
                Err(Error::proto(format!("Error uploading DA2: {}", e)))
            }
        }
    }

    fn boot_to(&mut self, addr: u32, data: &[u8]) -> Result<bool> {
        self.send_cmd(Cmd::BootTo)?;

        // Addr (LE) | Length (LE)
        // 00000040000000002c83050000000000 -> addr=0x4000000, len=0x0005832c
        let mut param = [0u8; 16];
        param[0..8].copy_from_slice(&(addr as u64).to_le_bytes());
        param[8..16].copy_from_slice(&(data.len() as u64).to_le_bytes());

        self.send_data(&[&param, data])?;

        status_any!(self, 0, Cmd::SyncSignal as u32);

        Ok(true)
    }

    fn send_data(&mut self, data: &[&[u8]]) -> Result<bool> {
        let mut hdr: [u8; 12];

        for param in data {
            hdr = self.generate_header(param);

            self.conn.write(&hdr)?;

            let mut pos = 0;
            let max_chunk_size = self.write_packet_length.unwrap_or(0x8000);

            while pos < param.len() {
                let end = param.len().min(pos + max_chunk_size);
                let chunk = &param[pos..end];
                debug!("[TX] Sending chunk (0x{:X} bytes)", chunk.len());
                self.conn.write(chunk)?;
                pos = end;
            }

            debug!("[TX] Completed sending 0x{:X} bytes", param.len());
        }

        status_ok!(self);

        Ok(true)
    }

    fn get_status(&mut self) -> Result<u32> {
        let data = self.read_data()?;

        if data.is_empty() {
            debug!("[RX] Status: empty data");
            return Err(Error::XFlash(XFlashError::from_code(0xFFFFFFFF)));
        }

        let status = le_u32!(data, 0);

        debug!("[RX] Status: 0x{:08X}", status);
        match status {
            0 => Ok(status),
            sync if sync == Cmd::SyncSignal as u32 => Ok(status),
            _ => Err(Error::XFlash(XFlashError::from_code(status))),
        }
    }

    fn send(&mut self, data: &[u8]) -> Result<bool> {
        self.send_data(&[data])
    }

    fn shutdown(&mut self) -> Result<()> {
        self.send_cmd(Cmd::Shutdown)?;

        let params: [u32; 7] = [
            0, // is_dev_reboot (0 = shutdown)
            0, // timeout_ms (unused when not rebooting)
            0, // async
            0, // bootup
            0, // dlbit
            0, // bNotResetRTCTime (0 = reset RTC)
            0, // bNotDisconnectUSB (0 = disconnect USB)
        ];

        let mut buf = [0u8; 28];
        for (i, v) in params.iter().enumerate() {
            buf[i * 4..(i + 1) * 4].copy_from_slice(&v.to_le_bytes());
        }

        info!("Shutting down device...");

        self.send(&buf)?;

        self.conn.port.close().ok();
        Ok(())
    }

    fn reboot(&mut self, bootmode: BootMode) -> Result<()> {
        self.send_cmd(Cmd::Shutdown)?;

        let bootup = match bootmode {
            BootMode::Normal => 0,
            BootMode::HomeScreen => 1,
            BootMode::Fastboot => 2,
            _ => 0,
        };

        let params: [u32; 7] = [
            1,      // is_dev_reboot
            0,      // timeout_ms (0 = default, WDT decides)
            0,      // async
            bootup, // bootup
            0,      // dlbit
            0,      // bNotResetRTCTime
            0,      // bNotDisconnectUSB
        ];

        let mut buf = [0u8; 28];
        for (i, v) in params.iter().enumerate() {
            buf[i * 4..(i + 1) * 4].copy_from_slice(&v.to_le_bytes());
        }

        info!("Rebooting device into {:?} mode...", bootmode);

        self.send(&buf)?;

        self.conn.port.close().ok();
        Ok(())
    }

    fn read_flash<W, F>(
        &mut self,
        addr: u64,
        size: usize,
        section: PartitionKind,
        progress: F,
        writer: W,
    ) -> Result<()>
    where
        W: Write + Send,
        F: FnMut(usize, usize) + Send,
    {
        flash::read_flash(self, addr, size, section, progress, writer)
    }

    fn write_flash<R, F>(
        &mut self,
        addr: u64,
        size: usize,
        mut reader: R,
        section: PartitionKind,
        mut progress: F,
    ) -> Result<()>
    where
        R: Read + Send,
        F: FnMut(usize, usize) + Send,
    {
        flash::write_flash(self, addr, size, &mut reader, section, &mut progress)
    }

    fn erase_flash<F>(
        &mut self,
        addr: u64,
        size: usize,
        section: PartitionKind,
        mut progress: F,
    ) -> Result<()>
    where
        F: FnMut(usize, usize) + Send,
    {
        flash::erase_flash(self, addr, size, section, &mut progress)
    }

    fn download<R, F>(
        &mut self,
        part_name: String,
        size: usize,
        mut reader: R,
        mut progress: F,
    ) -> Result<()>
    where
        R: Read + Send,
        F: FnMut(usize, usize) + Send,
    {
        flash::download(self, part_name, size, &mut reader, &mut progress)
    }

    fn upload<W, F>(&mut self, part_name: String, mut writer: W, mut progress: F) -> Result<()>
    where
        W: Write + Send,
        F: FnMut(usize, usize) + Send,
    {
        flash::upload(self, part_name, &mut writer, &mut progress)
    }

    fn format<F>(&mut self, part_name: String, mut progress: F) -> Result<()>
    where
        F: FnMut(usize, usize) + Send,
    {
        flash::format(self, part_name, &mut progress)
    }

    fn get_usb_speed(&mut self) -> Result<u32> {
        let usb_speed = self.devctrl(Cmd::GetUsbSpeed, None)?;
        debug!("USB Speed Data: {:?}", usb_speed);
        Ok(le_u32!(usb_speed, 0))
    }

    fn get_connection(&mut self) -> &mut Connection {
        &mut self.conn
    }

    fn set_connection_type(&mut self, conn_type: ConnectionType) -> Result<()> {
        self.conn.connection_type = conn_type;
        Ok(())
    }

    fn read32(&mut self, addr: u32) -> Result<u32> {
        #[cfg(not(feature = "no_exploits"))]
        if self.using_exts {
            return read32_ext(self, addr);
        }
        debug!("Reading 32-bit register at address 0x{:08X}", addr);
        let param = addr.to_le_bytes();
        let resp = self.devctrl(Cmd::DeviceCtrlReadRegister, Some(&[&param]))?;
        debug!("[RX] Read Register Response: {:02X?}", resp);
        if resp.len() < 4 {
            debug!("Short read: expected 4 bytes, got {}", resp.len());
            return Err(Error::io("Short register read"));
        }
        Ok(le_u32!(resp, 0))
    }

    fn write32(&mut self, addr: u32, value: u32) -> Result<()> {
        #[cfg(not(feature = "no_exploits"))]
        if self.using_exts {
            return write32_ext(self, addr, value);
        }
        let mut param = [0u8; 8];
        param[0..4].copy_from_slice(&addr.to_le_bytes());
        param[4..8].copy_from_slice(&value.to_le_bytes());
        debug!("[TX] Writing 32-bit value 0x{:08X} to address 0x{:08X}", value, addr);
        self.devctrl(Cmd::SetRegisterValue, Some(&[&param]))?;
        Ok(())
    }

    fn get_storage_type(&mut self) -> StorageType {
        self.get_or_detect_storage().map_or(StorageType::Unknown, |s| s.kind())
    }

    fn get_storage(&mut self) -> Option<Arc<dyn Storage>> {
        self.get_or_detect_storage()
    }

    fn get_partitions(&mut self) -> Vec<Partition> {
        let storage = match self.get_storage() {
            Some(s) => s,
            None => {
                error!("[Penumbra] Failed to get storage for partition parsing");
                return Vec::new();
            }
        };

        let storage_type = storage.kind();
        let pl_part1 = storage.get_pl_part1();
        let pl_part2 = storage.get_pl_part2();
        let user_part = storage.get_user_part();
        let pl1_size = storage.get_pl1_size() as usize;
        let pl2_size = storage.get_pl2_size() as usize;
        let user_size = storage.get_user_size() as usize;
        let gpt_size = 32 * 1024; // TODO: Change this when adding NAND support and PMT

        let mut partitions = vec![
            Partition::new("preloader", pl1_size, 0, pl_part1),
            Partition::new("preloader_backup", pl2_size, 0, pl_part2),
            Partition::new("PGPT", gpt_size, 0, user_part),
        ];

        let sgpt = Partition::new("SGPT", gpt_size, user_size as u64 - gpt_size as u64, user_part);

        let progress = |_, _| {};

        let mut pgpt_data = Vec::new();
        let mut pgpt_cursor = Cursor::new(&mut pgpt_data);
        self.upload("PGPT".into(), &mut pgpt_cursor, progress).ok();
        self.send(&[0u8; 4]).ok();
        let parsed_gpt_parts =
            Gpt::parse(&pgpt_data, storage_type).map(|g| g.partitions()).unwrap_or_default();

        let mut gpt_parts = if !parsed_gpt_parts.is_empty() {
            parsed_gpt_parts
        } else {
            let mut sgpt_data = Vec::new();
            let mut sgpt_cursor = Cursor::new(&mut sgpt_data);
            self.upload("SGPT".into(), &mut sgpt_cursor, progress).ok();
            self.send(&[0u8; 4]).ok();
            Gpt::parse(&sgpt_data, storage_type).map(|g| g.partitions()).unwrap_or_default()
        };

        partitions.append(&mut gpt_parts);
        partitions.push(sgpt);

        partitions
    }

    #[cfg(not(feature = "no_exploits"))]
    fn set_seccfg_lock_state(&mut self, locked: LockFlag) -> Option<Vec<u8>> {
        let seccfg = parse_seccfg(self);
        if seccfg.is_none() {
            error!("[Penumbra] Failed to parse seccfg, cannot set lock state");
            return None;
        }

        let mut seccfg = seccfg.unwrap();
        seccfg.set_lock_state(locked);
        write_seccfg(self, &mut seccfg)
    }

    #[cfg(not(feature = "no_exploits"))]
    fn peek<W, F>(&mut self, addr: u32, length: usize, mut writer: W, mut progress: F) -> Result<()>
    where
        W: Write + Send,
        F: FnMut(usize, usize) + Send,
    {
        peek(self, addr, length, &mut writer, &mut progress)
    }

    #[cfg(not(feature = "no_exploits"))]
    fn poke<R, F>(&mut self, addr: u32, length: usize, mut reader: R, mut progress: F) -> Result<()>
    where
        R: Read + Send,
        F: FnMut(usize, usize) + Send,
    {
        poke(self, addr, length, &mut reader, &mut progress)
    }

    #[cfg(not(feature = "no_exploits"))]
    fn read_rpmb<W, F>(
        &mut self,
        region: RpmbRegion,
        start_sector: u32,
        sectors_count: u32,
        mut writer: W,
        mut progress: F,
    ) -> Result<()>
    where
        W: Write + Send,
        F: FnMut(usize, usize) + Send,
    {
        read_rpmb(self, region, start_sector, sectors_count, &mut writer, &mut progress)
    }

    #[cfg(not(feature = "no_exploits"))]
    fn write_rpmb<R, F>(
        &mut self,
        region: RpmbRegion,
        start_sector: u32,
        sectors_count: u32,
        mut reader: R,
        mut progress: F,
    ) -> Result<()>
    where
        R: Read + Send,
        F: FnMut(usize, usize) + Send,
    {
        write_rpmb(self, region, start_sector, sectors_count, &mut reader, &mut progress)
    }

    #[cfg(not(feature = "no_exploits"))]
    fn auth_rpmb(&mut self, region: RpmbRegion, key: &[u8]) -> Result<()> {
        auth_rpmb(self, region, key)
    }

    #[cfg(not(feature = "no_exploits"))]
    fn patch_da(&mut self) -> Option<DA> {
        patch::patch_da(self).ok()
    }

    #[cfg(not(feature = "no_exploits"))]
    fn patch_da1(&mut self) -> Option<DAEntryRegion> {
        patch::patch_da1(self).ok()
    }

    #[cfg(not(feature = "no_exploits"))]
    fn patch_da2(&mut self) -> Option<DAEntryRegion> {
        patch::patch_da2(self).ok()
    }

    fn get_devinfo(&self) -> &DeviceInfo {
        &self.dev_info
    }

    fn get_da(&self) -> &DA {
        &self.da
    }
}
