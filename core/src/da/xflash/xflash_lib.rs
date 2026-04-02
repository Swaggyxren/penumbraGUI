/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2025 Shomy
*/
use std::io::{Read, Write};
use std::sync::Arc;

use log::{debug, error, info, trace, warn};

use crate::connection::Connection;
use crate::connection::port::ConnectionType;
use crate::core::auth::{AuthManager, SignData, SignPurpose, SignRequest};
use crate::core::devinfo::DeviceInfo;
use crate::core::emi::extract_emi_settings;
use crate::core::log_buffer::DeviceLog;
use crate::core::storage::Storage;
use crate::da::protocol::{DAProtocolParams, DataType, PacketHeader};
use crate::da::xflash::cmds::*;
#[cfg(not(feature = "no_exploits"))]
use crate::da::xflash::exts::boot_extensions;
use crate::da::xflash::storage::detect_storage;
use crate::da::{DA, DownloadProtocol};
use crate::error::{Error, Result, XFlashError};
use crate::le_u32;

pub struct XFlash {
    pub conn: Connection,
    pub da: DA,
    pub pl: Option<Vec<u8>>,
    pub dev_info: DeviceInfo,
    pub(super) using_exts: bool,
    pub(super) read_packet_length: Option<usize>,
    pub(super) write_packet_length: Option<usize>,
    pub(super) patch: bool,
    pub(super) verbose: bool,
    pub(super) usb_log_channel: bool,
    pub(super) device_log: DeviceLog,
}

impl XFlash {
    pub fn send_cmd(&mut self, cmd: Cmd) -> Result<bool> {
        let cmd_bytes = (cmd as u32).to_le_bytes();
        debug!("[TX] Sending Command: 0x{:08X}", cmd as u32);
        self.send(&cmd_bytes[..])
    }

    pub fn new(conn: Connection, params: DAProtocolParams) -> Self {
        XFlash {
            conn,
            da: params.da,
            pl: params.preloader,
            dev_info: params.devinfo,
            using_exts: false,
            read_packet_length: None,
            write_packet_length: None,
            patch: true,
            verbose: params.verbose,
            usb_log_channel: params.usb_log_channel,
            device_log: params.device_log,
        }
    }

    // Note: When called with multiple params, this function sends data only and does not read any
    // response. For that, call read_data separately and check status manually.
    // This is to accomodate the protocol, while also not breaking read_data for other operations.
    pub fn devctrl(&mut self, cmd: Cmd, params: Option<&[&[u8]]>) -> Result<Vec<u8>> {
        self.send_cmd(Cmd::DeviceCtrl)?;
        self.send_cmd(cmd)?;

        if let Some(p) = params {
            self.send_data(p)?;
            return Ok(Vec::new());
        }

        let read = self.read_data();
        status_ok!(self);

        read
    }

    fn read_next_flow_header(&mut self) -> Result<PacketHeader> {
        loop {
            let mut buf = [0u8; PacketHeader::SIZE];
            // Blocking read with a timeout using std (optional: implement timeout if needed)
            self.conn.read(&mut buf)?;

            let hdr = PacketHeader::from_bytes(&buf).ok_or_else(|| {
                debug!("[RX] Invalid packet header bytes: {:02X?}", buf);
                Error::io(format!("Invalid packet header: {:02X?}", buf))
            })?;

            match hdr.data_type {
                DataType::Flow => return Ok(hdr),
                DataType::Message => self.drain_message(hdr.length)?,
            }
        }
    }

    fn drain_message(&mut self, length: u32) -> Result<()> {
        let mut payload = vec![0u8; length as usize];
        self.conn.read(&mut payload)?;

        let body = String::from_utf8_lossy(&payload[4..]).into_owned();

        trace!("[DA Message] {}", body);

        if self.usb_log_channel {
            self.device_log.push(body);
        }

        Ok(())
    }

