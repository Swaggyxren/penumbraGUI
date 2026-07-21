/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2026 Shomy
*/

//! Built-in color themes for the Penumbra GUI.

use eframe::egui::style::{Selection, WidgetVisuals, Widgets};
use eframe::egui::{Color32, Rounding, Stroke, Visuals};
use eframe::epaint::Shadow;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeId {
    DarkCharcoal,
    Sunset,
    Warm,
    Cool,
    Hacker,
}

impl ThemeId {
    pub const ALL: &'static [ThemeId] = &[
        ThemeId::DarkCharcoal,
        ThemeId::Sunset,
        ThemeId::Warm,
        ThemeId::Cool,
        ThemeId::Hacker,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ThemeId::DarkCharcoal => "Dark (Charcoal)",
            ThemeId::Sunset => "Sunset",
            ThemeId::Warm => "Warm (Autumn Sunset)",
            ThemeId::Cool => "Cool (Star Command)",
            ThemeId::Hacker => "Hacker (Matrix)",
        }
    }

    pub fn palette(self) -> Palette {
        match self {
            ThemeId::DarkCharcoal => Palette {
                background: Color32::from_rgb(0x0E, 0x0F, 0x14), // `#0e0f14`
                panel: Color32::from_rgb(0x13, 0x14, 0x1B),      // `#13141b`
                panel_alt: Color32::from_rgb(0x1B, 0x1C, 0x26),  // `#1b1c26`
                border: Color32::from_rgb(0x20, 0x22, 0x30),      // `#202230`
                text: Color32::from_rgb(0xF3, 0xF4, 0xF6),        // `#f3f4f6`
                text_muted: Color32::from_rgb(0x8E, 0x93, 0xA6),  // `#8e93a6`
                accent: Color32::from_rgb(0x7C, 0x4D, 0xFF),      // `#7c4dff` (vibrant purple)
                accent_strong: Color32::from_rgb(0xA2, 0x9B, 0xFE), // `#a29bfe`
                success: Color32::from_rgb(0x10, 0xB9, 0x81),
                warn: Color32::from_rgb(0xF5, 0x9E, 0x0B),
                error: Color32::from_rgb(0xEF, 0x44, 0x44),
                is_dark: true,
            },
            ThemeId::Sunset => Palette {
                background: Color32::from_rgb(0x18, 0x22, 0x2D), // `#18222d` (dark slate blue)
                panel: Color32::from_rgb(0x1E, 0x2A, 0x38),      // `#1e2a38` (dark blue/slate)
                panel_alt: Color32::from_rgb(0x27, 0x36, 0x48),  // `#273648` (medium slate)
                border: Color32::from_rgb(0x2E, 0x3F, 0x54),     // `#2e3f54` (light slate border)
                text: Color32::from_rgb(0xF3, 0xF4, 0xF6),       // `#f3f4f6`
                text_muted: Color32::from_rgb(0x8A, 0x9B, 0xB0), // `#8a9bb0`
                accent: Color32::from_rgb(0xFF, 0x75, 0x82),     // `#ff7582` (sunset coral)
                accent_strong: Color32::from_rgb(0xC5, 0x6C, 0x86), // `#c56c86` (dusty rose)
                success: Color32::from_rgb(0x10, 0xB9, 0x81),
                warn: Color32::from_rgb(0xF5, 0x9E, 0x0B),
                error: Color32::from_rgb(0xEF, 0x44, 0x44),
                is_dark: true,
            },
            ThemeId::Warm => Palette {
                background: Color32::from_rgb(0x21, 0x14, 0x16), // `#211416` (deep rich burgundy)
                panel: Color32::from_rgb(0x32, 0x1E, 0x21),      // `#321e21` (warm burgundy)
                panel_alt: Color32::from_rgb(0x47, 0x2D, 0x30),  // `#472d30` (old burgundy)
                border: Color32::from_rgb(0x56, 0x36, 0x3B),     // `#56363b` (burgundy border)
                text: Color32::from_rgb(0xFF, 0xE1, 0xA8),       // `#ffe1a8` (warm peach)
                text_muted: Color32::from_rgb(0xC9, 0xCB, 0xA3), // `#c9cba3` (sage green)
                accent: Color32::from_rgb(0xE2, 0x6D, 0x5C),     // `#e26d5c` (terracotta)
                accent_strong: Color32::from_rgb(0x72, 0x3D, 0x46), // `#723d46` (colo)
                success: Color32::from_rgb(0x10, 0xB9, 0x81),
                warn: Color32::from_rgb(0xF5, 0x9E, 0x0B),
                error: Color32::from_rgb(0xEF, 0x44, 0x44),
                is_dark: true,
            },
            ThemeId::Cool => Palette {
                background: Color32::from_rgb(0x01, 0x02, 0x30), // `#010230` (deep space navy)
                panel: Color32::from_rgb(0x03, 0x04, 0x5E),      // `#03045e` (navy blue)
                panel_alt: Color32::from_rgb(0x00, 0x47, 0x70),  // `#004770` (deep steel blue)
                border: Color32::from_rgb(0x00, 0x77, 0xB6),     // `#0077b6` (star command blue)
                text: Color32::from_rgb(0xCA, 0xF0, 0xF8),       // `#caf0f8` (powder blue)
                text_muted: Color32::from_rgb(0x90, 0xE0, 0xEF), // `#90e0ef` (sky blue)
                accent: Color32::from_rgb(0x00, 0xB4, 0xD8),     // `#00b4d8` (cerulean blue)
                accent_strong: Color32::from_rgb(0x00, 0x77, 0xB6), // `#0077b6` (star command blue)
                success: Color32::from_rgb(0x10, 0xB9, 0x81),
                warn: Color32::from_rgb(0xF5, 0x9E, 0x0B),
                error: Color32::from_rgb(0xEF, 0x44, 0x44),
                is_dark: true,
            },
            ThemeId::Hacker => Palette {
                background: Color32::from_rgb(0x05, 0x08, 0x05), // Deep near-black charcoal green
                panel: Color32::from_rgb(0x0C, 0x14, 0x0C),      // Dark hacker green panel
                panel_alt: Color32::from_rgb(0x15, 0x22, 0x15),  // Distinct dark terminal panel
                border: Color32::from_rgb(0x22, 0x3C, 0x22),     // Muted matrix green border
                text: Color32::from_rgb(0x39, 0xFF, 0x14),       // Outrageous neon lime green
                text_muted: Color32::from_rgb(0x00, 0xAA, 0x00), // Pure green text
                accent: Color32::from_rgb(0x39, 0xFF, 0x14),     // Neon lime green accent
                accent_strong: Color32::from_rgb(0x00, 0xDD, 0x00), // Stronger green accent
                success: Color32::from_rgb(0x00, 0xFF, 0x66),
                warn: Color32::from_rgb(0xFF, 0xCC, 0x00),
                error: Color32::from_rgb(0xFF, 0x33, 0x33),
                is_dark: true,
            },
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub background: Color32,
    pub panel: Color32,
    pub panel_alt: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub accent: Color32,
    pub accent_strong: Color32,
    pub success: Color32,
    pub warn: Color32,
    pub error: Color32,
    pub is_dark: bool,
}

/// Apply `palette` to the egui [`Visuals`] used by the root context.
pub fn apply(palette: Palette, ctx: &eframe::egui::Context) {
    let mut visuals = if palette.is_dark { Visuals::dark() } else { Visuals::light() };

    visuals.override_text_color = Some(palette.text);
    visuals.window_fill = palette.panel;
    visuals.panel_fill = palette.background;
    visuals.extreme_bg_color = palette.panel_alt;
    visuals.faint_bg_color = palette.panel_alt;
    visuals.window_stroke = Stroke::new(1.0_f32, palette.border);
    visuals.window_shadow = Shadow::default();
    visuals.popup_shadow = Shadow::default();
    visuals.selection = Selection {
        bg_fill: palette.accent.gamma_multiply(0.35),
        stroke: Stroke::new(1.0_f32, palette.accent_strong),
    };
    visuals.hyperlink_color = palette.accent_strong;

    let round = Rounding::same(4.0); // Sleek modern sharp-ish corners
    let widgets = Widgets {
        noninteractive: WidgetVisuals {
            bg_fill: palette.panel,
            weak_bg_fill: palette.panel,
            bg_stroke: Stroke::new(1.0_f32, palette.border),
            rounding: round,
            fg_stroke: Stroke::new(1.0_f32, palette.text_muted),
            expansion: 0.0,
        },
        inactive: WidgetVisuals {
            bg_fill: palette.panel_alt,
            weak_bg_fill: palette.panel_alt,
            bg_stroke: Stroke::new(1.0_f32, palette.border), // subtle borders
            rounding: round,
            fg_stroke: Stroke::new(1.0_f32, palette.text),
            expansion: 0.0,
        },
        hovered: WidgetVisuals {
            bg_fill: palette.accent.gamma_multiply(0.20),
            weak_bg_fill: palette.accent.gamma_multiply(0.10),
            bg_stroke: Stroke::new(1.0_f32, palette.accent), // highlight border on hover
            rounding: round,
            fg_stroke: Stroke::new(1.0_f32, palette.text),
            expansion: 0.0,
        },
        active: WidgetVisuals {
            bg_fill: palette.accent,
            weak_bg_fill: palette.accent.gamma_multiply(0.80),
            bg_stroke: Stroke::new(1.0_f32, palette.accent_strong),
            rounding: round,
            fg_stroke: Stroke::new(1.0_f32, Color32::WHITE),
            expansion: 0.0,
        },
        open: WidgetVisuals {
            bg_fill: palette.panel_alt,
            weak_bg_fill: palette.panel_alt,
            bg_stroke: Stroke::new(1.0_f32, palette.accent),
            rounding: round,
            fg_stroke: Stroke::new(1.0_f32, palette.text),
            expansion: 0.0,
        },
    };
    visuals.widgets = widgets;

    ctx.set_visuals(visuals);
}
