//! Tests the requests that the settings and the preparation give.

use crate::desktop::MAX_CHARS_BEFORE_CARET;
use crate::engine::session::TRIMMED_CHARS_BEFORE_CARET;
use crate::engine::{Engine, Reaction};
use crate::inference::{
    Backend, InferenceError, InferenceRequest, LogProbability, SentenceEnd, TokenCount, TypoCheck,
};
use crate::key::{Key, Press};
use crate::settings::{Settings, SuggestionLength, TypoFix, TypoRule};
use crate::test_support::{ExpectedRequest, completion, preparation_request};

use super::support::{FakeDesktop, engine_showing, engine_waiting_for, engine_with, press, request, show};

#[test]
fn typo_fixes_off_asks_for_no_fixes() {
    let mut engine = engine_with(&Settings {
        typo_fix: TypoFix::Off,
        ..Settings::default()
    });

    let reaction = press(&mut engine, Key::Char('h'));

    assert!(
        matches!(
            reaction,
            Reaction::Request(InferenceRequest::Complete {
                typo_check: TypoCheck::Hide(_),
                ..
            })
        ),
        "unexpected {reaction:?}"
    );
}

#[test]
fn suggestion_continues_after_the_end_of_a_sentence() {
    let mut engine = engine_with(&Settings::default());

    let reaction = press(&mut engine, Key::Char('h'));

    assert!(
        matches!(
            reaction,
            Reaction::Request(InferenceRequest::Complete {
                sentence_end: SentenceEnd::Continue,
                ..
            })
        ),
        "unexpected {reaction:?}"
    );
}

#[test]
fn cpu_backend_asks_for_short_suggestions() {
    let mut engine = engine_with(&Settings {
        backend: Backend::Cpu,
        suggestion_length: SuggestionLength::VeryLong,
        ..Settings::default()
    });

    let reaction = press(&mut engine, Key::Char('h'));

    assert_eq!(
        reaction,
        Reaction::from(ExpectedRequest {
            max_tokens: TokenCount(6),
            min_log_probability: Some(SuggestionLength::Short.tail_cut()),
            ..ExpectedRequest::framed(&[], "h")
        })
    );
}

#[test]
fn typo_rule_that_suggests_asks_for_no_typo_cut() {
    let mut engine = engine_with(&Settings {
        typo_rule: TypoRule::Suggest,
        ..Settings::default()
    });

    let reaction = press(&mut engine, Key::Char('h'));

    assert_eq!(
        reaction,
        Reaction::from(ExpectedRequest {
            typo_check: TypoCheck::Off,
            ..ExpectedRequest::framed(&[], "h")
        })
    );
}

#[test]
fn very_short_suggestion_length_asks_for_three_tokens() {
    let mut engine = engine_with(&Settings {
        backend: Backend::Vulkan,
        suggestion_length: SuggestionLength::VeryShort,
        ..Settings::default()
    });

    let reaction = press(&mut engine, Key::Char('h'));

    assert_eq!(
        reaction,
        Reaction::from(ExpectedRequest {
            max_tokens: TokenCount(3),
            min_log_probability: Some(SuggestionLength::VeryShort.tail_cut()),
            ..ExpectedRequest::framed(&[], "h")
        })
    );
}

#[test]
fn longer_suggestion_lengths_ask_for_more_tokens_and_lower_tail_cuts() {
    let lengths = [SuggestionLength::Long, SuggestionLength::VeryLong];

    let reactions = lengths.map(|suggestion_length| {
        let mut engine = engine_with(&Settings {
            backend: Backend::Vulkan,
            suggestion_length,
            ..Settings::default()
        });
        press(&mut engine, Key::Char('h'))
    });

    assert_eq!(
        reactions,
        [(15, -5.9), (22, -8.6)].map(|(tokens, cut)| {
            Reaction::from(ExpectedRequest {
                max_tokens: TokenCount(tokens),
                min_log_probability: Some(LogProbability(cut)),
                ..ExpectedRequest::framed(&[], "h")
            })
        })
    );
}

#[test]
fn new_settings_apply_from_the_next_session() {
    let mut engine = Engine::default();
    press(&mut engine, Key::Char('h'));
    engine.on_completion(completion(""));
    engine.use_settings(&Settings::default());

    let reaction = press(&mut engine, Key::Char('i'));

    assert_eq!(reaction, request("hi"));
}

#[expect(clippy::unnecessary_wraps, reason = "prepare returns an Option")]
fn preparation(context: &str) -> Option<InferenceRequest> {
    Some(preparation_request(&[], context))
}

