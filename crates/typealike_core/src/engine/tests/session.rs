//! Tests the start and the end of a session, its text and its preface.

use std::time::Duration;

use crate::desktop::{AfterCaret, LinesBelow, Surroundings};
use crate::engine::preface::MAX_CLIPBOARD_AGE;
use crate::engine::{Engine, Reaction};
use crate::inference::{Backend, InferenceRequest};
use crate::key::{Key, Press};
use crate::prompt;
use crate::settings::{ClipboardAccess, Settings, WriterName, WriterNotes};
use crate::test_support::{ExpectedRequest, MAIL_APP, completion};

use super::support::{
    FakeDesktop, cpu_text_limits, engine_showing, engine_waiting_for, engine_with, press, request,
    request_with,
};

#[test]
fn caret_is_at_the_line_end_while_its_line_has_no_text_after_it() {
    let texts_after = ["", "  \nSee you", " on Friday."];

    let line_ends = texts_after.map(|text_after| {
        let mut engine = Engine::default();
        engine.on_key(
            Key::Char(' '),
            Press::First,
            &FakeDesktop::with_text_around("Let's meet", text_after),
        );
        engine.is_caret_at_line_end(AfterCaret::Unknown)
    });

    assert_eq!(line_ends, [true, true, false]);
}

#[test]
fn lines_below_the_caret_have_text_while_any_text_follows_the_caret() {
    let texts_after = ["", "\n  \n\t", " on Friday.", "\nSee you", " on\n\nFriday."];

    let lines_below = texts_after.map(|text_after| {
        let mut engine = Engine::default();
        engine.on_key(
            Key::Char(' '),
            Press::First,
            &FakeDesktop::with_text_around("Let's meet", text_after),
        );
        engine.lines_below_caret()
    });

    assert_eq!(
        lines_below,
        [
            LinesBelow::Empty,
            LinesBelow::Empty,
            LinesBelow::WithText,
            LinesBelow::WithText,
            LinesBelow::WithText,
        ]
    );
}

#[test]
fn lines_below_the_caret_are_empty_without_a_session() {
    let engine = Engine::default();

    assert_eq!(engine.lines_below_caret(), LinesBelow::Empty);
}

#[test]
fn look_that_sees_the_text_after_the_caret_decides_over_the_text_that_the_session_read() {
    let looks = [
        AfterCaret::Unknown,
        AfterCaret::Nothing,
        AfterCaret::Text,
        AfterCaret::Selection,
    ];
    let texts_after = ["", "Try \"create a util\""];

    let line_ends = texts_after.map(|text_after| {
        let mut engine = Engine::default();
        engine.on_key(
            Key::Char(' '),
            Press::First,
            &FakeDesktop::with_text_around("Let's meet", text_after),
        );
        looks.map(|after| engine.is_caret_at_line_end(after))
    });

    assert_eq!(
        line_ends,
        [[true, true, false, false], [false, true, false, false]]
    );
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

    engine.on_key(Key::Char(' '), Press::First, &desktop);

    assert!(!engine.is_caret_at_line_end(AfterCaret::Nothing));
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
    let mut engine = engine_waiting_for(&format!("x{}", "y".repeat(cpu_text_limits().max_chars - 2)));
    engine.on_completion(completion(""));

    let reaction = press(&mut engine, Key::Char('y'));

    assert_eq!(reaction, request(&"y".repeat(cpu_text_limits().trimmed_chars)));
}

#[test]
fn context_below_the_limit_keeps_its_start() {
    let mut engine = engine_waiting_for(&format!("x{}", "y".repeat(cpu_text_limits().max_chars - 3)));
    engine.on_completion(completion(""));

    let reaction = press(&mut engine, Key::Char('y'));

    assert_eq!(
        reaction,
        request(&format!("x{}", "y".repeat(cpu_text_limits().max_chars - 2)))
    );
}

#[test]
fn first_key_continues_the_field_text_before_the_caret() {
    let mut engine = Engine::default();

    let reaction = engine.on_key(
        Key::Char('l'),
        Press::First,
        &FakeDesktop::with_field_text("Hello wor"),
    );

    assert_eq!(reaction, request("Hello worl"));
}

#[test]
fn first_key_reads_each_no_break_space_of_the_field_text_as_a_space() {
    let spaces = ['\u{a0}', '\u{202f}', '\u{2007}'];

    let reactions = spaces.map(|space| {
        let mut engine = Engine::default();
        engine.on_key(
            Key::Char('l'),
            Press::First,
            &FakeDesktop::with_field_text(&format!("Hello{space}wor")),
        )
    });

    assert_eq!(reactions, spaces.map(|_| request("Hello worl")));
}

#[test]
fn first_key_reads_the_field_text_without_zero_width_spaces() {
    let mut engine = Engine::default();

    let reaction = engine.on_key(
        Key::Char('l'),
        Press::First,
        &FakeDesktop::with_field_text("\u{200b}-\u{200b} Hello\u{2060}\u{feff} wor"),
    );

    assert_eq!(reaction, request("- Hello worl"));
}

