//! The text that the user typed in each field of a window, which the field shows in place of the
//! text of the view while the view is behind.

use std::collections::{HashMap, VecDeque};

use typealike_core::settings_window::view::{ControlId, Row, Section};

/// The text that the user typed in each field. A view from the controller can come after newer
/// keys, and its older text would then move the caret and remove the newer keys. Thus a field
/// shows the typed text until the controller shows the newest edit. The controller handles the
/// edits in order, so a view text that is no edit that waits, while no edit waits, is a change of
/// the controller, such as an example of the notes that the user picked. The field then shows it.
#[derive(Debug, Default)]
pub(super) struct TypedTexts {
    typed: HashMap<ControlId, String>,
    /// The edits of each field that went to the controller and that no view showed yet, oldest
    /// first.
    waiting: HashMap<ControlId, VecDeque<String>>,
}

impl TypedTexts {
    /// The text that the field `id` shows: the typed text, else `shown`, the text of the view.
    pub(super) fn text<'text>(&'text self, id: &ControlId, shown: &'text str) -> &'text str {
        self.typed.get(id).map_or(shown, String::as_str)
    }

    /// Keeps `text`, which the user typed in the field `id` and which goes to the controller only
    /// when the user submits it.
    pub(super) fn on_typed(&mut self, id: ControlId, text: String) {
        self.typed.insert(id, text);
    }

    /// Keeps `text`, which the user typed in the field `id` and which goes to the controller now.
    pub(super) fn on_edited(&mut self, id: ControlId, text: String) {
        self.waiting
            .entry(id.clone())
            .or_default()
            .push_back(text.clone());
        self.typed.insert(id, text);
    }

    /// Takes the text of the field `id` for a submit.
    pub(super) fn submitted(&self, id: &ControlId) -> String {
        self.typed.get(id).cloned().unwrap_or_default()
    }

    /// Takes the text of each field of `sections` in a new view.
    pub(super) fn on_view<'view>(&mut self, sections: impl IntoIterator<Item = &'view Section>) {
        let fields = sections
            .into_iter()
            .flat_map(|section| &section.rows)
            .filter_map(|row| match row {
                Row::TextArea(area) => Some((&area.id, &area.text)),
                Row::TextLine(line) => Some((&line.id, &line.text)),
                _ => None,
            });
        for (id, text) in fields {
            self.on_view_text(id, text);
        }
    }

    fn on_view_text(&mut self, id: &ControlId, text: &str) {
        let waiting = self.waiting.entry(id.clone()).or_default();
        if let Some(shown) = waiting.iter().position(|edit| edit == text) {
            waiting.drain(..=shown);
            return;
        }
        let is_change_of_the_controller =
            waiting.is_empty() && self.typed.get(id).is_some_and(|typed| typed != text);
        if is_change_of_the_controller {
            self.typed.remove(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use typealike_core::settings_window::view::{RowIcon, TextArea};

    const NOTES: ControlId = ControlId::new("notes");

    fn view_with(text: &str) -> Section {
        Section {
            heading: None,
            back: None,
            rows: vec![Row::TextArea(TextArea {
                id: NOTES,
                icon: RowIcon::Note,
                label: String::new(),
                description: String::new(),
                text: text.to_owned(),
                button: None,
            })],
            footer: None,
        }
    }

    fn shown_after(edits: &[&str], views: &[&str]) -> String {
        let mut typed = TypedTexts::default();
        for edit in edits {
            typed.on_edited(NOTES, (*edit).to_owned());
        }
        for view in views {
            typed.on_view([&view_with(view)]);
        }
        let last_view = views.last().copied().unwrap_or_default();
        typed.text(&NOTES, last_view).to_owned()
    }

    #[test]
    fn view_behind_the_keys_keeps_the_typed_text() {
        let shown = shown_after(&["a", "ab", "abc"], &["a"]);

        assert_eq!(shown, "abc");
    }

    #[test]
    fn view_of_the_newest_edit_shows_it() {
        let shown = shown_after(&["a", "ab"], &["a", "ab"]);

        assert_eq!(shown, "ab");
    }

    #[test]
    fn repeated_older_view_keeps_the_typed_text() {
        let shown = shown_after(&["a", "ab"], &["a", "a"]);

        assert_eq!(shown, "ab");
    }

    #[test]
    fn change_of_the_controller_after_the_edits_shows_its_text() {
        let shown = shown_after(&["a", "ab"], &["a", "ab", "I am a student."]);

        assert_eq!(shown, "I am a student.");
    }

    #[test]
    fn submit_takes_the_typed_text() {
        let mut typed = TypedTexts::default();

        typed.on_typed(NOTES, "qwen".to_owned());

        assert_eq!(typed.submitted(&NOTES), "qwen");
    }
}