    // When called after calling a cmd that returns a status too,
    // call status_ok!() macro manually.
    // This function only reads the data, and cannot be used to read status,
    // or functions like read_flash will fail.
    pub fn read_data(&mut self) -> Result<Vec<u8>> {
        let hdr = self.read_next_flow_header()?;

        debug!("[RX] Packet header received: 0x{:X} bytes", hdr.length);

        let mut data = vec![0u8; hdr.length as usize];
        self.conn.read(&mut data)?;
        Ok(data)
    }

    pub(super) fn upload_stage1(
        &mut self,
        addr: u32,
        length: u32,
        data: Vec<u8>,
        sig_len: u32,
    ) -> Result<bool> {
        info!(
            "[Penumbra] Uploading DA1 region to address 0x{:08X} with length 0x{:X}",
            addr, length
        );

        self.conn.send_da(&data, length, addr, sig_len)?;
        info!("[Penumbra] Sent DA1, jumping to address 0x{:08X}...", addr);
        self.conn.jump_da(addr)?;

        let sync_byte = {
            let mut sync_buf = [0u8; 1];
            match self.conn.read(&mut sync_buf) {
                Ok(_) => sync_buf[0],
                Err(e) => return Err(Error::io(e.to_string())),
            }
        };

        info!("[Penumbra] Received sync byte");

        if sync_byte != 0xC0 {
            return Err(Error::proto("Incorrect sync byte received"));
        }

        let hdr = self.generate_header(&[0u8; 4]);
        self.conn.write(&hdr)?;
        self.conn.write(&(Cmd::SyncSignal as u32).to_le_bytes())?;

        // We can only set the environment parameters once, and for whatever reason if we set the
        // log level to DEBUG and try to send EMI settings in BROM mode, the DA hangs. This
        // appears to be a MediaTek quirk as usual. As a workaround, we always use INFO
        // level when in BROM mode, even if verbose logging is requested.
        let da_log_level: u32 = if self.verbose && self.conn.connection_type != ConnectionType::Brom
        {
            1 // DEBUG
        } else {
            2 // INFO
        };

        let log_channel: u32 = 1 + self.usb_log_channel as u32;

        // Force logs through UART regardless.
        // For some reason, as the error mentioned above, the DA just hangs if we set logs
        // through USB, regardless of the bootup mode.
        // TODO: Figure out why!
        let env_params: [u32; 5] = [
            da_log_level, // da_log_level
            log_channel,  // log_channel = 1: UART, 2: Usb, 3: Both
            1,            // system_os = OS_LINUX
            0,            // ufs_provision
            0,            // reserved
        ];
        let mut env_buf = [0u8; 20];
        for (i, v) in env_params.iter().enumerate() {
            env_buf[i * 4..(i + 1) * 4].copy_from_slice(&v.to_le_bytes());
        }

        self.send_data(&[&(Cmd::SetupEnvironment as u32).to_le_bytes(), &env_buf])?;

        self.send_data(&[&(Cmd::SetupHwInitParams as u32).to_le_bytes(), &[0u8; 4]])?;

        status_any!(self, Cmd::SyncSignal as u32);

        info!("[Penumbra] Received DA1 sync signal.");

        self.handle_emi()?;
        self.devctrl(Cmd::SetChecksumLevel, Some(&[&0u32.to_le_bytes()]))?;

        Ok(true)
    }

    #[cfg(not(feature = "no_exploits"))]
    pub(super) fn boot_extensions(&mut self) -> Result<bool> {
        if self.using_exts {
            warn!("DA extensions already in use, skipping re-upload");
            return Ok(true);
        }
        info!("Booting DA extensions...");
        self.using_exts = boot_extensions(self)?;
        Ok(true)
    }

    // This is an internal helper, do not use it directly
    pub(super) fn get_or_detect_storage(&mut self) -> Option<Arc<dyn Storage>> {
        if let Some(storage) = self.dev_info.storage() {
            return Some(storage);
        }

        if let Some(storage) = detect_storage(self) {
            self.dev_info.set_storage(storage.clone());
            return Some(storage);
        }

        None
    }