#[test]
fn field_text_is_read_once_per_session() {
    let mut engine = Engine::default();
    let desktop = FakeDesktop::with_field_text("Hello ");
    engine.on_key(Key::Char('w'), Press::First, &desktop);
    engine.on_completion(completion(""));

    let reaction = engine.on_key(Key::Char('o'), Press::First, &desktop);

    assert_eq!(
        (reaction, desktop.surroundings_reads.get()),
        (request("Hello wo"), 1)
    );
}

#[test]
fn reset_reads_the_field_again() {
    let mut engine = Engine::default();
    engine.on_key(
        Key::Char('x'),
        Press::First,
        &FakeDesktop::with_field_text("Old "),
    );
    engine.on_completion(completion(""));
    engine.reset();

    let reaction = engine.on_key(
        Key::Char('d'),
        Press::First,
        &FakeDesktop::with_field_text("New wor"),
    );

    assert_eq!(reaction, request("New word"));
}

#[test]
fn backspace_deletes_from_the_field_text() {
    let mut engine = Engine::default();

    let reaction = engine.on_key(
        Key::Backspace,
        Press::First,
        &FakeDesktop::with_field_text("Helloo"),
    );

    assert_eq!(reaction, request("Hello"));
}

#[test]
fn long_field_text_keeps_its_end() {
    let mut engine = Engine::default();
    let before_caret = format!("x{}", "y".repeat(cpu_text_limits().max_chars - 1));

    let reaction = engine.on_key(
        Key::Char('y'),
        Press::First,
        &FakeDesktop::with_field_text(&before_caret),
    );

    assert_eq!(
        reaction,
        request(&"y".repeat(cpu_text_limits().trimmed_chars + 1))
    );
}

/// Returns the last line of the prompt of `reaction`: the text before the caret, when it has no
/// line break.
fn last_prompt_line(reaction: &Reaction) -> &str {
    let Reaction::Request(InferenceRequest::Complete { prompt, .. }) = reaction else {
        panic!("a key requests a completion: {reaction:?}");
    };
    prompt.0.rsplit_once('\n').map_or("", |(_, line)| line)
}

#[test]
fn graphics_card_keeps_more_of_a_long_field_text_than_the_cpu() {
    let cases = [
        (Backend::Cpu, 8000),
        (Backend::Metal, 8000),
        (Backend::Metal, 9500),
    ];

    let kept_chars = cases.map(|(backend, field_chars)| {
        let mut engine = engine_with(&Settings {
            backend,
            ..Settings::default()
        });
        let desktop = FakeDesktop::with_field_text(&"y".repeat(field_chars));
        let reaction = engine.on_key(Key::Char('y'), Press::First, &desktop);
        last_prompt_line(&reaction).chars().count()
    });

    assert_eq!(kept_chars, [3001, 8001, 7001]);
}

#[test]
fn text_on_a_graphics_card_that_reaches_the_limit_drops_old_characters_in_one_step() {
    let mut engine = engine_with(&Settings {
        backend: Backend::Metal,
        ..Settings::default()
    });
    let desktop = FakeDesktop::with_field_text(&format!("x{}", "y".repeat(8997)));
    engine.on_key(Key::Char('y'), Press::First, &desktop);
    engine.on_completion(completion(""));

    let reaction = press(&mut engine, Key::Char('y'));

    assert_eq!(last_prompt_line(&reaction), "y".repeat(7000));
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

    let reaction = engine.on_key(Key::Char('h'), Press::First, &desktop);

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
        engine_with(&settings).on_key(
            Key::Char('h'),
            Press::First,
            &FakeDesktop::default().with_copy("1Z999", age),
        )
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

    let reaction = engine.on_key(Key::Char('h'), Press::First, &desktop);

    assert_eq!((reaction, desktop.clipboard_reads.get()), (request("h"), 0));
}

#[test]
fn address_field_of_an_email_gets_no_request() {
    let mut engine = Engine::default();

    let reaction = engine.on_key(
        Key::Char('a'),
        Press::First,
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
fn search_field_gets_no_request() {
    let mut engine = Engine::default();

    let reaction = engine.on_key(
        Key::Char('a'),
        Press::First,
        &FakeDesktop::default().in_search_field(),
    );

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn search_field_is_not_prepared() {
    let mut engine = Engine::default();

    let request = engine.prepare(&FakeDesktop::default().in_search_field());

    assert_eq!(request, None);
}

#[test]
fn body_of_an_email_gets_a_request() {
    let mut engine = Engine::default();

    let reaction = engine.on_key(
        Key::Char('a'),
        Press::First,
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
        Press::First,
        &FakeDesktop::default().in_app(MAIL_APP).with_field_name("Message"),
    );

    assert_eq!(
        engine.session_app().map(|app| app.app_id.as_str()),
        Some(MAIL_APP)
    );
}
