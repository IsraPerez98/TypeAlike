//! Tests the start and the end of a session, its text and its preface.

use std::time::Duration;

use crate::desktop::{MAX_CHARS_BEFORE_CARET, Surroundings};
use crate::engine::preface::MAX_CLIPBOARD_AGE;
use crate::engine::session::TRIMMED_CHARS_BEFORE_CARET;
use crate::engine::{Engine, Reaction};
use crate::key::Key;
use crate::prompt;
use crate::settings::{ClipboardAccess, Settings, WriterName, WriterNotes};
use crate::test_support::{ExpectedRequest, MAIL_APP, completion};

use super::support::{
    FakeDesktop, engine_showing, engine_waiting_for, engine_with, press, request, request_with,
};

#[test]
fn caret_is_at_the_line_end_while_its_line_has_no_text_after_it() {
    let texts_after = ["", "  \nSee you", " on Friday."];

    let line_ends = texts_after.map(|text_after| {
        let mut engine = Engine::default();
        engine.on_key(
            Key::Char(' '),
            &FakeDesktop::with_text_around("Let's meet", text_after),
        );
        engine.is_caret_at_line_end()
    });

    assert_eq!(line_ends, [true, true, false]);
}

#[test]
fn caret_in_a_field_that_hides_its_line_is_not_at_the_line_end() {
    let desktop = FakeDesktop {
        surroundings: Surroundings {
            is_caret_line_hidden: true,
            ..FakeDesktop::with_field_text("Let's meet").surroundings
        },
        ..FakeDesktop::default()
    };
    let mut engine = Engine::default();

    engine.on_key(Key::Char(' '), &desktop);

    assert!(!engine.is_caret_at_line_end());
}

