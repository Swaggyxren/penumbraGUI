/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2025-2026 Shomy
*/
use std::io::{Read, Write};

use log::debug;

use crate::core::storage::{PartitionKind, is_pl_part};
use crate::da::Xml;
use crate::da::xml::cmds::{
    ErasePartition, FileSystemOp, ReadPartition, WritePartition, XmlCmdLifetime,
};
use crate::da::xml::{EraseFlash, ReadFlash, WriteFlash};
use crate::error::Result;

pub fn upload<F, W>(xml: &mut Xml, part_name: &str, writer: W, progress: F) -> Result<()>
where
    W: Write,
    F: FnMut(usize, usize) + Send,
{
    debug!("Starting readback of partition '{}'", part_name);

    xmlcmd!(xml, ReadPartition, part_name, part_name)?;

    let read = xml.upload_file(writer, progress)?;
    xml.lifetime_ack(XmlCmdLifetime::CmdEnd)?;

    debug!("Upload completed, 0x{:X} bytes received.", read);

    Ok(())
}

pub fn read_flash<F, W>(
    xml: &mut Xml,
    addr: u64,
    size: usize,
    section: PartitionKind,
    writer: W,
    progress: F,
) -> Result<()>
where
    W: Write,
    F: FnMut(usize, usize) + Send,
{
    debug!("Reading flash at address {:#X} with size {:#X}", addr, size);

    xmlcmd!(xml, ReadFlash, section.as_str(), section.as_str(), size, addr)?;
    xml.upload_file(writer, progress)?;
    xml.lifetime_ack(XmlCmdLifetime::CmdEnd)?;

    debug!("Flash read completed, 0x{:X} bytes read.", size);

    Ok(())
}

pub fn download<F, R>(
    xml: &mut Xml,
    part_name: &str,
    size: usize,
    reader: R,
    progress: F,
) -> Result<()>
where
    R: Read,
    F: FnMut(usize, usize) + Send,
{
    debug!("Starting download to partition '{}' with size {:#X}", part_name, size);

    xmlcmd!(xml, WritePartition, part_name, part_name)?;
    // Progress report is not needed for PL partitions,
    // because the DA skips the erase process for them.
    if !is_pl_part(part_name) {
        let mock_progress = |_: usize, _: usize| {};
        xml.progress_report(mock_progress)?;
    }

    xml.file_system_op(FileSystemOp::Exists)?;
    xml.file_system_op(FileSystemOp::Exists)?;

    xml.download_file(size, reader, progress)?;
    xml.lifetime_ack(XmlCmdLifetime::CmdEnd)?;

    debug!("Download completed, {:#X} bytes sent.", size);

    Ok(())
}

pub fn write_flash<F, R>(
    xml: &mut Xml,
    addr: u64,
    size: usize,
    section: PartitionKind,
    reader: R,
    progress: F,
) -> Result<()>
where
    R: Read,
    F: FnMut(usize, usize) + Send,
{
    debug!("Writing flash at address {:#X} with size {:#X}", addr, size);

    xmlcmd!(xml, WriteFlash, section.as_str(), size, addr)?;

    xml.file_system_op(FileSystemOp::FileSize(size))?;
    xml.progress_report(|_, _| {})?; // Pre-erase
    xml.download_file(size, reader, progress)?;
    xml.lifetime_ack(XmlCmdLifetime::CmdEnd)?;

    debug!("Flash write completed, 0x{:X} bytes written.", size);

    Ok(())
}

pub fn format<F>(xml: &mut Xml, part_name: &str, progress: F) -> Result<()>
where
    F: FnMut(usize, usize) + Send,
{
    debug!("Formatting partition '{}'", part_name);

    xmlcmd!(xml, ErasePartition, part_name)?;
    xml.progress_report(progress)?;

    xml.lifetime_ack(XmlCmdLifetime::CmdEnd)?;

    debug!("Partition '{}' formatted.", part_name);

    Ok(())
}

pub fn erase_flash<F>(
    xml: &mut Xml,
    addr: u64,
    size: usize,
    section: PartitionKind,
    progress: F,
) -> Result<()>
where
    F: FnMut(usize, usize) + Send,
{
    debug!("Erasing flash at address {:#X} with size {:#X}", addr, size);

    xmlcmd!(xml, EraseFlash, section.as_str(), size, addr)?;
    xml.progress_report(progress)?;
    xml.lifetime_ack(XmlCmdLifetime::CmdEnd)?;

    debug!("Flash erase completed.");

    Ok(())
}
