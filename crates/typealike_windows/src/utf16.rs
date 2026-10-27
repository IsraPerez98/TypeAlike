//! The UTF-16 text that Win32 gives.

/// Decodes UTF-16 `units`, without the halves of the characters that a cut divided.
pub(crate) fn whole_chars(units: &[u16]) -> String {
    char::decode_utf16(units.iter().copied())
        .filter_map(Result::ok)
        .collect()
}

/// Returns the units of `units` before its first nul.
pub(crate) fn until_nul(units: &[u16]) -> &[u16] {
    units.split(|&unit| unit == 0).next().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_ends_at_the_first_nul() {
        let units = [0x48, 0x69, 0, 0x78];

        assert_eq!(until_nul(&units), [0x48, 0x69]);
    }

    #[test]
    fn text_without_nul_is_whole() {
        let units = [0x48, 0x69];

        assert_eq!(until_nul(&units), [0x48, 0x69]);
    }
}
