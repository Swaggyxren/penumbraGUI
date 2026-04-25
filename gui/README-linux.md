# Penumbra GUI on Linux

`penumbra-gui` is a Rust/egui desktop app and runs natively on Linux (X11
and Wayland). On Linux there is no separate driver to install — `libusb`
already ships with the kernel — but you do need a udev rule so non-root
users can open the MediaTek BROM / Preloader / DA endpoints.

## Arch Linux (recommended)

A `PKGBUILD` is shipped under [`packaging/arch`](../packaging/arch).

### From the repo

```
git clone https://github.com/Swaggyxren/penumbraGUI.git
cd penumbraGUI/packaging/arch
makepkg -si
```

`makepkg -si` builds `penumbra-gui` from the v1.1.0-gui tag, installs the
binary at `/usr/bin/penumbra-gui`, drops the `.desktop` entry and icons
into `hicolor`, and installs the udev rules at
`/usr/lib/udev/rules.d/51-mtk-penumbra.rules`.

### From the AUR

Once published the package will be `penumbra-gui` (not yet on AUR — check
the upstream repo for status).

```
yay -S penumbra-gui
```

### Activating the udev rules

The PKGBUILD installs them under `/usr/lib/udev/rules.d/` so they take
effect on the next plug-in. To apply immediately without re-plugging:

```
sudo udevadm control --reload-rules
sudo udevadm trigger
```

The rules drop access into the `uucp` group (Arch's group for serial /
USB devices). Add yourself once:

```
sudo usermod -aG uucp $USER
```

…then log out + log back in for the group to take effect.

## Other Linux distros

There is no distro-specific installer yet. Two options:

1. **Build from source.** Requires `rustc >= 1.83`, `cargo`, plus the
   Linux runtime libs already listed in the PKGBUILD (`libusb-1.0`,
   `fontconfig`, `freetype2`, `libxcb`, `libxkbcommon`, `wayland`).
   ```
   git clone https://github.com/Swaggyxren/penumbraGUI.git
   cd penumbraGUI
   cargo build --release -p penumbra-gui
   ./target/release/penumbra-gui
   ```

   The `gui` crate already turns on the libusb backend in its
   `Cargo.toml`, so no extra `--features` flag is needed.
2. **Grab the prebuilt tarball** from the [v1.1.0-gui release](https://github.com/Swaggyxren/penumbraGUI/releases/tag/v1.1.0-gui).
   It contains the binary, the `.desktop` entry, the icon hierarchy,
   the udev rules and an `install.sh` that copies them into the right
   places under `/usr/local`. Adjust the `GROUP=` line inside the
   `.rules` file if your distro uses `plugdev` or `dialout` instead of
   `uucp`.

## Putting the device into BROM / Preloader mode

Same procedure as Windows:

1. Power the phone off.
2. Hold **Vol Up + Vol Down** (or just Vol Up on some models).
3. Plug in USB while holding.

When the rules are loaded correctly, `lsusb` should list the device with
the `0e8d:` (or the vendor's) IDs from
`core/src/connection/port.rs::KNOWN_PORTS`, and `penumbra-gui` should
connect immediately.
