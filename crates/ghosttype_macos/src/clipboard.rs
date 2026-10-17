//! The text that the user copied, unless the app that copied it marked it as private.

use std::time::Instant;

use ghosttype_core::desktop::CopiedText;
use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};

/// The pasteboard types that password managers and other apps add to text that no other app may
/// read or keep, as nspasteboard.org defines them.
const PRIVATE_TYPES: [&str; 2] = ["org.nspasteboard.ConcealedType", "org.nspasteboard.TransientType"];

/// Notes when the user copies. macOS sends no event for a copy, so the time of a copy is the time
/// of the first key or click after it.
#[derive(Debug)]
pub(crate) struct ClipboardWatch {
    change_count: isize,
    /// When the last copy came, or `None` while the pasteboard holds a copy from before the start.
    copied_at: Option<Instant>,
}

impl ClipboardWatch {
    pub(crate) fn start() -> Self {
        Self {
            change_count: NSPasteboard::generalPasteboard().changeCount(),
            copied_at: None,
        }
    }

    /// Notes the time of a copy that came since the last call. Call it on each key and click.
    pub(crate) fn on_input(&mut self) {
        let change_count = NSPasteboard::generalPasteboard().changeCount();
        if change_count != self.change_count {
            self.change_count = change_count;
            self.copied_at = Some(Instant::now());
        }
    }

    pub(crate) fn copied_text(&self) -> Option<CopiedText> {
        Some(CopiedText {
            text: read_text()?,
            age: self.copied_at.map(|copied_at| copied_at.elapsed()),
        })
    }
}

fn read_text() -> Option<String> {
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
