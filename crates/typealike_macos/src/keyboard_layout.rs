//! Reads the keyboard layout of the user, which gives the text of a key press from its key code.

use std::ffi::c_void;
use std::ptr::NonNull;

use objc2_core_foundation::{CFData, CFRetained, CFString, CFType};
use objc2_core_graphics::{CGEventFlags, CGKeyCode};

const KEY_DOWN_ACTION: u16 = 0;
const NO_TRANSLATE_OPTIONS: u32 = 0;
const NO_ERROR: i32 = 0;
const MAX_TRANSLATED_UNITS: usize = 8;

/// The bits of the modifier state of `UCKeyTranslate`: the Carbon event modifiers shifted right
/// by 8 bits.
const MODIFIER_KEY_BITS: [(CGEventFlags, u32); 5] = [
    (CGEventFlags::MaskCommand, 0x01),
    (CGEventFlags::MaskShift, 0x02),
    (CGEventFlags::MaskAlphaShift, 0x04),
    (CGEventFlags::MaskAlternate, 0x08),
    (CGEventFlags::MaskControl, 0x10),
];

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    static kTISPropertyUnicodeKeyLayoutData: &'static CFString;
    #[cfg(test)]
    static kTISPropertyInputSourceID: &'static CFString;
    fn TISCopyCurrentKeyboardLayoutInputSource() -> Option<NonNull<CFType>>;
    #[cfg(test)]
    fn TISCreateInputSourceList(
        properties: &objc2_core_foundation::CFDictionary,
        include_all_installed: u8,
    ) -> Option<NonNull<objc2_core_foundation::CFArray>>;
    fn TISGetInputSourceProperty(source: &CFType, key: &CFString) -> Option<NonNull<c_void>>;
    fn LMGetKbdType() -> u8;
}

#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    fn UCKeyTranslate(
        layout: *const c_void,
        key: u16,
        key_action: u16,
        modifier_key_state: u32,
        keyboard_type: u32,
        options: u32,
        dead_key_state: *mut u32,
        max_length: usize,
        written: *mut usize,
        units: *mut u16,
    ) -> i32;
}

/// The dead key that waits for the next key press, as the keyboard layout tracks it.
#[derive(Debug, Default)]
pub(crate) struct DeadKeyState(u32);

impl DeadKeyState {
    /// Whether a dead key waits for the next key press.
    pub(crate) fn is_waiting(&self) -> bool {
        self.0 != 0
    }
}

/// The Unicode key data of a keyboard layout.
#[derive(Debug)]
pub(crate) struct KeyboardLayout {
    data: CFRetained<CFData>,
}

impl KeyboardLayout {
    /// Gives the keyboard layout that the user types with now.
    pub(crate) fn current() -> Option<Self> {
        // SAFETY: the call has no precondition. It gives a source that the caller owns, or null.
        let source = unsafe { TISCopyCurrentKeyboardLayoutInputSource() }?;
        // SAFETY: the copy gave the caller one reference to an input source.
        let source = unsafe { CFRetained::from_raw(source) };
        Self::of_source(&source)
    }

    /// Gives the installed keyboard layout with the input source `id`.
    #[cfg(test)]
    fn with_id(id: &str) -> Option<Self> {
        use objc2_core_foundation::{CFArray, CFDictionary};

        // SAFETY: the key is a constant string of HIToolbox.
        let id_key = unsafe { kTISPropertyInputSourceID };
        let filter = CFDictionary::from_slices(&[id_key], &[&*CFString::from_str(id)]);
        // SAFETY: the filter is a dictionary of input source properties. The call gives a list that
        // the caller owns, or null.
        let sources = unsafe { TISCreateInputSourceList(filter.as_ref(), u8::from(true)) }?;
        // SAFETY: the list holds input sources, and the caller owns this one reference to it.
        let sources: CFRetained<CFArray<CFType>> = unsafe { CFRetained::from_raw(sources.cast()) };
        let source = sources.get(0)?;
        Self::of_source(&source)
    }

