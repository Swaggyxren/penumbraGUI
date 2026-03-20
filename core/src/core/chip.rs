/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2026 Shomy
*/

/// Small database of SOCs, with their specific addresses that we might need
///
/// To add a new chip, create a `const` item using `ChipBuilder`, and add it
/// to the `chip_from_hw_code` match statement at the bottom.
/// If you find a chip with an unknown `hw_code`, please add it to the database and submit a PR!
///
/// For finding chips names: https://en.wikipedia.org/wiki/List_of_MediaTek_systems_on_chips

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChipInfo {
    name: &'static str,
    hw_code: u16,
    sej_base: u32,
    tzcc_base: u32,
    wdt: u32,
    uart: u32,
}

impl ChipInfo {
    /// Chip name & commercial name (e.g. `"MT6768/Helio G85"`).
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// HW code reported by the preloader or bootrom.
    pub const fn hw_code(&self) -> u16 {
        self.hw_code
    }

    /// SEJ base address. `0` if unknown.
    pub const fn sej_base(&self) -> u32 {
        self.sej_base
    }

    /// TZCC base address. `0` if unknown.
    pub const fn tzcc_base(&self) -> u32 {
        self.tzcc_base
    }

    /// Watchdog timer base address. `0` if unknown.
    pub const fn wdt(&self) -> u32 {
        self.wdt
    }

    /// UART base address. `0` if unknown.
    pub const fn uart(&self) -> u32 {
        self.uart
    }

    pub const fn has_sej(&self) -> bool {
        self.sej_base != 0
    }

    pub const fn has_tzcc(&self) -> bool {
        self.tzcc_base != 0
    }
}

pub struct ChipBuilder {
    name: &'static str,
    hw_code: u16,
    sej_base: u32,
    tzcc_base: u32,
    wdt: u32,
    uart: u32,
}

impl ChipBuilder {
    /// Start a new chip definition with the given name and raw `hw_code`.
    /// All address fields default to `0` (unknown).
    pub const fn new(name: &'static str, hw_code: u16) -> Self {
        Self { name, hw_code, sej_base: 0, tzcc_base: 0, wdt: 0, uart: 0 }
    }

    pub const fn with_sej_base(mut self, addr: u32) -> Self {
        self.sej_base = addr;
        self
    }

    pub const fn with_tzcc_base(mut self, addr: u32) -> Self {
        self.tzcc_base = addr;
        self
    }

    pub const fn with_wdt(mut self, addr: u32) -> Self {
        self.wdt = addr;
        self
    }

    pub const fn with_uart(mut self, addr: u32) -> Self {
        self.uart = addr;
        self
    }

    pub const fn build(self) -> ChipInfo {
        ChipInfo {
            name: self.name,
            hw_code: self.hw_code,
            sej_base: self.sej_base,
            tzcc_base: self.tzcc_base,
            wdt: self.wdt,
            uart: self.uart,
        }
    }
}

// Uses default known bases common on many socs, might work on most XFlash devices,
// but not guaranteed to be correct on XML ones.
pub const UNKNOWN_CHIP: ChipInfo = ChipBuilder::new("Unknown", 0x0000)
    .with_sej_base(0x1000A000)
    .with_tzcc_base(0x10210000)
    .with_uart(0x11002000)
    .build();

pub const MT6797: ChipInfo = ChipBuilder::new("MT6797/Helio X25", 0x279)
    .with_sej_base(0x1000A000)
    .with_wdt(0x10007000)
    .with_uart(0x11002000)
    .build();

pub const MT6755: ChipInfo = ChipBuilder::new("MT6755/Helio P10", 0x326)
    .with_sej_base(0x1000A000)
    .with_wdt(0x10007000)
    .with_uart(0x11002000)
    .build();

pub const MT6757: ChipInfo = ChipBuilder::new("MT6757/Helio P20", 0x551)
    .with_sej_base(0x1000A000)
    .with_wdt(0x10007000)
    .with_uart(0x11002000)
    .build();

pub const MT6799: ChipInfo = ChipBuilder::new("MT6799/Helio X30", 0x562)
    .with_sej_base(0x1000A000)
    .with_tzcc_base(0x11B20000)
    .with_wdt(0x10211000)
    .with_uart(0x11020000)
    .build();

pub const MT6750: ChipInfo = ChipBuilder::new("MT6750", 0x601)
    .with_sej_base(0x1000A000)
    .with_wdt(0x10007000)
    .with_uart(0x11002000)
    .build();

pub const MT6570: ChipInfo = ChipBuilder::new("MT6570/MT8321", 0x633)
    .with_sej_base(0x1000A000)
    .with_wdt(0x10007000)
    .with_uart(0x11002000)
    .build();

pub const MT6758: ChipInfo = ChipBuilder::new("MT6758/Helio P30", 0x688)
    .with_sej_base(0x10080000)
    .with_tzcc_base(0x11240000)
    .with_wdt(0x10211000)
    .with_uart(0x11020000)
    .build();