    /// Receives data from the device, writing it to the provided writer.
    /// Common loop for `read_flash` and `upload`.
    pub fn upload_data<W, F>(&mut self, size: usize, mut writer: W, mut progress: F) -> Result<()>
    where
        W: Write + Send,
        F: FnMut(usize, usize) + Send,
    {
        let mut bytes_read = 0;
        progress(0, size);
        loop {
            let chunk = self.read_data()?;
            if chunk.is_empty() {
                debug!("No data received, breaking.");
                break;
            }

            writer.write_all(&chunk)?;
            bytes_read += chunk.len();

            self.send(&[0u8; 4])?;

            progress(bytes_read, size);

            if bytes_read >= size {
                debug!("Requested size read. Breaking.");
                break;
            }

            debug!("Read {:X}/{:X} bytes...", bytes_read, size);
        }

        Ok(())
    }

    /// Sends data to the device from the provided reader.
    /// Common loop for `write_flash` and `download`.
    ///
    /// If we receive less data than requested from the reader,
    /// we pad the remaining bytes with 0s and send it anyway.
    pub fn download_data<R, F>(&mut self, size: usize, reader: R, progress: F) -> Result<()>
    where
        R: Read + Send,
        F: FnMut(usize, usize) + Send,
    {
        let chunk_size = self.write_packet_length.unwrap_or(0x8000);
        self.download_data_with(size, chunk_size, reader, progress)
    }

    /// Same as `download_data`, but with a custom chunk size.
    /// Useful for limiting the packet size when needed.
    pub fn download_data_with<R, F>(
        &mut self,
        size: usize,
        chunk_size: usize,
        mut reader: R,
        mut progress: F,
    ) -> Result<()>
    where
        R: Read + Send,
        F: FnMut(usize, usize) + Send,
    {
        let mut buffer = vec![0u8; chunk_size];
        let mut bytes_written = 0;

        progress(0, size);
        while bytes_written < size {
            // It is mandatory to make data size the same as size, or we will be leaving
            // older data in the partition. Usually, this is not an issue for partitions
            // with an header, like LK (which stores the start and length of the lk image),
            // but for other partitions, this might make the partition unusable.
            // This issue only arises when flashing stuff that is not coming from a dump made
            // with read_flash() or any other tool like mtkclient.
            let remaining = size - bytes_written;
            let to_read = remaining.min(chunk_size);

            let bytes_read = reader.read(&mut buffer[..to_read])?;
            let chunk = if bytes_read == 0 {
                &buffer[..to_read]
            } else if bytes_read < to_read {
                buffer[bytes_read..to_read].fill(0);
                &buffer[..to_read]
            } else {
                &buffer[..to_read]
            };

            // DA expects a checksum of the data chunk before the actual data
            // The actual checksum is a additive 16-bit checksum (Good job MTK!!)
            // For whoever is reading this code and has no clue what this is doing:
            // Just sum all bytes then AND with 0xFFFF :D!!!
            let checksum = chunk.iter().fold(0u32, |total, &byte| total + byte as u32) & 0xFFFF;
            self.send_data(&[&0u32.to_le_bytes(), &checksum.to_le_bytes(), chunk])?;

            bytes_written += chunk.len();
            progress(bytes_written, size);
            debug!("Written {}/{} bytes...", bytes_written, size);
        }

        status_ok!(self);

        Ok(())
    }

    pub fn progress_report<F>(&mut self, size: usize, mut progress: F) -> Result<()>
    where
        F: FnMut(usize, usize) + Send,
    {
        progress(0, size);
        loop {
            let status = self.read_data()?;
            if le_u32!(status, 0) == 0x40040005 {
                progress(size, size);
                break;
            }

            let status = self.read_data()?;
            let progress_percent = le_u32!(status, 0);

            // The device doesn't send statuses during erase/format, so we have to send
            // an acknowledgment manually through the port and not through send()
            let ack = [0u8; 4];
            let hdr = self.generate_header(&ack);
            self.conn.write(&hdr)?;
            self.conn.write(&ack)?;

            let progress_bytes = (progress_percent as usize * size) / 100;
            progress(progress_bytes, size);
        }

        Ok(())
    }

