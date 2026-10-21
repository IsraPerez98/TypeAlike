//! The user's appearance settings for the parts that GhostType draws with Win32: light or dark
//! mode and high contrast, and the colors and the menu theme that follow them.

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::Graphics::Gdi::{COLOR_WINDOW, COLOR_WINDOWTEXT, GetSysColor, SYS_COLOR_INDEX};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW};
use windows::Win32::System::Registry::RRF_RT_REG_DWORD;
use windows::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
use windows::Win32::UI::WindowsAndMessaging::{
    SPI_GETHIGHCONTRAST, SPI_SETHIGHCONTRAST, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
};
use windows::core::{PCSTR, PCWSTR, w};

use crate::pixels::Rgb;
use crate::registry::read_value;

const PERSONALIZE_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
const UXTHEME: PCWSTR = w!("uxtheme.dll");
const SET_PREFERRED_APP_MODE_ORDINAL: usize = 135;
const FLUSH_MENU_THEMES_ORDINAL: usize = 136;
const APP_MODE_DEFAULT: i32 = 0;
const APP_MODE_FORCE_DARK: i32 = 2;
const APP_MODE_FORCE_LIGHT: i32 = 3;
const COLOR_SET_CHANGE: &str = "ImmersiveColorSet";
/// The light and dark colors follow the Fluent tooltip: its fill, then the surface stroke and the
/// secondary text, each blended over the fill.
const LIGHT_COLORS: TooltipColors = TooltipColors {
    fill: Rgb::gray(0xF9),
    border: Rgb::gray(0xC4),
    text: Rgb::gray(0x5F),
};
const DARK_COLORS: TooltipColors = TooltipColors {
    fill: Rgb::gray(0x2C),
    border: Rgb::gray(0x49),
    text: Rgb::gray(0xCF),
};

/// Light or dark mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ThemeMode {
    Light,
    Dark,
}

/// Whether high contrast is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Contrast {
    Standard,
    High,
}

/// The appearance that app windows must follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SystemTheme {
    pub(crate) mode: ThemeMode,
    pub(crate) contrast: Contrast,
}

impl SystemTheme {
    /// Reads the current settings for app windows.
    pub(crate) fn read() -> Self {
        let mode = match read_personalize_flag(w!("AppsUseLightTheme")) {
            Some(0) => ThemeMode::Dark,
            _ => ThemeMode::Light,
        };
        let contrast = if is_high_contrast_on() {
            Contrast::High
        } else {
            Contrast::Standard
        };
        Self { mode, contrast }
    }
}

/// The colors of the badge and of the mirror below the caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TooltipColors {
    pub(crate) fill: Rgb,
    pub(crate) border: Rgb,
    pub(crate) text: Rgb,
}

impl TooltipColors {
    pub(crate) fn for_theme(theme: SystemTheme) -> Self {
        match (theme.contrast, theme.mode) {
            (Contrast::High, _) => Self::high_contrast(),
            (Contrast::Standard, ThemeMode::Light) => LIGHT_COLORS,
            (Contrast::Standard, ThemeMode::Dark) => DARK_COLORS,
        }
    }

    fn high_contrast() -> Self {
        let (window, text) = (system_color(COLOR_WINDOW), system_color(COLOR_WINDOWTEXT));
        Self {
            fill: window,
            border: text,
            text,
        }
    }
}

/// Reads the color that the system gives to `index`, which high contrast sets.
pub(crate) fn system_color(index: SYS_COLOR_INDEX) -> Rgb {
    // SAFETY: the call takes no pointer, and it gives a color for each index of its type.
    Rgb::from_colorref(unsafe { GetSysColor(index) })
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
        (Contrast::High, _) => APP_MODE_DEFAULT,
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
    let mut value = [0_u8; size_of::<u32>()];
    read_value(PERSONALIZE_KEY, name, RRF_RT_REG_DWORD, &mut value)?;
    Some(u32::from_le_bytes(value))
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
