//! The user's appearance settings: light or dark mode, accent color and high contrast, and the
//! menu theme that follows them.

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::Graphics::Gdi::{
    COLOR_BTNFACE, COLOR_BTNTEXT, COLOR_GRAYTEXT, COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT, COLOR_WINDOW,
    COLOR_WINDOWTEXT, GetSysColor,
};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW};
use windows::Win32::System::Registry::{RRF_RT_REG_BINARY, RRF_RT_REG_DWORD};
use windows::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
use windows::Win32::UI::WindowsAndMessaging::{
    SPI_GETHIGHCONTRAST, SPI_SETHIGHCONTRAST, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
};
use windows::core::{PCSTR, PCWSTR, w};

use crate::pixels::Rgb;
use crate::registry::read_value;

const PERSONALIZE_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
const DWM_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\DWM");
const ACCENT_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Accent");
const DEFAULT_ACCENT: AccentShades = AccentShades {
    on_dark: Rgb {
        red: 0x4C,
        green: 0xC2,
        blue: 0xFF,
    },
    on_light: Rgb {
        red: 0x00,
        green: 0x67,
        blue: 0xC0,
    },
};
const UXTHEME: PCWSTR = w!("uxtheme.dll");
const SET_PREFERRED_APP_MODE_ORDINAL: usize = 135;
const FLUSH_MENU_THEMES_ORDINAL: usize = 136;
const APP_MODE_DEFAULT: i32 = 0;
const APP_MODE_FORCE_DARK: i32 = 2;
const APP_MODE_FORCE_LIGHT: i32 = 3;
const PALETTE_ENTRY_BYTES: usize = 4;
const PALETTE_BYTES: usize = 8 * PALETTE_ENTRY_BYTES;
const LIGHT2_ENTRY: usize = 1;
const DARK1_ENTRY: usize = 4;
const COLOR_SET_CHANGE: &str = "ImmersiveColorSet";

/// Light or dark mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ThemeMode {
    Light,
    Dark,
}

/// The system colors that replace every other color while high contrast is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SystemColors {
    pub(crate) window: Rgb,
    pub(crate) window_text: Rgb,
    pub(crate) highlight: Rgb,
    pub(crate) highlight_text: Rgb,
    pub(crate) gray_text: Rgb,
    pub(crate) button_face: Rgb,
    pub(crate) button_text: Rgb,
}

/// The shades of the user's accent color that Fluent fills with, one for each theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AccentShades {
    /// The light shade (`SystemAccentColorLight2`) that reads on dark backgrounds.
    pub(crate) on_dark: Rgb,
    /// The dark shade (`SystemAccentColorDark1`) that reads on light backgrounds.
    pub(crate) on_light: Rgb,
}

impl AccentShades {
    /// Reads the `AccentPalette` registry value: eight `R G B _` entries from lightest to darkest.
    pub(crate) fn from_palette(palette: &[u8]) -> Option<Self> {
        let entry = |index: usize| {
            let start = index * PALETTE_ENTRY_BYTES;
            match palette.get(start..start + 3)? {
                &[red, green, blue] => Some(Rgb { red, green, blue }),
                _ => None,
            }
        };
        if palette.len() < PALETTE_BYTES {
            return None;
        }
        Some(Self {
            on_dark: entry(LIGHT2_ENTRY)?,
            on_light: entry(DARK1_ENTRY)?,
        })
    }

    fn uniform(accent: Rgb) -> Self {
        Self {
            on_dark: accent,
            on_light: accent,
        }
    }
}

/// Whether high contrast is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Contrast {
    Standard,
    High(SystemColors),
}

/// The appearance that app windows must follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SystemTheme {
    pub(crate) mode: ThemeMode,
    pub(crate) contrast: Contrast,
    pub(crate) accent: AccentShades,
}

impl SystemTheme {
    /// Reads the current settings for app windows.
    pub(crate) fn read() -> Self {
        let mode = match read_personalize_flag(w!("AppsUseLightTheme")) {
            Some(0) => ThemeMode::Dark,
            _ => ThemeMode::Light,
        };
        let contrast = if is_high_contrast_on() {
            Contrast::High(system_colors())
        } else {
            Contrast::Standard
        };
        let accent = read_accent_palette()
            .and_then(|palette| AccentShades::from_palette(&palette))
            .or_else(|| {
                read_dword(DWM_KEY, w!("AccentColor"))
                    .map(|value| AccentShades::uniform(Rgb::from_colorref(value)))
            })
            .unwrap_or(DEFAULT_ACCENT);
        Self {
            mode,
            contrast,
            accent,
        }
    }
}

type SetPreferredAppMode = unsafe extern "system" fn(i32) -> i32;
type FlushMenuThemes = unsafe extern "system" fn();
type Export = unsafe extern "system" fn() -> isize;

