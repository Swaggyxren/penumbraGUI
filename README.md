<img src="./docs/content/assets/banner.svg" alt="Penumbra banner">

---

Penumbra is a Rust crate and tool for interacting with Mediatek devices.<br>
It provides flashing and readback capabilities, as well as bootloader unlocking and relocking on vulnerable devices.<br>

## Requirements

* On Windows, you'll need to install WinUsb or Libusb drivers on the device. You can use [Zadig](https://zadig.akeo.ie/) for that, or the provided driver installer.
* On Linux you'll need to install `libudev` and add your user to the `dialout` group. In case Penumbra doesn't recognize the device, run with sudo or allow access to the device with udev rules.

## Usage

Penumbra can be used both as a crate for interacting directly with a device with your own code, as well as providing a CLI, (preliminary) [TUI](tui), and [GUI](gui).

For using the CLI, [read the documentation with all commands here](https://penumbra.itssho.my/Penumbra/Antumbra/CLI)

### Graphical UI (`penumbra-gui`)

A native desktop app that wraps the Penumbra core library is available in the [`gui/`](gui) crate. It is inspired by community flash-tool GUIs and provides:

* **PGPT Manager** — load the device's partition table, select partitions, and read them into a chosen backup folder with live progress
* **Auto-assign + Write Assigned** — map image files to partitions by name and flash them in one go
* **Bootloader** — seccfg unlock / lock with explicit confirmation dialogs
* **Operations** — reboot (Normal / Fastboot) and shutdown
* **Execution Log** — live log viewer with level filter, autoscroll, copy/save
* **Theming** — Dark Purple (default), Dark Blue, Dark Teal, and Light themes

Build and run the GUI with:

```sh
cargo run -p penumbra-gui --release
```

On Windows the binary is produced at `target/release/penumbra-gui.exe` — no console window is spawned.

For using the crate, use the device API:

```rs
use std::fs::File;
use std::io::{BufWriter, Write};

use anyhow::Result;
use penumbra::{DeviceBuilder, find_mtk_port, LockFlag};

fn main() -> Result<()> {
    env_logger::init();

    let da_path = std::path::Path::new("../DA_penangf.bin");
    let da_data = std::fs::read(da_path).expect("Failed to read DA file");

    println!("Searching for MTK port...");
    let mtk_port = loop {
        if let Some(port) = find_mtk_port() {
            break port;
        }
    };

    println!("Found MTK port: {}", mtk_port.get_port_name());

    let mut device = DeviceBuilder::default()
        .with_mtk_port(mtk_port)
        .with_da_data(da_data)
        .build()?;

    // Init the device (Handshake and populate dev info)
    device.init()?;

    let tgt_cfg = device.dev_info.target_config();
    println!("SBC: {}", (tgt_cfg & 0x1) != 0);

    // This will automatically enter DA mode. Seccfg unlock only works if the device can load extensions / is vulnerable
    device.set_seccfg_lock_state(LockFlag::Unlock)?;

    // Ignore progress for now
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

### Debug logs

Penumbra is still in early development, thus it can break quite easily.
If so, you can open an issue attaching debug logs.<br>
To get debug logs, run `antumbra` with the `-v` flag. A file called `antumbra.log` will be created in the current directory.
This will also enable UART debug logging. If possible, attach UART logs too.
If you don't have UART, you can use the `--usb-log` flag in `antumbra` to enable DA logging over USB.
A file called `da.log` will be created in the current directory with the logs.

> [!NOTE]
> Penumbra currently supports both V5 (XFlash) and V6 (XML) devices. Issues reporting incompatibility with other chipset will be ignored until broader support is added.
> If your device falls in one of these categories and you get the "unknown hardware code" warning, please open an issue attaching your device info, and relevant firmware
> files (preloader, DA, lk).

## Contributing

For contributing, you'll first need to setup a development environment.

Read on how to setup a dev environment and how to get started [here](CONTRIBUTING.md)

For contributing to the payloads, head to the [payloads repository](https://github.com/shomykohai/mtk-payloads).

### Current Roadmap

Core:
* [ ] Add V3 support
* [ ] Add amonet exploit

TUI:
* [ ] Refactor the TUI code to be more maintainable
* [ ] Add reusable components
* [ ] Make better key bindings

CLI:
* [ ] Add plstage
* [ ] Add Read Offset, Write Offset and Erase Offset commands
* [ ] Add register read/write commands

Documentation:
* [ ] Add documentation for the crate
* [ ] Add linecode exploit documentation

## Learning Resources

Penumbra has [its own documentation](https://penumbra.itssho.my/), where you can learn more about Mediatek devices and how the Download protocol works.

Other learning resources I suggest are the following
* [mtkclient](https://github.com/bkerler/mtkclient)
* [moto-experiments](https://github.com/R0rt1z2/moto-experiments)
* [kaeru](https://github.com/R0rt1z2/kaeru)
* [Carbonara exploit](https://penumbra.itssho.my/Mediatek/Exploits/Carbonara)
* [mtk-payloads](https://github.com/shomykohai/mtk-payloads)
* [da-boot](https://github.com/mt6572-mainline/da-boot)
* [fenrir](https://github.com/R0rt1z2/fenrir)
* [sprig](https://github.com/R0rt1z2/sprig)
* [HeapB8 exploit technical writeup](https://blog.r0rt1z2.com/posts/exploiting-mediatek-datwo/)

## Credits

* [ChimeraTool team](https://chimeratool.com/) - heapb8 was originally reverse-engineered from ChimeraTool.

## License

Penumbra is licensed under the GNU Affero General Public License v3 or later (AGPL-3.0-or-later), see [LICENSE](LICENSE) for details.

Some limited parts of the code in Penumbra are adapted from [mtkclient](https://github.com/bkerler/mtkclient). 
The code adapted from mtkclient is licensed under the GNU Public License v3 or later (GPL-3.0).

As for term 13 of the GPL-3.0 license, the GPL-3.0 components must comply the networking terms of the AGPL-3.0 license when used together.

Logo by [@archaeopteryz](https://github.com/archaeopteryz), all rights reserved. Use is allowed only for referencing "Penumbra" or "Antumbra", unless explicit permission has been granted.
