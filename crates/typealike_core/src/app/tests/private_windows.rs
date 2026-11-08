//! Tests of the private windows of a browser, where suggestions never show.

use super::fixtures::app_preparing_on_focus;
use crate::app::App;
use crate::desktop::{CopiedText, Desktop, Field, FieldText, Surroundings};
use crate::engine::Reaction;
use crate::key::{Key, Press};
use crate::settings::{PrivateWindowRule, Settings, SuggestionPlaces};
use crate::test_support::{english, off_badge};

/// A text field of a page in a window of Chrome with the title `title`.
struct ChromeWindow {
    title: &'static str,
}

const PRIVATE_WINDOW: ChromeWindow = ChromeWindow {
    title: "Trip notes - Google Chrome (Incognito)",
};

const NORMAL_WINDOW: ChromeWindow = ChromeWindow {
    title: "Trip notes - Google Chrome",
};

impl Desktop for ChromeWindow {
    fn field(&self) -> Field {
        Field::Open
    }

    fn surroundings(&self) -> Surroundings {
        Surroundings {
            app_id: Some("com.google.Chrome".to_owned()),
            window_title: Some(self.title.to_owned()),
            domain: Some("notes.example".to_owned()),
            field_text: Some(FieldText::default()),
            ..Surroundings::default()
        }
    }

    fn copied_text(&self) -> Option<CopiedText> {
        None
    }
}

#[test]
fn key_in_a_private_window_asks_for_no_suggestion() {
    let mut app = App::new();

    let reaction = app.on_key(Key::Char('H'), Press::First, &PRIVATE_WINDOW);

    assert_eq!((reaction, app.has_offer()), (Reaction::Hide, false));
}

#[test]
fn key_in_a_normal_window_of_the_browser_asks_for_a_suggestion() {
    let mut app = App::new();

    let reaction = app.on_key(Key::Char('H'), Press::First, &NORMAL_WINDOW);

    assert!(matches!(reaction, Reaction::Request(_)));
}

#[test]
fn typing_in_a_private_window_shows_the_off_badge() {
    let mut app = App::new();

    app.on_key(Key::Char('H'), Press::First, &PRIVATE_WINDOW);

    assert_eq!(app.badge_view(&english()), Some(off_badge()));
}

#[test]
fn settled_input_in_a_private_window_reads_no_screen_and_prepares_nothing() {
    let mut app = app_preparing_on_focus();

    let work = app.on_input_settled(&PRIVATE_WINDOW);

    assert_eq!(work, None);
}

#[test]
fn key_in_a_private_window_asks_for_a_suggestion_when_the_rule_keeps_suggestions() {
    let mut app = App::new();
    app.use_settings(&Settings {
        suggestion_places: SuggestionPlaces {
            private_windows: PrivateWindowRule::KeepSuggestions,
            ..SuggestionPlaces::default()
        },
        ..Settings::default()
    });

    let reaction = app.on_key(Key::Char('H'), Press::First, &PRIVATE_WINDOW);

    assert!(matches!(reaction, Reaction::Request(_)));
}
