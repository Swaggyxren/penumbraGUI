/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2026 Shomy, Penumbra Contributors
*/

//! Driver and system diagnostics for MediaTek flashing on Linux and Windows.

#[allow(unused_imports)]
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagLevel {
    Ok,
    Warning,
    ActionRequired,
}

#[derive(Debug, Clone)]
pub struct DiagCheckItem {
    pub title: String,
    pub detail: String,
    pub level: DiagLevel,
}

#[derive(Debug, Clone)]
pub struct DriverDiagStatus {
    pub os_name: String,
    pub is_linux: bool,
    pub is_windows: bool,
    pub overall_level: DiagLevel,
    pub items: Vec<DiagCheckItem>,
    pub bash_commands: Option<String>,
    pub instructions: Option<String>,
}

impl Default for DriverDiagStatus {
    fn default() -> Self {
        Self::run()
    }
}

impl DriverDiagStatus {
    /// Runs a non-blocking diagnostic scan of system drivers and permissions.
    pub fn run() -> Self {
        #[cfg(target_os = "linux")]
        {
            Self::diagnose_linux()
        }

        #[cfg(target_os = "windows")]
        {
            Self::diagnose_windows()
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            Self::diagnose_generic()
        }
    }

    /// Generates the standard Linux udev rules and group configuration bash commands.
    pub fn linux_setup_commands() -> String {
        let username = std::env::var("USER")
            .or_else(|_| std::env::var("LOGNAME"))
            .unwrap_or_else(|_| "$USER".to_string());

        format!(
            "sudo tee /etc/udev/rules.d/51-mtk-penumbra.rules << 'EOF'\n\
# MediaTek BROM / Preloader / DA USB endpoints (Penumbra)\n\
SUBSYSTEM==\"usb\", ATTR{{idVendor}}==\"0e8d\", MODE=\"0660\", TAG+=\"uaccess\", GROUP=\"uucp\"\n\
SUBSYSTEM==\"tty\", ATTRS{{idVendor}}==\"0e8d\", MODE=\"0660\", TAG+=\"uaccess\", ENV{{ID_MM_DEVICE_IGNORE}}=\"1\"\n\
EOF\n\
sudo udevadm control --reload-rules && sudo udevadm trigger\n\
sudo usermod -aG dialout,uucp,plugdev {username}\n"
        )
    }