pub const MT6763: ChipInfo = ChipBuilder::new("MT6763/Helio P23", 0x690)
    .with_sej_base(0x1000A000)
    .with_tzcc_base(0x10210000)
    .with_wdt(0x10007000)
    .with_uart(0x11002000)
    .build();

pub const MT6739: ChipInfo = ChipBuilder::new("MT6739/MT8765", 0x699)
    .with_sej_base(0x1000A000)
    .with_tzcc_base(0x10210000)
    .with_wdt(0x10007000)
    .with_uart(0x11002000)
    .build();

// penangf
pub const MT6768: ChipInfo = ChipBuilder::new("MT6768/MT6769, Helio G85", 0x707)
    .with_sej_base(0x1000A000)
    .with_tzcc_base(0x10210000)
    .with_wdt(0x10007000)
    .with_uart(0x11002000)
    .build();

pub const MT6761: ChipInfo = ChipBuilder::new("MT6761/Helio P22", 0x717)
    .with_sej_base(0x1000A000)
    .with_tzcc_base(0x10210000)
    .with_wdt(0x10007000)
    .with_uart(0x11002000)
    .build();

pub const MT6779: ChipInfo = ChipBuilder::new("MT6779/Helio P90", 0x725)
    .with_sej_base(0x1000A000)
    .with_tzcc_base(0x10210000)
    .with_wdt(0x10007000)
    .with_uart(0x11002000)
    .build();

pub const MT6765: ChipInfo = ChipBuilder::new("MT6765/Helio P35", 0x766)
    .with_sej_base(0x1000A000)
    .with_tzcc_base(0x10210000)
    .with_wdt(0x10007000)
    .with_uart(0x11002000)
    .build();

pub const MT6771: ChipInfo = ChipBuilder::new("MT6771/Helio P60", 0x788)
    .with_sej_base(0x1000A000)
    .with_tzcc_base(0x10210000)
    .with_wdt(0x10007000)
    .with_uart(0x11002000)
    .build();

pub const MT6785: ChipInfo = ChipBuilder::new("MT6785/Helio G90", 0x813)
    .with_sej_base(0x1000A000)
    .with_tzcc_base(0x10210000)
    .with_wdt(0x10007000)
    .with_uart(0x11002000)
    .build();

pub const MT6885: ChipInfo = ChipBuilder::new("MT6885/Dimensity 1000", 0x816)
    .with_sej_base(0x1000A000)
    .with_tzcc_base(0x10210000)
    .with_wdt(0x10007000)
    .with_uart(0x11002000)
    .build();

pub const MT6873: ChipInfo = ChipBuilder::new("MT6873/Dimensity 800", 0x886)
    .with_sej_base(0x1000A000)
    .with_tzcc_base(0x10210000)
    .with_wdt(0x10007000)
    .with_uart(0x11002000)
    .build();

pub const MT6983: ChipInfo = ChipBuilder::new("MT6983/Dimensity 9000", 0x907)
    .with_sej_base(0x1000A000)
    .with_tzcc_base(0x10210000)
    .with_wdt(0x1C007000)
    .with_uart(0x11001000)
    .build();

pub const MT6893: ChipInfo = ChipBuilder::new("MT6893/Dimensity 1200", 0x950)
    .with_sej_base(0x1000A000)
    .with_tzcc_base(0x10210000)
    .with_wdt(0x10007000)
    .with_uart(0x11002000)
    .build();

pub const MT6878: ChipInfo =
    ChipBuilder::new("MT6878/Dimensity 7300/7300X/7360/7400/7400X", 0x1375)
        .with_sej_base(0x1040E000)
        .with_tzcc_base(0x10400000)
        .with_wdt(0x1C00A000)
        .with_uart(0x11001000)
        .build();

pub const MT6877: ChipInfo = ChipBuilder::new("MT6877/Dimensity 900", 0x959)
    .with_sej_base(0x1000A000)
    .with_tzcc_base(0x10210000)
    .with_wdt(0x10007000)
    .with_uart(0x11002000)
    .build();

pub const MT6833: ChipInfo = ChipBuilder::new("MT6833/Dimensity 700", 0x989)
    .with_sej_base(0x1000A000)
    .with_tzcc_base(0x10210000)
    .with_wdt(0x10007000)
    .with_uart(0x11002000)
    .build();

pub const MT6853: ChipInfo = ChipBuilder::new("MT6853/Dimensity 720", 0x996)
    .with_sej_base(0x1000A000)
    .with_tzcc_base(0x10210000)
    .with_wdt(0x10007000)
    .with_uart(0x11002000)
    .build();

pub const MT6781: ChipInfo = ChipBuilder::new("MT6781/Helio G96", 0x1066)
    .with_sej_base(0x1000A000)
    .with_tzcc_base(0x10210000)
    .with_wdt(0x10007000)
    .with_uart(0x11002000)
    .build();

pub const MT6855: ChipInfo = ChipBuilder::new("MT6855/Dimensity 8100", 0x1129)
    .with_sej_base(0x1000A000)
    .with_tzcc_base(0x10210000)
    .with_wdt(0x1C007000)
    .with_uart(0x11001000)
    .build();

