//! The icons of the settings window. Each page has a color icon of the Fluent UI System Icons in
//! the navigation, as in the Settings app. High contrast shows the symbol of the page in the text
//! color instead, because a color icon does not follow the colors of the theme.

use ghosttype_core::settings_window::view::PageIcon;
use windows_reactor::{EncodedImage, Icon, Symbol};

use crate::system_theme::Contrast;

const ROBOT_GLYPH: &str = "\u{E99A}";

/// The icon of a page with `icon` in the navigation, while the contrast is `contrast`.
pub(super) fn page_icon(icon: PageIcon, contrast: Contrast) -> Icon {
    match contrast {
        Contrast::Standard => Icon::image_svg(EncodedImage::from_static(color_page_icon(icon))),
        Contrast::High => symbol_page_icon(icon),
    }
}

fn color_page_icon(icon: PageIcon) -> &'static [u8] {
    match icon {
        PageIcon::Gear => include_bytes!("../../assets/fluent/settings.svg"),
        PageIcon::Cube => include_bytes!("../../assets/fluent/bot.svg"),
        PageIcon::TextCursor => include_bytes!("../../assets/fluent/edit.svg"),
        PageIcon::TextViewfinder => include_bytes!("../../assets/fluent/scan_type.svg"),
        PageIcon::PersonText => include_bytes!("../../assets/fluent/contact_card.svg"),
    }
}

fn symbol_page_icon(icon: PageIcon) -> Icon {
    match icon {
        PageIcon::Gear => Symbol::Setting.into(),
        PageIcon::Cube => Icon::font(ROBOT_GLYPH),
        PageIcon::TextCursor => Symbol::Edit.into(),
        PageIcon::TextViewfinder => Symbol::Scan.into(),
        PageIcon::PersonText => Symbol::ContactInfo.into(),
    }
}