    /// Provides step-by-step driver setup instructions for Windows.
    pub fn windows_instructions() -> &'static str {
        "1. MediaTek Preloader USB VCOM Driver is required for Windows to communicate with BROM & Preloader without disconnecting.\n\
2. Download and run the MediaTek USB VCOM Driver (or MTK All-In-One Driver installer).\n\
3. If using the LibUSB backend, use Zadig (https://zadig.akeo.ie) to install WinUSB on \"MediaTek USB Port\" (VID: 0E8D, PID: 0003).\n\
4. If driver installation is blocked on Windows 10/11, temporarily disable Driver Signature Enforcement in Windows Advanced Startup.\n\
5. Connect your device via USB while holding Volume Down or Volume Up."
    }

    #[cfg(target_os = "linux")]
    fn diagnose_linux() -> Self {
        let mut items = Vec::new();
        let mut overall_level = DiagLevel::Ok;

        // 1. Check udev rules
        let (udev_found, udev_path) = check_linux_udev_rules();
        if udev_found {
            items.push(DiagCheckItem {
                title: "MediaTek udev Rules".into(),
                detail: format!(
                    "Installed ({})",
                    udev_path.as_deref().unwrap_or("MediaTek rule active")
                ),
                level: DiagLevel::Ok,
            });
        } else {
            overall_level = DiagLevel::ActionRequired;
            items.push(DiagCheckItem {
                title: "MediaTek udev Rules".into(),
                detail: "Missing /etc/udev/rules.d/51-mtk-penumbra.rules (normal users cannot access USB endpoints)".into(),
                level: DiagLevel::ActionRequired,
            });
        }

        // 2. Check user group membership
        let (in_group, current_user, groups) = check_linux_user_groups();
        if in_group {
            let matching: Vec<_> = groups
                .iter()
                .filter(|g| ["uucp", "dialout", "plugdev"].contains(&g.as_str()))
                .cloned()
                .collect();
            items.push(DiagCheckItem {
                title: "User Permissions".into(),
                detail: format!("User '{current_user}' belongs to: {}", matching.join(", ")),
                level: DiagLevel::Ok,
            });
        } else {
            if overall_level == DiagLevel::Ok {
                overall_level = DiagLevel::ActionRequired;
            }
            items.push(DiagCheckItem {
                title: "User Permissions".into(),
                detail: format!(
                    "User '{current_user}' is not in 'uucp', 'dialout', or 'plugdev' group"
                ),
                level: DiagLevel::ActionRequired,
            });
        }

        // 3. Check ModemManager
        let modemmanager_active = check_modemmanager_active();
        if modemmanager_active {
            if udev_found {
                items.push(DiagCheckItem {
                    title: "ModemManager Filter".into(),
                    detail: "ModemManager running; udev rules configured to ignore MTK ports".into(),
                    level: DiagLevel::Ok,
                });
            } else {
                if overall_level != DiagLevel::ActionRequired {
                    overall_level = DiagLevel::Warning;
                }
                items.push(DiagCheckItem {
                    title: "ModemManager Interference".into(),
                    detail: "ModemManager is active and may interrupt Preloader handshake without udev filter".into(),
                    level: DiagLevel::Warning,
                });
            }
        } else {
            items.push(DiagCheckItem {
                title: "ModemManager".into(),
                detail: "Not active or not interfering".into(),
                level: DiagLevel::Ok,
            });
        }

        Self {
            os_name: "Linux".into(),
            is_linux: true,
            is_windows: false,
            overall_level,
            items,
            bash_commands: Some(Self::linux_setup_commands()),
            instructions: Some(Self::windows_instructions().to_string()),
        }
    }

    #[cfg(target_os = "windows")]
    fn diagnose_windows() -> Self {
        let mut items = Vec::new();
        let mut overall_level = DiagLevel::Ok;

        let (driver_found, driver_desc) = check_windows_drivers();
        if driver_found {
            items.push(DiagCheckItem {
                title: "MediaTek USB Driver".into(),
                detail: format!("Detected: {driver_desc}"),
                level: DiagLevel::Ok,
            });
        } else {
            overall_level = DiagLevel::ActionRequired;
            items.push(DiagCheckItem {
                title: "MediaTek USB Driver".into(),
                detail: "MediaTek Preloader / VCOM Driver or WinUSB not detected in Windows PNP store".into(),
                level: DiagLevel::ActionRequired,
            });
        }

        // Check UsbDk / WinUSB fallback availability
        let usbdk_present = check_windows_service("UsbDk");
        if usbdk_present {
            items.push(DiagCheckItem {
                title: "UsbDk Filter Driver".into(),
                detail: "Installed and ready for LibUSB port access".into(),
                level: DiagLevel::Ok,
            });
        } else {
            items.push(DiagCheckItem {
                title: "UsbDk / WinUSB".into(),
                detail: "Optional UsbDk service not installed (standard VCOM or WinUSB used)".into(),
                level: DiagLevel::Ok,
            });
        }

        Self {
            os_name: "Windows".into(),
            is_linux: false,
            is_windows: true,
            overall_level,
            items,
            bash_commands: Some(Self::linux_setup_commands()),
            instructions: Some(Self::windows_instructions().to_string()),
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    fn diagnose_generic() -> Self {
        Self {
            os_name: std::env::consts::OS.to_string(),
            is_linux: false,
            is_windows: false,
            overall_level: DiagLevel::Ok,
            items: vec![DiagCheckItem {
                title: "USB Subsystem".into(),
                detail: "Standard OS USB stack active".into(),
                level: DiagLevel::Ok,
            }],
            bash_commands: Some(Self::linux_setup_commands()),
            instructions: Some(Self::windows_instructions().to_string()),
        }
    }

    /// Launches Windows Device Manager if on Windows.
    pub fn open_device_manager() -> bool {
        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("devmgmt.msc").spawn().is_ok()
        }
        #[cfg(not(target_os = "windows"))]
        {
            false
        }
    }
}

// ---------------------------------------------------------------------------
// Linux Internal Diagnostics
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
fn check_linux_udev_rules() -> (bool, Option<String>) {
    let candidate_files = [
        "/etc/udev/rules.d/51-mtk-penumbra.rules",
        "/etc/udev/rules.d/51-edl.rules",
        "/etc/udev/rules.d/99-mediatek.rules",
        "/usr/lib/udev/rules.d/51-mtk-penumbra.rules",
        "/lib/udev/rules.d/51-mtk-penumbra.rules",
    ];

    for path_str in candidate_files {
        let path = Path::new(path_str);
        if path.exists() {
            return (true, Some(path_str.to_string()));
        }
    }

    // Check directory contents for any rule matching MediaTek VID 0e8d
    let search_dirs = ["/etc/udev/rules.d", "/usr/lib/udev/rules.d", "/lib/udev/rules.d"];
    for dir in search_dirs {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().map_or(false, |ext| ext == "rules") {
                    if let Ok(content) = std::fs::read_to_string(&p) {
                        if content.contains("0e8d") || content.contains("0E8D") {
                            return (true, Some(p.display().to_string()));
                        }
                    }
                }
            }
        }
    }

    (false, None)
}

