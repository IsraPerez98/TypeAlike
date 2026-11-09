//! Tests the keys that the OS repeats while the user holds a key.

use super::harness::{Harness, field_around};
use crate::key::Key;
use crate::settings::Settings;
use crate::test_support::suggestion_request;

#[test]
fn held_enter_asks_for_no_suggestion_when_the_caret_settles() {
    let mut harness = Harness::new(Settings::default());
    harness.shell.surroundings = field_around("Best regards", "");
    harness.press(Key::Char(','));
    harness.press(Key::Enter);
    harness.repeat(Key::Enter);
    harness.shell.surroundings = field_around("Best regards,\n\n", "");
    let requests_after_enter = harness.requests();

    harness.settle_caret();

    assert_eq!(harness.requests(), requests_after_enter);
}

#[test]
fn repeated_keys_ask_for_no_suggestion() {
    let mut harness = Harness::new(Settings::default());
    harness.type_text("Hello");
    let requests_after_typing = harness.requests();

    harness.repeat(Key::Backspace);
    harness.repeat(Key::Backspace);

    assert_eq!(harness.requests(), requests_after_typing);
}

#[test]
fn held_key_asks_for_a_suggestion_when_the_input_settles() {
    let mut harness = Harness::new(Settings::default());
    harness.type_text("Hello");
    harness.repeat(Key::Backspace);
    harness.repeat(Key::Backspace);

    harness.settle_input();

    assert_eq!(harness.last_request(), Some(suggestion_request(&[], "Hel")));
}
