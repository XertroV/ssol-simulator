//! Shared Open SSOL visual language for Bevy UI.
//!
//! Cosmic dark surfaces, cool cyan light-accents (c / relativity), amber warnings.
//! Prefer compact instrument panels over bare white text.

use bevy::prelude::*;

/// Deep void behind translucent panels.
#[allow(dead_code)]
pub const VOID: Color = Color::srgba(0.02, 0.03, 0.05, 0.82);
/// Primary panel fill.
pub const PANEL: Color = Color::srgba(0.045, 0.055, 0.075, 0.92);
/// Nested / inset well.
pub const WELL: Color = Color::srgba(0.025, 0.03, 0.045, 0.88);
/// Interactive row idle.
pub const ROW: Color = Color::srgba(0.09, 0.10, 0.14, 0.9);
/// Cool border (light / horizon).
pub const BORDER: Color = Color::srgba(0.55, 0.68, 0.92, 0.22);
/// Stronger border for focus / primary chrome.
pub const BORDER_STRONG: Color = Color::srgba(0.65, 0.78, 1.0, 0.38);
/// Primary text.
pub const TEXT: Color = Color::srgba(0.96, 0.97, 1.0, 0.98);
/// Secondary / labels.
pub const TEXT_DIM: Color = Color::srgba(0.62, 0.70, 0.82, 0.92);
/// Muted captions.
pub const TEXT_MUTED: Color = Color::srgba(0.48, 0.55, 0.66, 0.9);
/// Relativistic accent (c, light).
pub const ACCENT: Color = Color::srgba(0.45, 0.78, 1.0, 0.98);
/// Soft accent fill (sliders, highlights).
#[allow(dead_code)]
pub const ACCENT_SOFT: Color = Color::srgba(0.35, 0.58, 0.92, 0.95);
/// Warning / conflict.
pub const WARN: Color = Color::srgba(1.0, 0.58, 0.22, 0.98);
/// Warning surface.
pub const WARN_BG: Color = Color::srgba(0.16, 0.07, 0.02, 0.94);
/// Info surface.
pub const INFO_BG: Color = Color::srgba(0.05, 0.08, 0.14, 0.94);
/// Success / orbs collected.
pub const SUCCESS: Color = Color::srgba(0.45, 0.95, 0.72, 0.98);
/// Overlay scrim.
pub const SCRIM: Color = Color::srgba(0.01, 0.015, 0.03, 0.72);

pub const RADIUS_SM: f32 = 8.0;
pub const RADIUS_MD: f32 = 12.0;
pub const RADIUS_LG: f32 = 16.0;
pub const RADIUS_PILL: f32 = 99.0;

pub fn text_font(font: Handle<Font>, size: f32) -> TextFont {
    TextFont {
        font: font.into(),
        font_size: FontSize::Px(size),
        ..default()
    }
}

#[allow(dead_code)]
pub fn panel_node(padding: f32) -> Node {
    Node {
        padding: UiRect::all(Val::Px(padding)),
        border: UiRect::all(Val::Px(1.0)),
        border_radius: BorderRadius::all(Val::Px(RADIUS_MD)),
        flex_direction: FlexDirection::Column,
        ..default()
    }
}

#[allow(dead_code)]
pub fn hud_chip() -> (Node, BackgroundColor, BorderColor) {
    (
        Node {
            padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
            border: UiRect::all(Val::Px(1.0)),
            border_radius: BorderRadius::all(Val::Px(RADIUS_SM)),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(2.0),
            ..default()
        },
        BackgroundColor(PANEL),
        BorderColor::all(BORDER),
    )
}