#[cfg(target_os = "linux")]
fn check_linux_user_groups() -> (bool, String, Vec<String>) {
    let username = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "user".to_string());

    let mut groups = Vec::new();

    // Try `id -Gn` command
    if let Ok(out) = std::process::Command::new("id").arg("-Gn").output() {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            groups = text.split_whitespace().map(|s| s.to_string()).collect();
        }
    }

    // Fallback: parse /etc/group
    if groups.is_empty() {
        if let Ok(content) = std::fs::read_to_string("/etc/group") {
            for line in content.lines() {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 4 {
                    let group_name = parts[0];
                    let members: Vec<&str> = parts[3].split(',').collect();
                    if members.contains(&username.as_str()) {
                        groups.push(group_name.to_string());
                    }
                }
            }
        }
    }

    let in_group = groups.iter().any(|g| g == "uucp" || g == "dialout" || g == "plugdev");
    (in_group, username, groups)
}

#[cfg(target_os = "linux")]
fn check_modemmanager_active() -> bool {
    if let Ok(out) = std::process::Command::new("systemctl")
        .args(["is-active", "--quiet", "ModemManager"])
        .output()
    {
        if out.status.success() {
            return true;
        }
    }

    // Fallback: inspect /proc for ModemManager process
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let comm_path = p.join("comm");
                if let Ok(comm) = std::fs::read_to_string(comm_path) {
                    if comm.trim() == "ModemManager" {
                        return true;
                    }
                }
            }
        }
    }

    false
}

// ---------------------------------------------------------------------------
// Windows Internal Diagnostics
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
fn check_windows_drivers() -> (bool, String) {
    // 1. Query PNP devices via powershell for MediaTek or VID 0E8D
    let ps_cmd = "Get-PnpDevice -Class Ports,USB -ErrorAction SilentlyContinue | Where-Object { $_.FriendlyName -match 'MediaTek|MTK|Preloader' -or $_.InstanceId -match 'VID_0E8D' } | Select-Object -First 1 -ExpandProperty FriendlyName";
    if let Ok(out) = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", ps_cmd])
        .output()
    {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !text.is_empty() {
                return (true, text);
            }
        }
    }

    // 2. Query pnputil for third-party driver store INF matching MediaTek
    if let Ok(out) = std::process::Command::new("pnputil")
        .args(["/enum-drivers"])
        .output()
    {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            if text.to_lowercase().contains("mediatek") || text.to_lowercase().contains("mtk") {
                return (true, "MediaTek OEM Driver (Driver Store)".into());
            }
        }
    }

    // 3. Fallback: Check standard USB Serial Service
    if check_windows_service("usbser") {
        return (true, "Windows USB Serial (usbser.sys) available".into());
    }

    (false, "No MediaTek VCOM driver found".into())
}

#[cfg(target_os = "windows")]
fn check_windows_service(service_name: &str) -> bool {
    let cmd = format!("Get-Service -Name '{service_name}' -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty Status");
    if let Ok(out) = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &cmd])
        .output()
    {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            return !text.is_empty();
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_driver_diag_run() {
        let diag = DriverDiagStatus::run();
        assert!(!diag.os_name.is_empty());
        assert!(!diag.items.is_empty());
        assert!(diag.bash_commands.is_some());
        assert!(diag.instructions.is_some());
    }

    #[test]
    fn test_linux_setup_commands() {
        let cmds = DriverDiagStatus::linux_setup_commands();
        assert!(cmds.contains("0e8d"));
        assert!(cmds.contains("51-mtk-penumbra.rules"));
        assert!(cmds.contains("udevadm control --reload-rules"));
        assert!(cmds.contains("usermod -aG"));
    }

    #[test]
    fn test_windows_instructions() {
        let instructions = DriverDiagStatus::windows_instructions();
        assert!(instructions.contains("MediaTek Preloader USB VCOM Driver"));
        assert!(instructions.contains("Zadig"));
        assert!(instructions.contains("0E8D"));
    }
}
