//! Tests how a session continues on the new line after Enter.

use std::time::Duration;

use crate::desktop::MAX_CHARS_BEFORE_CARET;
use crate::engine::session::TRIMMED_CHARS_BEFORE_CARET;
use crate::engine::{Engine, Reaction};
use crate::key::Key;
use crate::settings::{ClipboardAccess, Settings};
use crate::test_support::{MAIL_APP, completion};

use super::support::{
    FakeDesktop, engine_showing, engine_waiting_for, engine_with, press, request, request_with,
};

/// A field with `before_caret` and a recent copy, which a new session puts into its preface.
fn field_with_copy(before_caret: &str) -> FakeDesktop {
    FakeDesktop::with_field_text(before_caret).with_copy("Sara Novak", Some(Duration::from_secs(1)))
}

fn engine_after_enter(typed: &str) -> Engine {
    engine_after_enter_in(typed, typed)
}

fn engine_after_enter_in(typed: &str, field_at_enter: &str) -> Engine {
    let mut engine = engine_with(&Settings {
        clipboard_access: ClipboardAccess::Allowed,
        ..Settings::default()
    });
    engine.prepare(&FakeDesktop::with_field_text(typed));
    engine.on_completion(completion(""));
    engine.on_key(Key::Enter, &FakeDesktop::with_field_text(field_at_enter));
    engine
}

#[test]
fn new_line_continues_the_field_text_when_the_app_changed_the_typed_text() {
    let mut engine = engine_after_enter_in("¿Que tal?", "¿Qué tal?");

    let reaction = engine.suggest_on_new_line(&field_with_copy("¿Qué tal?\n"));

    assert_eq!(reaction, Some(request("¿Qué tal?\n")));
}

#[test]
fn new_line_continues_the_typed_text_when_the_field_read_at_enter_is_old() {
    let mut engine = engine_after_enter_in("Best regards,", "Best regards");

    let reaction = engine.suggest_on_new_line(&field_with_copy("Best regards,\n"));

    assert_eq!(reaction, Some(request("Best regards,\n")));
}

#[test]
fn new_line_continues_when_the_field_shows_a_typed_space_as_a_no_break_space() {
    let mut engine = engine_after_enter_in("Thanks ", "Thanks\u{a0}");

    let reaction = engine.suggest_on_new_line(&field_with_copy("Thanks\u{a0}\n"));

    assert_eq!(reaction, Some(request("Thanks \n")));
}

#[test]
fn dead_key_keeps_the_session_for_the_accented_letter() {
    let mut engine = Engine::default();
    engine.on_key(Key::Char('u'), &FakeDesktop::with_field_text("¿Q"));
    engine.on_key(Key::Dead, &FakeDesktop::with_field_text("¿Qu"));

    let reaction = engine.on_key(Key::Char('e'), &FakeDesktop::with_field_text("¿Qu´"));

    assert_eq!(reaction, request("¿Que"));
}