#[test]
fn reset_hides_the_suggestion() {
    let mut engine = engine_showing("he", "llo");

    let reaction = engine.reset();

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn typing_after_reset_starts_an_empty_context() {
    let mut engine = engine_showing("he", "llo");
    engine.reset();

    let reaction = press(&mut engine, Key::Char('l'));

    assert_eq!(reaction, request("l"));
}

#[test]
fn completion_after_reset_is_dropped() {
    let mut engine = engine_waiting_for("hel");
    engine.reset();

    let reaction = engine.on_completion(completion("lo"));

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn typing_after_reset_while_waiting_requests_the_new_context() {
    let mut engine = engine_waiting_for("hel");
    engine.reset();

    let reaction = press(&mut engine, Key::Char('x'));

    assert_eq!(reaction, request("x"));
}

#[test]
fn completion_from_before_a_reset_is_dropped_after_a_new_request() {
    let mut engine = engine_waiting_for("hel");
    engine.reset();
    press(&mut engine, Key::Char('x'));

    let reaction = engine.on_completion(completion("lo"));

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn context_that_reaches_the_limit_drops_old_characters_in_one_step() {
    let mut engine = engine_waiting_for(&format!("x{}", "y".repeat(MAX_CHARS_BEFORE_CARET - 2)));
    engine.on_completion(completion(""));

    let reaction = press(&mut engine, Key::Char('y'));

    assert_eq!(reaction, request(&"y".repeat(TRIMMED_CHARS_BEFORE_CARET)));
}

#[test]
fn context_below_the_limit_keeps_its_start() {
    let mut engine = engine_waiting_for(&format!("x{}", "y".repeat(MAX_CHARS_BEFORE_CARET - 3)));
    engine.on_completion(completion(""));

    let reaction = press(&mut engine, Key::Char('y'));

    assert_eq!(
        reaction,
        request(&format!("x{}", "y".repeat(MAX_CHARS_BEFORE_CARET - 2)))
    );
}

#[test]
fn first_key_continues_the_field_text_before_the_caret() {
    let mut engine = Engine::default();

    let reaction = engine.on_key(Key::Char('l'), &FakeDesktop::with_field_text("Hello wor"));

    assert_eq!(reaction, request("Hello worl"));
}

#[test]
fn field_text_is_read_once_per_session() {
    let mut engine = Engine::default();
    let desktop = FakeDesktop::with_field_text("Hello ");
    engine.on_key(Key::Char('w'), &desktop);
    engine.on_completion(completion(""));

    let reaction = engine.on_key(Key::Char('o'), &desktop);

    assert_eq!(
        (reaction, desktop.surroundings_reads.get()),
        (request("Hello wo"), 1)
    );
}

#[test]
fn reset_reads_the_field_again() {
    let mut engine = Engine::default();
    engine.on_key(Key::Char('x'), &FakeDesktop::with_field_text("Old "));
    engine.on_completion(completion(""));
    engine.reset();

    let reaction = engine.on_key(Key::Char('d'), &FakeDesktop::with_field_text("New wor"));

    assert_eq!(reaction, request("New word"));
}

#[test]
fn backspace_deletes_from_the_field_text() {
    let mut engine = Engine::default();

    let reaction = engine.on_key(Key::Backspace, &FakeDesktop::with_field_text("Helloo"));

    assert_eq!(reaction, request("Hello"));
}

#[test]
fn long_field_text_keeps_its_end() {
    let mut engine = Engine::default();
    let before_caret = format!("x{}", "y".repeat(MAX_CHARS_BEFORE_CARET - 1));

    let reaction = engine.on_key(Key::Char('y'), &FakeDesktop::with_field_text(&before_caret));

    assert_eq!(reaction, request(&"y".repeat(TRIMMED_CHARS_BEFORE_CARET + 1)));
}

#[test]
fn writer_notes_come_after_the_frame() {
    let mut engine = engine_with(&Settings {
        writer_notes: WriterNotes("I am a nurse.".to_owned()),
        ..Settings::default()
    });

    let reaction = press(&mut engine, Key::Char('h'));

    assert_eq!(
        reaction,
        Reaction::from(ExpectedRequest {
            prompt: prompt::prompt_with_notes("I am a nurse.", &[], "h"),
            ..ExpectedRequest::framed(&[], "h")
        })
    );
}

#[test]
fn writer_name_comes_after_the_notes() {
    let mut engine = engine_with(&Settings {
        writer_name: WriterName("Sara Novak".to_owned()),
        ..Settings::default()
    });

    let reaction = press(&mut engine, Key::Char('h'));

    assert_eq!(reaction, request_with(&["My name is Sara Novak."], "h"));
}

#[test]
fn recent_copy_comes_before_the_text() {
    let mut engine = engine_with(&Settings {
        clipboard_access: ClipboardAccess::Allowed,
        ..Settings::default()
    });
    let desktop = FakeDesktop::default().with_copy(
        "1Z999",
        Some(MAX_CLIPBOARD_AGE.saturating_sub(Duration::from_secs(1))),
    );

    let reaction = engine.on_key(Key::Char('h'), &desktop);

    assert_eq!(reaction, request_with(&["Text on my clipboard: “1Z999”"], "h"));
}

#[test]
fn old_copy_or_copy_from_before_the_start_stays_out() {
    let settings = Settings {
        clipboard_access: ClipboardAccess::Allowed,
        ..Settings::default()
    };
    let ages = [Some(MAX_CLIPBOARD_AGE), None];

    let reactions = ages.map(|age| {
        engine_with(&settings).on_key(Key::Char('h'), &FakeDesktop::default().with_copy("1Z999", age))
    });

    assert_eq!(reactions, ages.map(|_| request("h")));
}

#[test]
fn denied_clipboard_is_never_read() {
    let mut engine = engine_with(&Settings {
        clipboard_access: ClipboardAccess::Denied,
        ..Settings::default()
    });
    let desktop = FakeDesktop::default().with_copy("1Z999", Some(Duration::ZERO));

    let reaction = engine.on_key(Key::Char('h'), &desktop);

    assert_eq!((reaction, desktop.clipboard_reads.get()), (request("h"), 0));
}

#[test]
fn address_field_of_an_email_gets_no_request() {
    let mut engine = Engine::default();

    let reaction = engine.on_key(
        Key::Char('a'),
        &FakeDesktop::default().in_app(MAIL_APP).with_field_name("To:"),
    );

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn subject_field_of_an_email_is_not_prepared() {
    let mut engine = Engine::default();

    let request = engine.prepare(&FakeDesktop::default().in_app(MAIL_APP).with_field_name("Subject"));

    assert_eq!(request, None);
}

#[test]
fn body_of_an_email_gets_a_request() {
    let mut engine = Engine::default();

    let reaction = engine.on_key(
        Key::Char('a'),
        &FakeDesktop::default().in_app(MAIL_APP).with_field_name("Message"),
    );

    assert!(matches!(reaction, Reaction::Request(_)), "{reaction:?}");
}

#[test]
fn debug_does_not_reveal_typed_or_suggested_text() {
    let engine = engine_showing("secret", "word");

    let debug = format!("{engine:?}");

    assert!(!debug.contains("secret") && !debug.contains("word"));
}

#[test]
fn session_knows_its_app() {
    let mut engine = Engine::default();

    engine.on_key(
        Key::Char('S'),
        &FakeDesktop::default().in_app(MAIL_APP).with_field_name("Message"),
    );

    assert_eq!(
        engine.session_app().map(|app| app.app_id.as_str()),
        Some(MAIL_APP)
    );
}
