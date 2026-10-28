//! The icons of the settings window and of the setup window. Each page has a color icon of the
//! Fluent UI System Icons in the navigation, as in the Settings app, and each step of the setup has
//! a larger one above its title. High contrast shows the symbol of the page or of the step in the
//! text color instead, because a color icon does not follow the colors of the theme. Each card of a
//! setting starts with a glyph of the icon font in the text color.

use typealike_core::settings_window::view::{PageIcon, RowIcon};
use typealike_core::setup::SetupStep;
use windows_reactor::{EncodedImage, Icon, Symbol};

use crate::system_theme::Contrast;

const ROBOT_GLYPH: &str = "\u{E99A}";
const LIGHTBULB_GLYPH: &str = "\u{EA80}";
const EDIT_GLYPH: &str = "\u{E70F}";
const PERMISSIONS_GLYPH: &str = "\u{E8D7}";
const CONTACT_GLYPH: &str = "\u{E779}";
const ACCEPT_GLYPH: &str = "\u{E8FB}";

/// The color picture above the title of `step` in the setup window, as an SVG file.
pub(super) fn setup_picture(step: SetupStep) -> &'static [u8] {
    match step {
        SetupStep::Welcome => include_bytes!("../../assets/fluent/setup/text_edit_style.svg"),
        SetupStep::Model => include_bytes!("../../assets/fluent/setup/bot.svg"),
        SetupStep::Permission => include_bytes!("../../assets/fluent/setup/shield_checkmark.svg"),
        SetupStep::About => include_bytes!("../../assets/fluent/setup/contact_card.svg"),
        SetupStep::Ready => include_bytes!("../../assets/fluent/setup/checkmark_circle.svg"),
    }
}

/// The glyph above the title of `step` in the setup window in high contrast. Each glyph is in
/// Segoe Fluent Icons and also in Segoe MDL2 Assets.
pub(super) fn setup_glyph(step: SetupStep) -> &'static str {
    match step {
        SetupStep::Welcome => EDIT_GLYPH,
        SetupStep::Model => ROBOT_GLYPH,
        SetupStep::Permission => PERMISSIONS_GLYPH,
        SetupStep::About => CONTACT_GLYPH,
        SetupStep::Ready => ACCEPT_GLYPH,
    }
}

/// The glyph that shows `icon` at the leading edge of a card. Each glyph is in Segoe Fluent Icons
/// and also in Segoe MDL2 Assets, the icon font of Windows 10.
pub(super) fn row_glyph(icon: RowIcon) -> &'static str {
    match icon {
        RowIcon::Power => "\u{E7E8}",
        RowIcon::Clipboard => "\u{E77F}",
        RowIcon::LightningBolt => "\u{E945}",
        RowIcon::Stopwatch => "\u{E916}",
        RowIcon::WarningSign => "\u{E7BA}",
        RowIcon::SpellCheck => "\u{F87B}",
        RowIcon::Ruler => "\u{ECC6}",
        RowIcon::StopButton => "\u{E71A}",
        RowIcon::Robot => ROBOT_GLYPH,
        RowIcon::Chip => "\u{E950}",
        RowIcon::Person => "\u{E77B}",
        RowIcon::Note => "\u{E70B}",
        RowIcon::Monitor => "\u{E7F4}",
        RowIcon::Accessibility => "\u{E776}",
        RowIcon::Gauge => "\u{EC4A}",
        RowIcon::ClockArrow => "\u{E81C}",
        RowIcon::FloppyDisk => "\u{E74E}",
        RowIcon::Letters => "\u{E8D2}",
        RowIcon::Pencil => "\u{E70F}",
        RowIcon::Moon => "\u{E708}",
        RowIcon::Battery => "\u{EA95}",
        RowIcon::Game => "\u{E7FC}",
        RowIcon::MagnifyingGlass => "\u{E721}",
        RowIcon::Globe => "\u{E774}",
    }
}

/// The icon of a page with `icon` in the navigation, while the contrast is `contrast`.
pub(super) fn page_icon(icon: PageIcon, contrast: Contrast) -> Icon {
    match contrast {
        Contrast::Standard => Icon::image_svg(EncodedImage::from_static(color_page_icon(icon))),
        Contrast::High => symbol_page_icon(icon),
    }
}

fn color_page_icon(icon: PageIcon) -> &'static [u8] {
    match icon {
        // Windows asks for no permission, so the Permissions page never shows here.
        PageIcon::Gear | PageIcon::Hand => include_bytes!("../../assets/fluent/settings.svg"),
        PageIcon::Cube => include_bytes!("../../assets/fluent/bot.svg"),
        PageIcon::TextCursor => include_bytes!("../../assets/fluent/edit.svg"),
        PageIcon::TextViewfinder => include_bytes!("../../assets/fluent/scan_type.svg"),
        PageIcon::PersonText => include_bytes!("../../assets/fluent/contact_card.svg"),
        PageIcon::AppGrid => include_bytes!("../../assets/fluent/apps.svg"),
        PageIcon::Lightbulb => include_bytes!("../../assets/fluent/lightbulb.svg"),
    }
}

fn symbol_page_icon(icon: PageIcon) -> Icon {
    match icon {
        PageIcon::Gear => Symbol::Setting.into(),
        PageIcon::Cube => Icon::font(ROBOT_GLYPH),
        PageIcon::TextCursor => Symbol::Edit.into(),
        PageIcon::TextViewfinder => Symbol::Scan.into(),
        PageIcon::PersonText => Symbol::ContactInfo.into(),
        PageIcon::AppGrid => Symbol::AllApps.into(),
        PageIcon::Lightbulb => Icon::font(LIGHTBULB_GLYPH),
        PageIcon::Hand => Symbol::Permissions.into(),
    }
}