#[test]
fn preparing_decodes_the_field_text_and_adds_no_tokens() {
    let mut engine = Engine::default();

    let request = engine.prepare(&FakeDesktop::with_field_text("Hello wor"));

    assert_eq!(request, preparation("Hello wor"));
}

#[test]
fn preparing_an_empty_field_decodes_the_preface() {
    let mut engine = Engine::default();

    let request = engine.prepare(&FakeDesktop::default());

    assert_eq!(request, preparation(""));
}

#[test]
fn first_key_after_preparing_continues_the_prepared_session() {
    let mut engine = Engine::default();
    let desktop = FakeDesktop::with_field_text("Hello wor");
    engine.prepare(&desktop);

    let reaction = engine.on_key(Key::Char('l'), Press::First, &desktop);

    assert_eq!(
        (reaction, desktop.surroundings_reads.get()),
        (request("Hello worl"), 1)
    );
}

#[test]
fn first_key_after_preparing_a_long_field_continues_the_prepared_text() {
    let mut engine = Engine::default();
    let desktop = FakeDesktop::with_field_text(&format!("x{}", "y".repeat(MAX_CHARS_BEFORE_CARET - 1)));
    let recent_text = "y".repeat(TRIMMED_CHARS_BEFORE_CARET);
    let prepared = engine.prepare(&desktop);

    let reaction = engine.on_key(Key::Char('z'), Press::First, &desktop);

    assert_eq!(
        (prepared, reaction),
        (preparation(&recent_text), request(&format!("{recent_text}z")))
    );
}

#[test]
fn preparing_while_a_completion_runs_does_nothing() {
    let mut engine = engine_waiting_for("h");

    let request = engine.prepare(&FakeDesktop::with_field_text("Hello"));

    assert_eq!(request, None);
}

#[test]
fn preparing_an_idle_session_decodes_its_prompt() {
    let mut engine = engine_showing("h", "ello");

    let request = engine.prepare(&FakeDesktop::with_field_text("Hello"));

    assert_eq!(request, preparation("h"));
}

#[test]
fn completion_of_the_preparation_shows_nothing() {
    let mut engine = Engine::default();
    engine.prepare(&FakeDesktop::with_field_text("Hello wor"));

    let reaction = engine.on_completion(completion(""));

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn completion_of_a_key_after_the_preparation_shows() {
    let mut engine = Engine::default();
    let desktop = FakeDesktop::with_field_text("Hello wor");
    engine.prepare(&desktop);
    engine.on_key(Key::Char('l'), Press::First, &desktop);
    engine.on_completion(Err(InferenceError::Superseded));

    let reaction = engine.on_completion(completion("d"));

    assert_eq!(reaction, show("d"));
}

#[test]
fn request_carries_the_text_after_the_caret_on_its_line() {
    let cases = [
        (
            " yesterday, so you can see it.\nBest,\nDana",
            " yesterday, so you can see it.",
        ),
        ("\nBest,\nDana", ""),
        ("   \nBest,\nDana", ""),
        ("", ""),
    ];

    let lines_after = cases.map(|(after, _)| {
        let mut engine = Engine::default();
        let reaction = engine.on_key(
            Key::Char(' '),
            Press::First,
            &FakeDesktop::with_text_around("I paid the", after),
        );
        let Reaction::Request(InferenceRequest::Complete { line_after, .. }) = reaction else {
            panic!("unexpected {reaction:?}");
        };
        line_after
    });

    assert_eq!(lines_after, cases.map(|(_, line_after)| line_after.to_owned()));
}

#[test]
fn request_reads_a_no_break_space_after_the_caret_as_a_space() {
    let mut engine = Engine::default();

    let reaction = engine.on_key(
        Key::Char(' '),
        Press::First,
        &FakeDesktop::with_text_around("I paid the", " bill\u{a0}today"),
    );

    let Reaction::Request(InferenceRequest::Complete { line_after, .. }) = reaction else {
        panic!("unexpected {reaction:?}");
    };
    assert_eq!(line_after, " bill today");
}

#[test]
fn request_inside_a_line_has_no_tail_cut_and_twice_the_tokens() {
    let mut engine = Engine::default();

    let reaction = engine.on_key(
        Key::Char(' '),
        Press::First,
        &FakeDesktop::with_text_around("I paid the", " yesterday, so you can see it."),
    );

    let Reaction::Request(InferenceRequest::Complete {
        max_tokens,
        min_log_probability,
        ..
    }) = reaction
    else {
        panic!("unexpected {reaction:?}");
    };
    let length = Settings::default().suggestion_length_in_use();
    assert_eq!(
        (max_tokens, min_log_probability),
        (TokenCount(length.max_tokens().0 * 2), None)
    );
}