    pub(super) fn generate_header(&self, data: &[u8]) -> [u8; PacketHeader::SIZE] {
        let hdr = PacketHeader::new(data.len() as u32);
        debug!("[TX] Packet header sent: 0x{:X} bytes", data.len());
        hdr.to_bytes()
    }

    fn handle_emi(&mut self) -> Result<()> {
        let conn_agent = self.devctrl(Cmd::GetConnectionAgent, None)?;

        // If the connection agent is "preloader", there's no need to upload EMI settings
        if conn_agent == b"preloader" {
            return Ok(());
        }

        let pl = self
            .pl
            .as_ref()
            .ok_or_else(|| Error::penumbra("Device is in BROM but no preloader was provided!"))?;

        let emi = extract_emi_settings(pl)
            .ok_or_else(|| Error::penumbra("Failed to extract EMI settings from preloader!"))?;

        info!("[Penumbra] Uploading EMI settings to device...");
        self.send_cmd(Cmd::InitExtRam)?;
        self.send_data(&[&(emi.len() as u32).to_le_bytes(), emi.as_slice()])?;
        info!("[Penumbra] EMI settings uploaded successfully.");

        Ok(())
    }

    pub(super) fn handle_sla(&mut self) -> Result<bool> {
        let resp = match self.devctrl(Cmd::SlaEnabledStatus, None) {
            Ok(r) => r,
            Err(_) => {
                // The CMD might not be supported on some devices, so we just assume SLA is disabled
                return Ok(true);
            }
        };

        let sla_enabled = le_u32!(resp, 0) != 0;

        if !sla_enabled {
            return Ok(true);
        }

        info!("DA SLA is enabled");

        let da2_data = match self.da.get_da2() {
            Some(da2) => da2.data.clone(),
            None => Vec::new(),
        };

        let auth = AuthManager::get();
        if !auth.can_sign(&da2_data) {
            #[cfg(not(feature = "no_exploits"))]
            {
                info!("No available signers for DA SLA, trying dummy signature...");
                let dummy_sig = vec![0u8; 256];
                if self.devctrl(Cmd::SetRemoteSecPolicy, Some(&[&dummy_sig])).is_ok() {
                    info!("DA SLA signature accepted (dummy)!");
                    return Ok(true);
                }
            }

            error!("No signer available for DA SLA! Can't proceed.");
            return Err(Error::penumbra(
                "DA SLA is enabled, but no signer is available. Can't continue.",
            ));
        }

        const HEADER: usize = 4;
        const RND_LEN: usize = 0x10;
        const HRID_LEN: usize = 0x10;
        const SOC_ID_LEN: usize = 0x20;

        let firmware_info = self.devctrl(Cmd::GetDevFwInfo, None)?;
        debug!("Firmware Info: {:02X?}", firmware_info);

        let rnd = &firmware_info[HEADER..HEADER + RND_LEN];

        let hrid = firmware_info.get(HEADER + RND_LEN..HEADER + RND_LEN + HRID_LEN).unwrap_or(&[]);

        let soc_id = firmware_info
            .get(HEADER + RND_LEN + HRID_LEN..HEADER + RND_LEN + HRID_LEN + SOC_ID_LEN)
            .unwrap_or(&[]);

        let sign_data = SignData {
            rnd: rnd.to_vec(),
            hrid: hrid.to_vec(),
            soc_id: soc_id.to_vec(),
            raw: firmware_info.to_vec(),
        };
        let sign_req =
            SignRequest { data: sign_data, purpose: SignPurpose::DaSla, pubk_mod: da2_data };

        info!("Found signer for DA SLA!");
        let signed_rnd = auth.sign(&sign_req)?;
        info!("Signed DA SLA challenge. Uploading to device...");
        self.devctrl(Cmd::SetRemoteSecPolicy, Some(&[&signed_rnd]))?;
        info!("DA SLA signature accepted!");
        Ok(true)
    }
}
