/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2026 Shomy
*/

//! Plain-English mapping for [`penumbra::error::Error`] values.
//!
//! The core uses tagged variants (`Io(...)`, `Protocol(...)`, etc.) which is
//! great for programmatic handling but ugly when surfaced in the GUI. This
//! module formats an error with a short user-facing description plus the
//! original technical detail, so people see e.g. "Device rejected the write
//! (DA reported error at command end)" instead of `Io("Invalid lifetime
//! acknowledgment")`.

use penumbra::error::Error;

/// Returns a friendly, single-line description of the error suitable for
/// surfacing in the GUI status area or a toast.
pub fn friendly(err: &Error) -> String {
    match err {
        Error::Timeout => "Device stopped responding (timeout).".into(),
        Error::Connection(msg) => format!("Connection error: {msg}"),
        Error::Io(msg) => friendly_io(msg),
        Error::Protocol(msg) => friendly_proto(msg),
        Error::Xml(x) => format!("Device protocol error: {x}"),
        Error::XFlash(x) => format!("Flash error: {x}"),
        Error::BrPl(x) => format!("Preloader/BROM error: {x}"),
        Error::Status { ctx, status } => format!("{ctx} (status 0x{status:08X})"),
        Error::Penumbra(msg) => msg.clone(),
        // Fallback to Display for anything we haven't special-cased.
        other => other.to_string(),
    }
}

fn friendly_io(msg: &str) -> String {
    let m = msg.to_ascii_lowercase();
    if m.contains("broken pipe") || m.contains("pipe error") {
        "USB connection lost (device disconnected or rebooted).".into()
    } else if m.contains("invalid lifetime acknowledgment") {
        // Old/short error string from before the message-extraction patch.
        "Device rejected the operation (DA returned an error).".into()
    } else if m.contains("da reported an error") {
        // New string — already user-facing, surface as-is.
        format!("Device rejected the operation: {msg}")
    } else if m.contains("invalid packet header") {
        "Got malformed reply from device (link out of sync).".into()
    } else if m.contains("no such device") || m.contains("not found") {
        "Device not found on USB. Re-plug the cable and try again.".into()
    } else if m.contains("timed out") || m.contains("timeout") {
        "Device stopped responding (timeout).".into()
    } else if m.contains("permission denied") {
        "Permission denied accessing USB device. Check udev rules / drivers.".into()
    } else {
        format!("Communication error: {msg}")
    }
}

fn friendly_proto(msg: &str) -> String {
    let m = msg.to_ascii_lowercase();
    if m.contains("invalid acknowledgment") {
        "Device sent an unexpected reply during handshake. \
         The DA may not support this command on this chip."
            .into()
    } else if m.contains("expected cmd:") {
        format!("Out of sync with device ({msg}).")
    } else {
        format!("Protocol error: {msg}")
    }
}
