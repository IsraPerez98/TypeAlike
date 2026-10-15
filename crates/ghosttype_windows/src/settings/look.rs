//! What a control looks like, packed into the control's window data, so that drawing needs no
//! state but the window itself.

use crate::settings::controls::ControlKind;
use crate::settings::palette::{ControlStyle, Surface, Tone};
use crate::settings::typography::TextRole;

const KINDS: [ControlKind; 11] = [
    ControlKind::Label,
    ControlKind::TrailingLabel,
    ControlKind::Paragraph,
    ControlKind::Button,
    ControlKind::PrimaryButton,
    ControlKind::NavigationButton,
    ControlKind::Picker,
    ControlKind::Switch,
    ControlKind::TextField,
    ControlKind::TextArea,
    ControlKind::ProgressBar,
];
const ROLES: [TextRole; 6] = [
    TextRole::Title,
    TextRole::Body,
    TextRole::BodyStrong,
    TextRole::Caption,
    TextRole::Icon,
    TextRole::SmallIcon,
];
const SURFACES: [Surface; 2] = [Surface::Page, Surface::Card];
const TONES: [Tone; 3] = [Tone::Primary, Tone::Secondary, Tone::Critical];

const KIND_MASK: usize = 0xF;
const ROLE_SHIFT: u32 = 4;
const ROLE_MASK: usize = 0x7;
const SURFACE_SHIFT: u32 = 7;
const SURFACE_MASK: usize = 0x1;
const TONE_SHIFT: u32 = 8;
const TONE_MASK: usize = 0x3;
const HOVERED_BIT: usize = 1 << 10;
const SELECTED_BIT: usize = 1 << 11;
const GLYPH_SHIFT: u32 = 16;
const GLYPH_MASK: usize = 0xFFFF;

/// Everything that decides how a control draws, apart from its text and system state such as
/// keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ControlLook {
    pub(crate) kind: ControlKind,
    pub(crate) role: TextRole,
    pub(crate) style: ControlStyle,
    /// The icon before a navigation item's label, from Segoe Fluent Icons.
    pub(crate) glyph: Option<char>,
    /// Whether the pointer is over the control.
    pub(crate) is_hovered: bool,
    /// Whether a navigation item stands for the shown page, or a switch is on.
    pub(crate) is_selected: bool,
}

impl ControlLook {
    pub(crate) fn new(kind: ControlKind, role: TextRole, style: ControlStyle) -> Self {
        Self {
            kind,
            role,
            style,
            glyph: None,
            is_hovered: false,
            is_selected: false,
        }
    }

    /// Packs the look into a window data value. Glyphs outside the basic plane are dropped.
    pub(crate) fn to_data(self) -> usize {
        let glyph = self
            .glyph
            .and_then(|glyph| u16::try_from(u32::from(glyph)).ok())
            .map_or(0, usize::from);
        let flag = |is_set: bool, bit: usize| if is_set { bit } else { 0 };
        (index_of(&KINDS, self.kind) + 1)
            | index_of(&ROLES, self.role) << ROLE_SHIFT
            | index_of(&SURFACES, self.style.surface) << SURFACE_SHIFT
            | index_of(&TONES, self.style.tone) << TONE_SHIFT
            | flag(self.is_hovered, HOVERED_BIT)
            | flag(self.is_selected, SELECTED_BIT)
            | glyph << GLYPH_SHIFT
    }

    /// Reads a look packed by [`Self::to_data`]; `None` for windows that hold no look.
    pub(crate) fn from_data(data: usize) -> Option<Self> {
        let kind = *KINDS.get((data & KIND_MASK).checked_sub(1)?)?;
        let glyph = u32::try_from((data >> GLYPH_SHIFT) & GLYPH_MASK).ok();
        Some(Self {
            kind,
            role: *ROLES.get((data >> ROLE_SHIFT) & ROLE_MASK)?,
            style: ControlStyle {
                surface: *SURFACES.get((data >> SURFACE_SHIFT) & SURFACE_MASK)?,
                tone: *TONES.get((data >> TONE_SHIFT) & TONE_MASK)?,
            },
            glyph: glyph.filter(|code| *code != 0).and_then(char::from_u32),
            is_hovered: data & HOVERED_BIT != 0,
            is_selected: data & SELECTED_BIT != 0,
        })
    }
}

fn index_of<Item: PartialEq + Copy>(items: &[Item], item: Item) -> usize {
    items
        .iter()
        .position(|listed| *listed == item)
        .expect("every variant is listed")
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARD_DETAIL: ControlStyle = ControlStyle {
        surface: Surface::Card,
        tone: Tone::Critical,
    };

    #[test]
    fn look_round_trips_through_window_data() {
        let looks = [
            ControlLook::new(ControlKind::Label, TextRole::Caption, CARD_DETAIL),
            ControlLook {
                glyph: Some('\u{E896}'),
                is_selected: true,
                ..ControlLook::new(
                    ControlKind::NavigationButton,
                    TextRole::Body,
                    ControlStyle {
                        surface: Surface::Page,
                        tone: Tone::Primary,
                    },
                )
            },
            ControlLook {
                is_hovered: true,
                ..ControlLook::new(ControlKind::ProgressBar, TextRole::SmallIcon, CARD_DETAIL)
            },
        ];

        let read = looks.map(|look| ControlLook::from_data(look.to_data()));

        assert_eq!(read, looks.map(Some));
    }

    #[test]
    fn empty_window_data_holds_no_look() {
        assert_eq!(ControlLook::from_data(0), None);
    }

    #[test]
    fn glyph_outside_basic_plane_is_dropped() {
        let look = ControlLook {
            glyph: Some('\u{1F600}'),
            ..ControlLook::new(ControlKind::Button, TextRole::Body, CARD_DETAIL)
        };

        assert_eq!(
            ControlLook::from_data(look.to_data()).map(|read| read.glyph),
            Some(None)
        );
    }
}
