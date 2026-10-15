//! The text that the user copied, unless the app that copied it marked it as private.

use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};

/// The pasteboard types that password managers and other apps add to text that no other app may
/// read or keep, as nspasteboard.org defines them.
const PRIVATE_TYPES: [&str; 2] = ["org.nspasteboard.ConcealedType", "org.nspasteboard.TransientType"];

/// Reads the text on the general pasteboard.
pub(crate) fn copied_text() -> Option<String> {
    let pasteboard = NSPasteboard::generalPasteboard();
    let types: Vec<String> = pasteboard.types()?.iter().map(|kind| kind.to_string()).collect();
    if is_private(&types) {
        return None;
    }
    // SAFETY: reads an immutable system constant.
    let string_type = unsafe { NSPasteboardTypeString };
    pasteboard.stringForType(string_type).map(|text| text.to_string())
}

fn is_private(types: &[String]) -> bool {
    types.iter().any(|kind| PRIVATE_TYPES.contains(&kind.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concealed_or_transient_text_is_private() {
        let cases = [
            (vec!["public.utf8-plain-text"], false),
            (
                vec!["public.utf8-plain-text", "org.nspasteboard.ConcealedType"],
                true,
            ),
            (vec!["org.nspasteboard.TransientType"], true),
        ];

        let verdicts = cases
            .clone()
            .map(|(types, _)| is_private(&types.into_iter().map(str::to_owned).collect::<Vec<_>>()));

        assert_eq!(verdicts, cases.map(|(_, is_private)| is_private));
    }
}
