//! Cos theme: colors and metrics matching the SwiftUI implementation for
//! System / Light / Dark / True Dark appearances.

use cos_core::AppearanceMode;
use gpui::{hsla, Hsla, WindowAppearance};

pub const SIDEBAR_WIDTH: f32 = 236.0;
pub const COMPOSER_RADIUS: f32 = 18.0;

pub const BLUE: Hsla = Hsla { h: 0.597, s: 1.0, l: 0.632, a: 1.0 }; // rgb(0.23, 0.57, 1.0)
pub const VIOLET: Hsla = Hsla { h: 0.748, s: 0.95, l: 0.67, a: 1.0 }; // rgb(0.54, 0.36, 0.98)
pub const ORANGE: Hsla = Hsla { h: 0.063, s: 1.0, l: 0.59, a: 1.0 }; // rgb(1.0, 0.48, 0.18)
pub const GREEN: Hsla = Hsla { h: 0.36, s: 0.65, l: 0.45, a: 1.0 };
pub const INDIGO: Hsla = Hsla { h: 0.66, s: 0.6, l: 0.6, a: 1.0 };
pub const RED: Hsla = Hsla { h: 0.0, s: 0.7, l: 0.55, a: 1.0 };

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    pub mode: AppearanceMode,
    pub dark: bool,
    pub true_dark: bool,
    // Surfaces
    pub window_background: Hsla,
    pub sidebar_background: Hsla,
    pub chat_top: Hsla,
    pub chat_bottom: Hsla,
    pub card_background: Hsla,
    pub card_border: Hsla,
    pub composer_background: Hsla,
    // Text
    pub primary: Hsla,
    pub secondary: Hsla,
    pub tertiary: Hsla,
    // Overlays mapped from SwiftUI `.primary.opacity(n)`
    pub fill_011: Hsla,
    pub fill_032: Hsla,
    pub fill_045: Hsla,
    pub fill_05: Hsla,
    pub fill_055: Hsla,
    pub fill_06: Hsla,
    pub fill_075: Hsla,
    pub fill_10: Hsla,
    pub fill_12: Hsla,
    pub divider: Hsla,
}

impl Theme {
    pub fn resolve(mode: AppearanceMode, system: WindowAppearance) -> Self {
        let system_dark = matches!(system, WindowAppearance::Dark | WindowAppearance::VibrantDark);
        let (dark, true_dark) = match mode {
            AppearanceMode::System => (system_dark, false),
            AppearanceMode::Light => (false, false),
            AppearanceMode::Dark => (true, false),
            AppearanceMode::TrueDark => (true, true),
        };
        let base = if dark { 1.0 } else { 0.0 };
        let fill = |alpha: f32| hsla(0.0, 0.0, base, alpha);
        let primary = fill(1.0);
        let secondary = fill(0.55);
        let tertiary = fill(0.35);

        let (window_background, sidebar_background, chat_top, chat_bottom) = if true_dark {
            (hsla(0.0, 0.0, 0.0, 1.0), hsla(0.0, 0.0, 0.0, 1.0), hsla(0.0, 0.0, 0.0, 1.0), hsla(0.0, 0.0, 0.0, 1.0))
        } else if dark {
            // NSColor windowBackground / textBackground approximations (darkAqua)
            (
                hsla(0.0, 0.0, 0.157, 1.0),
                hsla(0.0, 0.0, 0.14, 1.0),
                hsla(0.0, 0.0, 0.122, 0.72),
                hsla(0.0, 0.0, 0.157, 1.0),
            )
        } else {
            (
                hsla(0.0, 0.0, 0.925, 1.0),
                hsla(0.0, 0.0, 0.96, 1.0),
                hsla(0.0, 0.0, 1.0, 0.72),
                hsla(0.0, 0.0, 0.925, 1.0),
            )
        };

        Self {
            mode,
            dark,
            true_dark,
            window_background,
            sidebar_background,
            chat_top,
            chat_bottom,
            card_background: fill(0.045),
            card_border: hsla(0.0, 0.0, 1.0, 0.08),
            composer_background: if true_dark {
                hsla(0.0, 0.0, 0.0, 1.0)
            } else if dark {
                hsla(0.0, 0.0, 0.19, 1.0)
            } else {
                hsla(0.0, 0.0, 0.985, 1.0)
            },
            primary,
            secondary,
            tertiary,
            fill_011: fill(0.11),
            fill_032: fill(0.032),
            fill_045: fill(0.045),
            fill_05: fill(0.05),
            fill_055: fill(0.055),
            fill_06: fill(0.06),
            fill_075: fill(0.075),
            fill_10: fill(0.10),
            fill_12: fill(0.12),
            divider: fill(0.14),
        }
    }
}

pub fn format_number(value: i64) -> String {
    let digits = value.to_string();
    let negative = digits.starts_with('-');
    let digits = digits.trim_start_matches('-');
    let mut out = String::new();
    for (index, c) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if negative {
        format!("-{out}")
    } else {
        out
    }
}
