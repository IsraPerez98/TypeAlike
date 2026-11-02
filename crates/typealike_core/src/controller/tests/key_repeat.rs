//! Tests the keys that the OS repeats while the user holds a key.

use super::harness::Harness;
use crate::key::Key;
use crate::settings::Settings;
use crate::test_support::suggestion_request;

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
