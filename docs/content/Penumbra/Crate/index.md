Penumbra is a crate for interacting and servicing with MediaTek devices over [[Download Agent|DA]] mode.
It provides a high-level API for interacting with the device, which can be used to implement custom tools for many operations, such as firmware flashing, readback, bootloader unlocking, RPMB operations, and more.

> [!Note]
> Penumbra is licensed under the GNU Affero General Public License v3.0 or later (AGPLv3-or-later), and as such, any code that incorporates, links or is directly derived from Penumbra must also be licensed under the AGPLv3 or later. 

> [!Warning]
> Penumbra is not intended to be used for malicious purposes, rather as a tool for servicing and repairing devices. Please use it responsibly.
> As such, **no support will be provided** for FRP erasing, IMEI spoofing.


## Requirements

To use Penumbra on your own code, you'll need to add it as a dependency in your `Cargo.toml`:

```toml
[dependencies]
penumbra-mtk = { git = "https: //github.com/shomykohai/penumbra", branch = "main" }
```

> [!Note]
> You'll need to use the `nightly` rust toolchain to build Penumbra, as it uses some unstable features.

## Usage

Below, is a simple example of how to use Penumbra to connect to a device and perform some operations:


```rust
use std::fs::File;
use std::io::{BufWriter, Write};

use anyhow::Result;
use penumbra::{DeviceBuilder, find_mtk_port, LockFlag};

fn main() -> Result<()> {
    env_logger::init();

    let da_path = std::path::Path::new("../DA_penangf.bin");
    let da_data = std::fs::read(da_path).expect("Failed to read DA file");

    // Specify the VID and PID to filter only Preloader port
    let vid = Some(0x0E8D);
    let pid = Some(0x2000);

    // Find the MTK port with the specified VID and PID.
    // Backend set to Auto will try to find the first compatible port,
    // with either USB or Serial.
    let mtk_port = PortType::find_device(vid, pid, PortBackend::Auto)
        .expect("Port should open")
        .ok_or("No MTK port found")?;

    println!("Found MTK port: {}", mtk_port.get_port_name());

    let mut device = DeviceBuilder::new(mtk_port)
        .with_da_data(da_data)
        .build()?;

    // Init the device (Handshake and populate dev info)
    device.init()?;

    let tgt_cfg = device.devinfo().target_config();
    println!("SBC: {}", (tgt_cfg & 0x1) != 0);

    // This will automatically enter DA mode. 
    // Seccfg unlock only works if the device can load extensions / is vulnerable
    device.set_seccfg_lock_state(LockFlag::Unlock)?;

    let mut progress = |read: u64, total: u64| {
        println!("Progress: {}/{}", read, total);
    };

    let file = File::create("lk_a.bin")?;
    let mut writer = BufWriter::new(file);

    device.read_partition("lk_a", &mut progress, &mut writer)?;

    writer.flush()?;

    Ok(())
}
```

## Feature flags

Penumbra has some feature flags you can enable.

* `nusb` - Enabled the `nusb` backend for USB. This is the default one.
* `libusb` - Enabled the `libusb` backend for USB. This is an alternative to `nusb`, and may work better on some platforms.
* `serial` - Enabled the `serial` backend for Serial. Works best on windows with default MediaTek VCOM drivers, but not all features might be available.
* `localslakeyring` - Use penumbra sla keyring for completing SLA challenges. This includes leaked keys for some devices.
* `reenumerate` - Reenumerate the device to DA mode after the handshake. This will slow down connection during initialization, but may be useful for some devices.
* `exploits` - Enable exploits, automatic DA patching, [[DA Extensions]] and custom commands. This allows to perform more operations on vulnerable devices, and read/write to partitions that are normally locked and memory regions.

By default, penumbra enables `nusb`, `libusb`, `serial`, `localslakeyring` and `exploits`. You can disable them by using `default-features = false` in your `Cargo.toml`:
