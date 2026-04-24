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
    DarkPurple,
    DarkBlue,
    DarkTeal,
    Light,
}

impl ThemeId {
    pub const ALL: &'static [ThemeId] =
        &[ThemeId::DarkPurple, ThemeId::DarkBlue, ThemeId::DarkTeal, ThemeId::Light];

    pub fn label(self) -> &'static str {
        match self {
            ThemeId::DarkPurple => "Dark Purple",
            ThemeId::DarkBlue => "Dark Blue",
            ThemeId::DarkTeal => "Dark Teal",
            ThemeId::Light => "Light",
        }
    }

    pub fn palette(self) -> Palette {
        match self {
            ThemeId::DarkPurple => Palette {
                background: Color32::from_rgb(0x0E, 0x0B, 0x18),
                panel: Color32::from_rgb(0x18, 0x12, 0x2A),
                panel_alt: Color32::from_rgb(0x1F, 0x18, 0x36),
                border: Color32::from_rgb(0x3A, 0x2E, 0x5A),
                text: Color32::from_rgb(0xE8, 0xE4, 0xF2),
                text_muted: Color32::from_rgb(0xA8, 0xA0, 0xC0),
                accent: Color32::from_rgb(0x8B, 0x5C, 0xF6),
                accent_strong: Color32::from_rgb(0xA8, 0x7C, 0xFF),
                success: Color32::from_rgb(0x34, 0xD3, 0x99),
                warn: Color32::from_rgb(0xF5, 0xA6, 0x3D),
                error: Color32::from_rgb(0xF4, 0x72, 0x7E),
                header_badge: Color32::from_rgb(0x7C, 0x3A, 0xED),
                smart_backup: Color32::from_rgb(0x14, 0xB8, 0xA6),
                is_dark: true,
            },
            ThemeId::DarkBlue => Palette {
                background: Color32::from_rgb(0x0A, 0x10, 0x1C),
                panel: Color32::from_rgb(0x11, 0x1B, 0x2E),
                panel_alt: Color32::from_rgb(0x16, 0x24, 0x3C),
                border: Color32::from_rgb(0x27, 0x3A, 0x5A),
                text: Color32::from_rgb(0xE4, 0xEC, 0xF7),
                text_muted: Color32::from_rgb(0x90, 0xA4, 0xC0),
                accent: Color32::from_rgb(0x3B, 0x82, 0xF6),
                accent_strong: Color32::from_rgb(0x60, 0xA5, 0xFA),
                success: Color32::from_rgb(0x34, 0xD3, 0x99),
                warn: Color32::from_rgb(0xF5, 0xA6, 0x3D),
                error: Color32::from_rgb(0xF4, 0x72, 0x7E),
                header_badge: Color32::from_rgb(0x25, 0x63, 0xEB),
                smart_backup: Color32::from_rgb(0x14, 0xB8, 0xA6),
                is_dark: true,
            },
            ThemeId::DarkTeal => Palette {
                background: Color32::from_rgb(0x08, 0x14, 0x14),
                panel: Color32::from_rgb(0x0E, 0x1F, 0x20),
                panel_alt: Color32::from_rgb(0x13, 0x2B, 0x2D),
                border: Color32::from_rgb(0x24, 0x48, 0x4A),
                text: Color32::from_rgb(0xE2, 0xF3, 0xF1),
                text_muted: Color32::from_rgb(0x90, 0xB5, 0xB0),
                accent: Color32::from_rgb(0x14, 0xB8, 0xA6),
                accent_strong: Color32::from_rgb(0x2D, 0xD4, 0xBF),
                success: Color32::from_rgb(0x34, 0xD3, 0x99),
                warn: Color32::from_rgb(0xF5, 0xA6, 0x3D),
                error: Color32::from_rgb(0xF4, 0x72, 0x7E),
                header_badge: Color32::from_rgb(0x0F, 0x76, 0x6E),
                smart_backup: Color32::from_rgb(0x14, 0xB8, 0xA6),
                is_dark: true,
            },
            ThemeId::Light => Palette {
                background: Color32::from_rgb(0xF4, 0xF2, 0xFA),
                panel: Color32::from_rgb(0xFF, 0xFF, 0xFF),
                panel_alt: Color32::from_rgb(0xEC, 0xE7, 0xF7),
                border: Color32::from_rgb(0xC8, 0xBF, 0xDA),
                text: Color32::from_rgb(0x1A, 0x16, 0x28),
                text_muted: Color32::from_rgb(0x6A, 0x60, 0x80),
                accent: Color32::from_rgb(0x7C, 0x3A, 0xED),
                accent_strong: Color32::from_rgb(0x6D, 0x28, 0xD9),
                success: Color32::from_rgb(0x0E, 0x9F, 0x6E),
                warn: Color32::from_rgb(0xD9, 0x77, 0x06),
                error: Color32::from_rgb(0xDC, 0x26, 0x26),
                header_badge: Color32::from_rgb(0x7C, 0x3A, 0xED),
                smart_backup: Color32::from_rgb(0x0D, 0x94, 0x88),
                is_dark: false,
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
    pub header_badge: Color32,
    pub smart_backup: Color32,
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

    let round = Rounding::same(6.0);
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
            bg_stroke: Stroke::new(1.0_f32, palette.border),
            rounding: round,
            fg_stroke: Stroke::new(1.0_f32, palette.text),
            expansion: 0.0,
        },
        hovered: WidgetVisuals {
            bg_fill: palette.accent.gamma_multiply(0.30),
            weak_bg_fill: palette.accent.gamma_multiply(0.15),
            bg_stroke: Stroke::new(1.0_f32, palette.accent),
            rounding: round,
            fg_stroke: Stroke::new(1.0_f32, palette.text),
            expansion: 1.0,
        },
        active: WidgetVisuals {
            bg_fill: palette.accent,
            weak_bg_fill: palette.accent.gamma_multiply(0.70),
            bg_stroke: Stroke::new(1.0_f32, palette.accent_strong),
            rounding: round,
            fg_stroke: Stroke::new(1.0_f32, Color32::WHITE),
            expansion: 1.0,
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
