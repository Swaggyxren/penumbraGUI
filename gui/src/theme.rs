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
    PenumbraTactical,
}

impl ThemeId {
    pub const ALL: &'static [ThemeId] = &[ThemeId::PenumbraTactical];

    pub fn label(self) -> &'static str {
        match self {
            ThemeId::PenumbraTactical => "Penumbra Tactical",
        }
    }

    pub fn palette(self) -> Palette {
        match self {
            ThemeId::PenumbraTactical => Palette {
                background: Color32::from_rgb(0x0D, 0x0F, 0x14), // #0d0f14
                panel: Color32::from_rgb(0x16, 0x19, 0x20),      // #161920
                panel_alt: Color32::from_rgb(0x1E, 0x1F, 0x25),  // #1e1f25
                border: Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x0F), // #ffffff0f
                text: Color32::from_rgb(0xE2, 0xE2, 0xE9),
                text_muted: Color32::from_rgb(0xC9, 0xC4, 0xD7), // #c9c4d7
                accent: Color32::from_rgb(0x7C, 0x6A, 0xF7),     // #7c6af7
                accent_strong: Color32::from_rgb(0x5A, 0x46, 0xD3), // inverse_primary
                success: Color32::from_rgb(0x4A, 0xDE, 0x80),    // #4ade80
                warn: Color32::from_rgb(0xFF, 0xB8, 0x6D),       // tertiary
                error: Color32::from_rgb(0xF8, 0x71, 0x71),      // #f87171
                header_badge: Color32::from_rgb(0x7C, 0x6A, 0xF7), // accent
                smart_backup: Color32::from_rgb(0x4A, 0xDE, 0x80), // success
                is_dark: true,
            },
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
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

    let round = Rounding::same(4.0);
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