    fn of_source(source: &CFType) -> Option<Self> {
        // SAFETY: `source` is an input source, and the key is a constant string of HIToolbox. The
        // property belongs to `source`, which lives during the retain below.
        let data = unsafe { TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData) }?;
        // SAFETY: the Unicode key layout of an input source is an immutable `CFData`.
        let data = unsafe { CFRetained::retain(data.cast::<CFData>()) };
        Some(Self { data })
    }

    /// Returns the UTF-16 text that a press of `key` types with the modifiers of `flags`. After a
    /// dead key in `dead_key`, the text composes both keys. `None` when the layout cannot translate
    /// the key.
    pub(crate) fn translate(
        &self,
        key: CGKeyCode,
        flags: CGEventFlags,
        dead_key: &mut DeadKeyState,
    ) -> Option<Vec<u16>> {
        let mut units = [0_u16; MAX_TRANSLATED_UNITS];
        let mut written = 0_usize;
        // SAFETY: the call reads a global of the input system and has no precondition.
        let keyboard_type = unsafe { LMGetKbdType() };
        // SAFETY: the layout pointer refers to the key data of `self.data`, which is immutable and
        // lives during the call. The other pointers refer to locals, and the buffer holds the
        // maximum length passed.
        let status = unsafe {
            UCKeyTranslate(
                self.data.byte_ptr().cast(),
                key,
                KEY_DOWN_ACTION,
                modifier_key_state(flags),
                u32::from(keyboard_type),
                NO_TRANSLATE_OPTIONS,
                &raw mut dead_key.0,
                units.len(),
                &raw mut written,
                units.as_mut_ptr(),
            )
        };
        if status != NO_ERROR {
            *dead_key = DeadKeyState::default();
            return None;
        }
        let typed = units.get(..written)?.to_vec();
        if !typed.is_empty() {
            // UCKeyTranslate keeps the last dead key in the high bits after the composed text.
            *dead_key = DeadKeyState::default();
        }
        Some(typed)
    }
}

/// Gives the modifier state of `UCKeyTranslate` for the modifiers of `flags`.
fn modifier_key_state(flags: CGEventFlags) -> u32 {
    MODIFIER_KEY_BITS
        .iter()
        .filter(|(flag, _)| flags.contains(*flag))
        .fold(0, |state, (_, bit)| state | bit)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// The input source functions trap when two threads call them at once, and the tests run in
    /// parallel.
    static INPUT_SOURCES: Mutex<()> = Mutex::new(());

    const ACUTE_DEAD_KEY_CODE: CGKeyCode = 33;
    const O_KEY_CODE: CGKeyCode = 31;
    const N_KEY_CODE: CGKeyCode = 45;
    const SPACE_KEY_CODE: CGKeyCode = 49;

    const E_KEY_CODE: CGKeyCode = 14;

    fn layout_with_id(id: &str) -> KeyboardLayout {
        let _input_sources = INPUT_SOURCES
            .lock()
            .expect("no test panics while it reads the input sources");
        KeyboardLayout::with_id(id).expect("macOS has the layout")
    }

    fn latin_american() -> KeyboardLayout {
        layout_with_id("com.apple.keylayout.LatinAmerican")
    }

    fn text_after_dead_key(key: CGKeyCode) -> String {
        let layout = latin_american();
        let mut dead_key = DeadKeyState::default();
        layout.translate(ACUTE_DEAD_KEY_CODE, CGEventFlags::empty(), &mut dead_key);
        let units = layout
            .translate(key, CGEventFlags::empty(), &mut dead_key)
            .expect("the layout translates the key");
        String::from_utf16_lossy(&units)
    }

    #[test]
    fn dead_key_types_nothing_and_waits() {
        let mut dead_key = DeadKeyState::default();

        let units = latin_american().translate(ACUTE_DEAD_KEY_CODE, CGEventFlags::empty(), &mut dead_key);

        assert_eq!((units, dead_key.is_waiting()), (Some(Vec::new()), true));
    }

    #[test]
    fn key_after_a_dead_key_types_what_the_layout_composes() {
        let keys = [O_KEY_CODE, SPACE_KEY_CODE, N_KEY_CODE];

        let texts = keys.map(text_after_dead_key);

        assert_eq!(texts, ["ó", "´", "´n"]);
    }

    #[test]
    fn key_after_a_dead_key_ends_the_wait() {
        let layout = latin_american();
        let mut dead_key = DeadKeyState::default();
        layout.translate(ACUTE_DEAD_KEY_CODE, CGEventFlags::empty(), &mut dead_key);

        layout.translate(O_KEY_CODE, CGEventFlags::empty(), &mut dead_key);

        assert!(!dead_key.is_waiting());
    }

    #[test]
    fn option_dead_key_composes_with_the_next_key() {
        let layout = layout_with_id("com.apple.keylayout.US");
        let mut dead_key = DeadKeyState::default();
        layout.translate(E_KEY_CODE, CGEventFlags::MaskAlternate, &mut dead_key);

        let units = layout.translate(E_KEY_CODE, CGEventFlags::empty(), &mut dead_key);

        assert_eq!(units, Some("é".encode_utf16().collect()));
    }

    #[test]
    fn shift_types_the_capital_letter() {
        let mut dead_key = DeadKeyState::default();

        let units = latin_american().translate(O_KEY_CODE, CGEventFlags::MaskShift, &mut dead_key);

        assert_eq!(units, Some("O".encode_utf16().collect()));
    }
}