/// Makes the menus of this process follow `theme`: dark menus in dark mode.
///
/// Windows has no documented call for this. uxtheme exports one by ordinal (135 and 136) since
/// Windows 10 1903, and apps such as Notepad++ use it. Without the exports, menus keep the
/// system look.
pub(crate) fn match_menus_to(theme: SystemTheme) {
    let mode = match (theme.contrast, theme.mode) {
        (Contrast::High(_), _) => APP_MODE_DEFAULT,
        (Contrast::Standard, ThemeMode::Dark) => APP_MODE_FORCE_DARK,
        (Contrast::Standard, ThemeMode::Light) => APP_MODE_FORCE_LIGHT,
    };
    // SAFETY: plain call; the module stays loaded for the rest of the process, as theming needs.
    let Ok(module) = (unsafe { LoadLibraryExW(UXTHEME, None, LOAD_LIBRARY_SEARCH_SYSTEM32) }) else {
        return;
    };
    // SAFETY: the module is loaded; an ordinal in place of a name is valid for GetProcAddress.
    let exports = unsafe {
        (
            GetProcAddress(module, ordinal(SET_PREFERRED_APP_MODE_ORDINAL)),
            GetProcAddress(module, ordinal(FLUSH_MENU_THEMES_ORDINAL)),
        )
    };
    let (Some(set_mode), Some(flush)) = exports else {
        tracing::debug!("dark menus unavailable");
        return;
    };
    // SAFETY: every Windows build that has these ordinals exports these signatures at them.
    unsafe {
        let set_mode = std::mem::transmute::<Export, SetPreferredAppMode>(set_mode);
        let flush = std::mem::transmute::<Export, FlushMenuThemes>(flush);
        set_mode(mode);
        flush();
    }
}

/// Whether a `WM_SETTINGCHANGE` with `wparam` and `lparam` tells that the theme or the contrast
/// changed.
pub(crate) fn is_appearance_change(wparam: WPARAM, lparam: LPARAM) -> bool {
    u32::try_from(wparam.0) == Ok(SPI_SETHIGHCONTRAST.0) || is_color_set_change(lparam)
}

fn is_color_set_change(lparam: LPARAM) -> bool {
    if lparam.0 == 0 {
        return false;
    }
    let area = PCWSTR(std::ptr::with_exposed_provenance(lparam.0.cast_unsigned()));
    // SAFETY: for WM_SETTINGCHANGE a non-zero lparam is a nul-terminated string valid for the message.
    unsafe { area.to_string() }.is_ok_and(|area| area == COLOR_SET_CHANGE)
}

fn ordinal(value: usize) -> PCSTR {
    PCSTR(std::ptr::without_provenance(value))
}

/// Whether the taskbar and notification area use the light theme.
pub(crate) fn is_taskbar_light() -> bool {
    read_personalize_flag(w!("SystemUsesLightTheme")).is_some_and(|value| value != 0)
}

fn read_personalize_flag(name: PCWSTR) -> Option<u32> {
    read_dword(PERSONALIZE_KEY, name)
}

fn read_dword(key: PCWSTR, name: PCWSTR) -> Option<u32> {
    let mut value = [0_u8; size_of::<u32>()];
    read_value(key, name, RRF_RT_REG_DWORD, &mut value)?;
    Some(u32::from_le_bytes(value))
}

fn read_accent_palette() -> Option<[u8; PALETTE_BYTES]> {
    let mut palette = [0_u8; PALETTE_BYTES];
    let size = read_value(ACCENT_KEY, w!("AccentPalette"), RRF_RT_REG_BINARY, &mut palette)?;
    (size == PALETTE_BYTES).then_some(palette)
}

fn is_high_contrast_on() -> bool {
    let mut settings = HIGHCONTRASTW {
        cbSize: size_of::<HIGHCONTRASTW>()
            .try_into()
            .expect("settings size fits in u32"),
        ..HIGHCONTRASTW::default()
    };
    // SAFETY: the structure is a local whose size is set as the call requires.
    let read = unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            settings.cbSize,
            Some((&raw mut settings).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    read.is_ok() && settings.dwFlags.contains(HCF_HIGHCONTRASTON)
}

fn system_colors() -> SystemColors {
    // SAFETY: plain calls.
    let color = |index| Rgb::from_colorref(unsafe { GetSysColor(index) });
    SystemColors {
        window: color(COLOR_WINDOW),
        window_text: color(COLOR_WINDOWTEXT),
        highlight: color(COLOR_HIGHLIGHT),
        highlight_text: color(COLOR_HIGHLIGHTTEXT),
        gray_text: color(COLOR_GRAYTEXT),
        button_face: color(COLOR_BTNFACE),
        button_text: color(COLOR_BTNTEXT),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PALETTE: [u8; 32] = [
        0x99, 0xEB, 0xFF, 0x00, 0x4C, 0xC2, 0xFF, 0x00, 0x00, 0x91, 0xF8, 0x00, 0x00, 0x78, 0xD4, 0x00, 0x00,
        0x67, 0xC0, 0x00, 0x00, 0x3E, 0x92, 0x00, 0x00, 0x1A, 0x68, 0x00, 0xF7, 0x63, 0x0C, 0x00,
    ];

    #[test]
    fn accent_palette_gives_light2_and_dark1_shades() {
        assert_eq!(
            AccentShades::from_palette(&PALETTE),
            Some(AccentShades {
                on_dark: Rgb {
                    red: 0x4C,
                    green: 0xC2,
                    blue: 0xFF
                },
                on_light: Rgb {
                    red: 0x00,
                    green: 0x67,
                    blue: 0xC0
                },
            })
        );
    }

    #[test]
    fn short_accent_palette_gives_no_shades() {
        assert_eq!(AccentShades::from_palette(&PALETTE[..16]), None);
    }
}
