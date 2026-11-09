//! Tests the keys that the OS repeats while the user holds a key: they change the text, and the
//! suggestion waits until the hold ends.

use crate::engine::{Engine, Reaction};
use crate::key::{Key, Press};

use super::support::{FakeDesktop, engine_showing, press, request, show};

fn repeat(engine: &mut Engine, key: Key) -> Reaction {
    engine.on_key(key, Press::Repeat, &FakeDesktop::default())
}

#[test]
fn repeated_backspace_hides_the_ghost_text_without_a_request() {
    let mut engine = engine_showing("Hello", " there");

    let reaction = repeat(&mut engine, Key::Backspace);

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn repeated_letter_hides_the_ghost_text_without_a_request() {
    let mut engine = engine_showing("No", " way");

    let reaction = repeat(&mut engine, Key::Char('o'));

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn repeated_letter_that_the_ghost_text_shows_takes_it() {
    let mut engine = engine_showing("hel", "lo world");

    let reaction = repeat(&mut engine, Key::Char('l'));

    assert_eq!(reaction, show("o world"));
}

#[test]
fn end_of_a_hold_asks_for_a_suggestion_at_the_new_text() {
    let mut engine = engine_showing("Hello", " there");
    repeat(&mut engine, Key::Backspace);
    repeat(&mut engine, Key::Backspace);

    let reaction = engine.suggest_when_due();

    assert_eq!(reaction, Some(request("Hel")));
}

#[test]
fn end_of_a_hold_shows_the_earlier_suggestion_at_the_same_text_without_a_request() {
    let mut engine = engine_showing("hel", "lo world");
    press(&mut engine, Key::Char('p'));
    repeat(&mut engine, Key::Backspace);

    let reaction = engine.suggest_when_due();

    assert_eq!(reaction, Some(show("lo world")));
}

#[test]
fn end_of_a_hold_asks_once() {
    let mut engine = engine_showing("Hello", " there");
    repeat(&mut engine, Key::Backspace);
    engine.suggest_when_due();

    let reaction = engine.suggest_when_due();

    assert_eq!(reaction, None);
}

#[test]
fn input_without_a_hold_asks_for_nothing_after_it() {
    let mut engine = engine_showing("Hello", " there");

    let reaction = engine.suggest_when_due();

    assert_eq!(reaction, None);
}

#[test]
fn press_after_a_hold_asks_in_place_of_the_end_of_the_hold() {
    let mut engine = engine_showing("Hello", " there");
    repeat(&mut engine, Key::Backspace);
    press(&mut engine, Key::Char('p'));

    let reaction = engine.suggest_when_due();

    assert_eq!(reaction, None);
}

#[test]
fn end_of_the_session_drops_the_suggestion_after_the_hold() {
    let mut engine = engine_showing("Hello", " there");
    repeat(&mut engine, Key::Backspace);
    press(&mut engine, Key::Other);

    let reaction = engine.suggest_when_due();

    assert_eq!(reaction, None);
}

#[test]
fn discarded_session_drops_the_suggestion_after_the_hold() {
    let mut engine = engine_showing("Hello", " there");
    repeat(&mut engine, Key::Backspace);
    engine.discard_session();

    let reaction = engine.suggest_when_due();

    assert_eq!(reaction, None);
}
