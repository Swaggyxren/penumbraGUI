/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2025 Shomy
*/
#[cfg(feature = "libusb")]
pub mod libusb_backend;
#[cfg(feature = "serial")]
pub mod serial_backend;
#[cfg(not(any(feature = "libusb", feature = "serial")))]
pub mod usb_backend;
#[cfg(feature = "libusb")]
pub use libusb_backend::UsbMTKPort;
#[cfg(feature = "serial")]
pub use serial_backend::SerialMTKPort;
#[cfg(not(any(feature = "libusb", feature = "serial")))]
pub use usb_backend::UsbMTKPort;