pub const MT6895: ChipInfo = ChipBuilder::new("MT6895/Dimensity 8200", 0x1172)
    .with_sej_base(0x1C009000)
    .with_tzcc_base(0x1C807000)
    .with_wdt(0x1C007000)
    .with_uart(0x11001000)
    .build();

pub const MT6897: ChipInfo = ChipBuilder::new("MT6897/Dimensity 8300", 0x1203)
    .with_sej_base(0x1040E000)
    .with_tzcc_base(0x10403000)
    .with_wdt(0x1C007000)
    .with_uart(0x11002000)
    .build();

// Emerald
pub const MT6789: ChipInfo = ChipBuilder::new("MT6789/Helio G99", 0x1208)
    .with_sej_base(0x1000A000)
    .with_tzcc_base(0x10210000)
    .with_wdt(0x10007000)
    .with_uart(0x11002000)
    .build();

pub const MT6835: ChipInfo = ChipBuilder::new("MT6835/Dimensity 6100+", 0x1209)
    .with_sej_base(0x1000A000)
    .with_tzcc_base(0x10210000)
    .with_wdt(0x1C007000)
    .with_uart(0x11002000)
    .build();

// Pacman
pub const MT6886: ChipInfo = ChipBuilder::new("MT6886/Dimensity 7200", 0x1229)
    .with_sej_base(0x1C009000)
    .with_tzcc_base(0x1C807000)
    .with_wdt(0x1C007000)
    .with_uart(0x11001000)
    .build();

pub const MT6985: ChipInfo = ChipBuilder::new("MT6985/Dimensity 9200", 0x1296)
    .with_sej_base(0x1C009000)
    .with_tzcc_base(0x1C807000)
    .with_wdt(0x1C007000)
    .with_uart(0x1C011000)
    .build();

pub const MT6991: ChipInfo = ChipBuilder::new("MT6991/Dimensity 9400", 0x1357)
    .with_sej_base(0x1800E000)
    .with_tzcc_base(0x18003000)
    .with_wdt(0x1C010000)
    .with_uart(0x16000000)
    .build();

pub const MT6993: ChipInfo = ChipBuilder::new("MT6993/Dimensity 9500", 0x1471)
    .with_sej_base(0x1800E000)
    .with_tzcc_base(0x18005000)
    .with_wdt(0x1C010000)
    .with_uart(0x16010000)
    .build();

pub const MT8167: ChipInfo = ChipBuilder::new("MT8167/MT8516", 0x8167)
    .with_sej_base(0x1000A000)
    .with_wdt(0x10007000)
    .with_uart(0x11005000)
    .build();

pub const MT8168: ChipInfo = ChipBuilder::new("MT8168", 0x8168)
    .with_sej_base(0x1000A000)
    .with_wdt(0x10007000)
    .with_uart(0x11002000)
    .build();

pub const MT8512: ChipInfo = ChipBuilder::new("MT8512", 0x8512)
    .with_sej_base(0x1000A000)
    .with_wdt(0x10007000)
    .with_uart(0x11002000)
    .build();

pub const MT8695: ChipInfo = ChipBuilder::new("MT8695", 0x8695)
    .with_sej_base(0x1000A000)
    .with_wdt(0x10007000)
    .with_uart(0x11002000)
    .build();

/// Look up chip information by `hw_code`. Returns a reference to a static [`ChipInfo`] struct.
/// If the `hw_code` is not recognized, returns a reference to `UNKNOWN_CHIP`.
///
/// If you find a chip with an unknown `hw_code`, please add it to the database and submit a
/// PR!
pub const fn chip_from_hw_code(hw_code: u16) -> &'static ChipInfo {
    match hw_code {
        0x279 => &MT6797,
        0x326 => &MT6755,
        0x551 => &MT6757,
        0x562 => &MT6799,
        0x601 => &MT6750,
        0x633 => &MT6570,
        0x688 => &MT6758,
        0x690 => &MT6763,
        0x699 => &MT6739,
        0x707 => &MT6768,
        0x717 => &MT6761,
        0x725 => &MT6779,
        0x766 => &MT6765,
        0x788 => &MT6771,
        0x813 => &MT6785,
        0x816 => &MT6885,
        0x886 => &MT6873,
        0x907 => &MT6983,
        0x950 => &MT6893,
        0x959 => &MT6877,
        0x989 => &MT6833,
        0x996 => &MT6853,
        0x1066 => &MT6781,
        0x1129 => &MT6855,
        0x1172 => &MT6895,
        0x1203 => &MT6897,
        0x1208 => &MT6789,
        0x1209 => &MT6835,
        0x1229 => &MT6886,
        0x1375 => &MT6878,
        0x1296 => &MT6985,
        0x1357 => &MT6991,
        0x1471 => &MT6993,
        0x8167 => &MT8167,
        0x8168 => &MT8168,
        0x8512 => &MT8512,
        0x8695 => &MT8695,
        _ => &UNKNOWN_CHIP,
    }
}
