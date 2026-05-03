/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2026 Shomy
*/

//! Parser for SP Flash Tool style MediaTek scatter layout files.
//!
//! Handles both the YAML-ish `.txt` variant and the equivalent `.xml` variant
//! shipped by the same firmware. The parser is intentionally lenient and
//! stdlib-only — it only extracts the fields the GUI needs to render the
//! table and call `Device::download`.

use std::fs;
use std::path::Path;

#[derive(Clone, Debug, Default)]
pub struct ScatterFile {
    pub platform: Option<String>,
    pub project: Option<String>,
    pub storage: Option<String>,
    pub block_size: Option<u64>,
    pub entries: Vec<ScatterEntry>,
}

#[derive(Clone, Debug, Default)]
pub struct ScatterEntry {
    pub index: String,
    pub name: String,
    pub file_name: String,
    pub is_download: bool,
    pub partition_size: u64,
    pub region: String,
    pub kind: String,
    /// Which `storage_type` section the entry belongs to (e.g. "EMMC", "UFS").
    pub storage_type: String,
}

pub fn parse_from_path(path: &Path) -> Result<ScatterFile, String> {
    let bytes = fs::read(path).map_err(|e| format!("Failed to read scatter file: {e}"))?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let trimmed = text.trim_start();
    if trimmed.starts_with("<?xml") || trimmed.starts_with("<root") {
        parse_xml(&text)
    } else {
        parse_txt(&text)
    }
}

fn parse_size(s: &str) -> u64 {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).unwrap_or(0)
    } else {
        s.parse::<u64>().unwrap_or(0)
    }
}

fn parse_bool(s: &str) -> bool {
    matches!(s.trim().to_ascii_lowercase().as_str(), "true" | "1" | "yes")
}

fn parse_txt(text: &str) -> Result<ScatterFile, String> {
    let mut file = ScatterFile::default();
    let mut current: Option<ScatterEntry> = None;
    let mut active_storage_type = String::new();

    for raw in text.lines() {
        let line = raw.split_once('#').map(|(l, _)| l).unwrap_or(raw);
        let trimmed = line.trim_start();
        let trimmed = trimmed.trim_start_matches("- ").trim();
        if trimmed.is_empty() {
            continue;
        }

        let Some((k, v)) = trimmed.split_once(':') else {
            continue;
        };
        let key = k.trim();
        let value = v.trim();

        if key == "storage_type" {
            if let Some(e) = current.take() {
                file.entries.push(e);
            }
            active_storage_type = value.to_string();
            continue;
        }

        if key == "partition_index" {
            if let Some(e) = current.take() {
                file.entries.push(e);
            }
            current = Some(ScatterEntry {
                index: value.to_string(),
                storage_type: active_storage_type.clone(),
                ..Default::default()
            });
            continue;
        }

        if let Some(e) = current.as_mut() {
            match key {
                "partition_name" => e.name = value.to_string(),
                "file_name" => e.file_name = value.to_string(),
                "is_download" => e.is_download = parse_bool(value),
                "partition_size" => e.partition_size = parse_size(value),
                "region" => e.region = value.to_string(),
                "type" => e.kind = value.to_string(),
                _ => {}
            }
        } else {
            match key {
                "platform" => file.platform = Some(value.to_string()),
                "project" => file.project = Some(value.to_string()),
                "storage" if file.storage.is_none() => {
                    file.storage = Some(value.to_string());
                }
                "block_size" => file.block_size = Some(parse_size(value)),
                _ => {}
            }
        }
    }

    if let Some(e) = current.take() {
        file.entries.push(e);
    }

    if file.entries.is_empty() {
        return Err("No partitions found in scatter file.".into());
    }
    Ok(file)
}

fn parse_xml(text: &str) -> Result<ScatterFile, String> {
    let mut file = ScatterFile::default();
    let mut current: Option<ScatterEntry> = None;
    let mut in_partition = false;
    let mut active_storage_type = String::new();

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with("<storage_type ") {
            if let Some(n) = extract_name_attr(line) {
                active_storage_type = n;
            }
            continue;
        }

        if line.starts_with("<partition_index ") {
            if let Some(idx) = extract_name_attr(line) {
                current = Some(ScatterEntry {
                    index: idx,
                    storage_type: active_storage_type.clone(),
                    ..Default::default()
                });
                in_partition = true;
            }
            continue;
        }
        if line == "</partition_index>" {
            if let Some(e) = current.take() {
                file.entries.push(e);
            }
            in_partition = false;
            continue;
        }

        // Record storage name from e.g. `<storage name="EMMC">`.
        if !in_partition && line.starts_with("<storage ") && file.storage.is_none() {
            if let Some(n) = extract_name_attr(line) {
                file.storage = Some(n);
            }
            continue;
        }

        let Some((tag, val)) = extract_simple(line) else {
            continue;
        };

        if in_partition {
            if let Some(e) = current.as_mut() {
                match tag {
                    "partition_name" => e.name = val.to_string(),
                    "file_name" => e.file_name = val.to_string(),
                    "is_download" => e.is_download = parse_bool(val),
                    "partition_size" => e.partition_size = parse_size(val),
                    "region" => e.region = val.to_string(),
                    "type" => e.kind = val.to_string(),
                    _ => {}
                }
            }
        } else {
            match tag {
                "platform" => file.platform = Some(val.to_string()),
                "project" => file.project = Some(val.to_string()),
                "block_size" => file.block_size = Some(parse_size(val)),
                _ => {}
            }
        }
    }

    if file.entries.is_empty() {
        return Err("No partitions found in scatter XML.".into());
    }
    Ok(file)
}

fn extract_name_attr(line: &str) -> Option<String> {
    let key = "name=\"";
    let start = line.find(key)? + key.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn extract_simple(line: &str) -> Option<(&str, &str)> {
    let after_lt = line.strip_prefix('<')?;
    let (tag, rest) = after_lt.split_once('>')?;
    if tag.contains(' ') || tag.starts_with('/') {
        return None;
    }
    let close = format!("</{tag}>");
    let value = rest.strip_suffix(&close)?;
    Some((tag, value))
}

/// Region name used by SP Flash Tool for preloader partitions. These live in
/// the EMMC boot regions (not the GPT-addressable user area) so the GUI can't
/// flash them through `Device::download`; we skip those rows.
pub fn is_preloader_region(region: &str) -> bool {
    let r = region.to_ascii_uppercase();
    r.contains("BOOT1") || r.contains("BOOT_1") || r == "EMMC_BOOT1_BOOT2"
}
