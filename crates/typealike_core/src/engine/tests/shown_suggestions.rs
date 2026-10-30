//! Tests how a backspace or a retype shows a suggestion of the session again without a request.

use crate::engine::{AfterAccept, Engine, Reaction};
use crate::key::Key;
use crate::test_support::completion;

use super::support::{FakeDesktop, engine_showing, engine_waiting_for, press, request, show, type_text};

#[test]
fn backspace_after_a_wrong_letter_shows_the_earlier_suggestion_again() {
    let mut engine = engine_showing("hel", "lo world");
    press(&mut engine, Key::Char('p'));

    let reaction = press(&mut engine, Key::Backspace);

    assert_eq!(reaction, show("lo world"));
}

#[test]
fn backspace_onto_a_typed_part_of_a_suggestion_shows_its_rest() {
    let mut engine = engine_showing("hel", "lo world");
    type_text(&mut engine, "lp");

    let reaction = press(&mut engine, Key::Backspace);

    assert_eq!(reaction, show("o world"));
}

#[test]
fn backspace_shows_a_suggestion_that_showed_word_by_word_again() {
    let mut engine = engine_waiting_for("hel");
    engine.on_partial_completion("lo big wor");
    press(&mut engine, Key::Char('p'));

    let reaction = press(&mut engine, Key::Backspace);

    assert_eq!(reaction, show("lo big"));
}

#[test]
fn retyped_letter_after_a_backspace_follows_the_earlier_suggestion() {
    let mut engine = engine_showing("hel", "lo world");
    press(&mut engine, Key::Char('p'));
    press(&mut engine, Key::Backspace);

    let reaction = press(&mut engine, Key::Char('l'));

    assert_eq!(reaction, show("o world"));
}

#[test]
fn letter_after_a_dead_key_follows_the_earlier_suggestion() {
    let mut engine = engine_showing("qu", "é tal");
    press(&mut engine, Key::Dead);

    let reaction = press(&mut engine, Key::Char('é'));

    assert_eq!(reaction, show(" tal"));
}

#[test]
fn completion_of_the_request_before_the_backspace_keeps_the_earlier_suggestion() {
    let mut engine = engine_showing("hel", "lo world");
    press(&mut engine, Key::Char('p'));
    press(&mut engine, Key::Backspace);

    let reaction = engine.on_completion(completion("ful"));

    assert_eq!(reaction, show("lo world"));
}

#[test]
fn tab_on_the_last_part_of_an_earlier_suggestion_requests_the_next_suggestion() {
    let mut engine = engine_showing("hel", "lo");
    press(&mut engine, Key::Char('p'));
    press(&mut engine, Key::Backspace);

    let reaction = press(&mut engine, Key::Tab);

    assert_eq!(
        reaction,
        Reaction::Accept {
            text: "lo".to_owned(),
            then: AfterAccept::Next(Box::new(request("hello"))),
        }
    );
}

#[test]
fn backspace_after_enter_shows_the_suggestion_at_the_end_of_the_line_above_again() {
    let mut engine = Engine::default();
    engine.on_key(Key::Char(','), &FakeDesktop::with_field_text("Best regards"));
    engine.on_completion(completion(" Sara"));
    engine.on_key(Key::Enter, &FakeDesktop::with_field_text("Best regards,"));

    let reaction = engine.on_key(Key::Backspace, &FakeDesktop::with_field_text("Best regards,\n"));

    assert_eq!(reaction, show(" Sara"));
}

#[test]
fn new_line_after_enter_requests_its_own_suggestion() {
    let mut engine = Engine::default();
    engine.on_key(Key::Char(','), &FakeDesktop::with_field_text("Best regards"));
    engine.on_completion(completion(" Sara"));
    engine.on_key(Key::Enter, &FakeDesktop::with_field_text("Best regards,"));

    let reaction = engine.suggest_on_new_line(&FakeDesktop::with_field_text("Best regards,\n"));

    assert_eq!(reaction, Some(request("Best regards,\n")));
}

#[test]
fn letter_after_escape_does_not_follow_the_dismissed_suggestion() {
    let mut engine = engine_showing("hel", "lo world");
    press(&mut engine, Key::Escape);
    engine.end_field_pause();

    let reaction = press(&mut engine, Key::Char('l'));

    assert_eq!(reaction, request("hell"));
}

#[test]
fn backspace_after_escape_does_not_show_the_dismissed_suggestion() {
    let mut engine = engine_showing("hel", "lo world");
    press(&mut engine, Key::Escape);
    engine.end_field_pause();
    press(&mut engine, Key::Char('p'));

    let reaction = press(&mut engine, Key::Backspace);

    assert_eq!(reaction, request("hel"));
}

#[test]
fn backspace_after_escape_shows_an_earlier_suggestion_that_did_not_show_there() {
    let mut engine = engine_showing("he", "y there");
    press(&mut engine, Key::Char('l'));
    engine.on_completion(completion("lo world"));
    press(&mut engine, Key::Escape);
    engine.end_field_pause();

    let reaction = press(&mut engine, Key::Backspace);

    assert_eq!(reaction, show("y there"));
}

#[test]
fn new_session_does_not_show_the_suggestions_of_the_session_before() {
    let mut engine = engine_showing("hel", "lo world");
    engine.reset();

    let reaction = engine.on_key(Key::Char('l'), &FakeDesktop::with_field_text("hel"));

    assert_eq!(reaction, request("hell"));
}
