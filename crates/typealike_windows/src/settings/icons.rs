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
const INFO_GLYPH: &str = "\u{E946}";
const EDIT_GLYPH: &str = "\u{E70F}";
const PERMISSIONS_GLYPH: &str = "\u{E8D7}";
const CONTACT_GLYPH: &str = "\u{E779}";
const ACCEPT_GLYPH: &str = "\u{E8FB}";
const SCAN_GLYPH: &str = "\u{E8FE}";

/// The color picture above the title of `step` in the setup window, as a PNG file.
pub(super) fn setup_picture(step: SetupStep) -> &'static [u8] {
    match step {
        SetupStep::Welcome => include_bytes!("../../assets/fluent/setup/text_edit_style.png"),
        SetupStep::About => include_bytes!("../../assets/fluent/setup/contact_card.png"),
        SetupStep::Sources => include_bytes!("../../assets/fluent/setup/scan_type.png"),
        SetupStep::Permission => include_bytes!("../../assets/fluent/setup/shield_checkmark.png"),
        SetupStep::Model => include_bytes!("../../assets/fluent/setup/bot.png"),
        SetupStep::Ready => include_bytes!("../../assets/fluent/setup/checkmark_circle.png"),
    }
}

/// The glyph above the title of `step` in the setup window in high contrast. Each glyph is in
/// Segoe Fluent Icons and also in Segoe MDL2 Assets.
pub(super) fn setup_glyph(step: SetupStep) -> &'static str {
    match step {
        SetupStep::Welcome => EDIT_GLYPH,
        SetupStep::About => CONTACT_GLYPH,
        SetupStep::Sources => SCAN_GLYPH,
        SetupStep::Permission => PERMISSIONS_GLYPH,
        SetupStep::Model => ROBOT_GLYPH,
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
        RowIcon::FullScreen => "\u{E740}",
        RowIcon::MagnifyingGlass => "\u{E721}",
        RowIcon::Globe => "\u{E774}",
        RowIcon::ChatBubbles => "\u{E8F2}",
        RowIcon::CircleArrows => "\u{E895}",
    }
}

/// The icon of a page with `icon` in the navigation, while the contrast is `contrast`.
pub(super) fn page_icon(icon: PageIcon, contrast: Contrast) -> Icon {
    match contrast {
        Contrast::Standard => Icon::image_data(EncodedImage::from_static(color_page_icon(icon))),
        Contrast::High => symbol_page_icon(icon),
    }
}

fn color_page_icon(icon: PageIcon) -> &'static [u8] {
    match icon {
        // Windows asks for no permission, so the Permissions page never shows here.
        PageIcon::Gear | PageIcon::Hand => include_bytes!("../../assets/fluent/settings.png"),
        PageIcon::Cube => include_bytes!("../../assets/fluent/bot.png"),
        PageIcon::TextCursor => include_bytes!("../../assets/fluent/edit.png"),
        PageIcon::TextViewfinder => include_bytes!("../../assets/fluent/scan_type.png"),
        PageIcon::PersonText => include_bytes!("../../assets/fluent/contact_card.png"),
        PageIcon::AppGrid => include_bytes!("../../assets/fluent/apps.png"),
        PageIcon::Lightbulb => include_bytes!("../../assets/fluent/lightbulb.png"),
        PageIcon::Info => include_bytes!("../../assets/fluent/question_circle.png"),
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
        PageIcon::Info => Icon::font(INFO_GLYPH),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_picture_is_a_bitmap_for_four_times_its_size() {
        let steps = [
            SetupStep::Welcome,
            SetupStep::About,
            SetupStep::Sources,
            SetupStep::Permission,
            SetupStep::Model,
            SetupStep::Ready,
        ];

        let sizes = steps.map(|step| bitmap_size(setup_picture(step)));

        assert_eq!(sizes, [Some((224, 224)); 6]);
    }

    #[test]
    fn each_page_icon_is_a_bitmap_for_two_times_its_size() {
        let icons = [
            PageIcon::Gear,
            PageIcon::Hand,
            PageIcon::Cube,
            PageIcon::TextCursor,
            PageIcon::TextViewfinder,
            PageIcon::PersonText,
            PageIcon::AppGrid,
            PageIcon::Lightbulb,
            PageIcon::Info,
        ];

        let sizes = icons.map(|icon| bitmap_size(color_page_icon(icon)));

        assert_eq!(sizes, [Some((32, 32)); 9]);
    }

    /// The width and the height of the PNG file `bytes`, from its header.
    fn bitmap_size(bytes: &[u8]) -> Option<(u32, u32)> {
        let header = bytes.strip_prefix(b"\x89PNG\r\n\x1a\n")?;
        let dimension =
            |range: std::ops::Range<usize>| Some(u32::from_be_bytes(header.get(range)?.try_into().ok()?));
        Some((dimension(8..12)?, dimension(12..16)?))
    }
}