#[test]
fn dead_key_hides_the_suggestion() {
    let mut engine = engine_showing("qu", "é tal");

    let reaction = press(&mut engine, Key::Dead);

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn tab_after_a_dead_key_passes_through() {
    let mut engine = engine_showing("qu", "é tal");
    press(&mut engine, Key::Dead);

    let reaction = press(&mut engine, Key::Tab);

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn dead_key_while_a_suggestion_shows_keeps_the_session() {
    let mut engine = engine_showing("qu", "é tal");

    press(&mut engine, Key::Dead);

    assert!(engine.has_session());
}

#[test]
fn enter_hides_the_suggestion() {
    let mut engine = engine_showing("he", "llo");

    let reaction = press(&mut engine, Key::Enter);

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn tab_after_enter_passes_through() {
    let mut engine = engine_showing("he", "llo");
    press(&mut engine, Key::Enter);

    let reaction = press(&mut engine, Key::Tab);

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn completion_of_a_request_before_enter_shows_nothing() {
    let mut engine = Engine::default();
    let desktop = FakeDesktop::with_field_text("Best regards");
    engine.on_key(Key::Char(','), &desktop);
    engine.on_key(Key::Enter, &desktop);

    let reaction = engine.on_completion(completion(" Sara"));

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn settled_input_after_enter_requests_the_new_line_with_the_same_preface() {
    let mut engine = engine_after_enter("Best regards,");

    let reaction = engine.suggest_on_new_line(&field_with_copy("Best regards,\n"));

    assert_eq!(reaction, Some(request("Best regards,\n")));
}

#[test]
fn key_after_enter_continues_the_session_on_the_new_line() {
    let mut engine = engine_after_enter("Best regards,");

    let reaction = engine.on_key(Key::Char('S'), &field_with_copy("Best regards,\n"));

    assert_eq!(reaction, request("Best regards,\nS"));
}

#[test]
fn new_line_keeps_what_the_app_adds_after_the_line_break() {
    let cases = [
        "- milk\n- ",
        "- milk\n\n",
        "- milk\r\n",
        "- milk\n    ",
        "- milk\n2. ",
    ];

    let reactions =
        cases.map(|field| engine_after_enter("- milk").suggest_on_new_line(&field_with_copy(field)));

    assert_eq!(reactions, cases.map(|field| Some(request(field))));
}

#[test]
fn second_enter_waits_for_the_same_read() {
    let mut engine = engine_after_enter("Hi Tom,");
    press(&mut engine, Key::Enter);

    let reaction = engine.suggest_on_new_line(&field_with_copy("Hi Tom,\n\n"));

    assert_eq!(reaction, Some(request("Hi Tom,\n\n")));
}

#[test]
fn session_ends_when_enter_starts_no_new_line() {
    let cases = [
        ("See you at 8", ""),
        ("weather in lisbon", "weather in lisbon"),
        ("明日の会議はとうきょう", "明日の会議は東京"),
        ("$ ls", "$ ls\nnotes.txt\n$ "),
        ("Dear Ana,", "Dear Ana,\nSara"),
        ("Thanks", "Thanks, "),
    ];

    let outcomes = cases.map(|(typed, field)| {
        let mut engine = engine_after_enter(typed);
        let reaction = engine.suggest_on_new_line(&field_with_copy(field));
        (reaction, engine.has_session())
    });

    assert_eq!(outcomes, cases.map(|_| (None, false)));
}

#[test]
fn key_after_enter_without_a_new_line_starts_a_new_session() {
    let mut engine = engine_after_enter("See you at 8");

    let reaction = engine.on_key(Key::Char('O'), &field_with_copy(""));

    assert_eq!(
        reaction,
        request_with(&["Text on my clipboard: “Sara Novak”"], "O")
    );
}

#[test]
fn settled_input_without_enter_suggests_nothing() {
    let mut engine = engine_waiting_for("he");

    let reaction = engine.suggest_on_new_line(&FakeDesktop::with_field_text("he\n"));

    assert_eq!(reaction, None);
}

#[test]
fn enter_ends_a_session_whose_field_the_platform_cannot_read() {
    let mut engine = engine_waiting_for("he");

    press(&mut engine, Key::Enter);

    assert!(!engine.has_session());
}

#[test]
fn enter_ends_a_session_in_a_subject_field() {
    let mut engine = Engine::default();
    let desktop = FakeDesktop::with_field_text("Lunch")
        .in_app(MAIL_APP)
        .with_field_name("Subject");
    engine.on_key(Key::Char('?'), &desktop);

    engine.on_key(Key::Enter, &desktop);

    assert!(!engine.has_session());
}

#[test]
fn new_line_after_a_trimmed_text_continues_its_end() {
    let mut engine = Engine::default();
    let desktop = FakeDesktop::with_field_text(&format!("x{}", "y".repeat(MAX_CHARS_BEFORE_CARET - 1)));
    engine.on_key(Key::Char('y'), &desktop);
    engine.on_completion(completion(""));
    engine.on_key(Key::Enter, &desktop);
    let field = format!("{}\n", "y".repeat(MAX_CHARS_BEFORE_CARET - 1));

    let reaction = engine.suggest_on_new_line(&FakeDesktop::with_field_text(&field));

    assert_eq!(
        reaction,
        Some(request(&format!(
            "{}\n",
            "y".repeat(TRIMMED_CHARS_BEFORE_CARET + 1)
        )))
    );
}
