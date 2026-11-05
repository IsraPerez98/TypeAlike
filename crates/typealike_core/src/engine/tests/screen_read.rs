//! Tests the reads of the screen text and how a session uses them.

use crate::desktop::{
    Confidence, MAX_CHARS_BEFORE_CARET, ScreenLine, ScreenRead, ScreenReadId, ScreenRect, ScreenText,
};
use crate::engine::{Engine, Reaction};
use crate::excerpt::estimated_tokens;
use crate::inference::{Backend, CONTEXT_TOKENS, InferenceRequest};
use crate::key::{Key, Press};
use crate::settings::{OcrLevel, Settings, SuggestionLength};
use crate::test_support::completion;

use super::support::{FakeDesktop, engine_waiting_for, engine_with, press, request, request_with};

fn screen_with(line: &str) -> ScreenText {
    ScreenText {
        lines: vec![ScreenLine {
            text: line.to_owned(),
            confidence: Confidence::CERTAIN,
            frame: ScreenRect {
                left: 0,
                top: 100,
                right: 500,
                bottom: 120,
            },
        }],
        caret: None,
        field: ScreenRect {
            left: 0,
            top: 200,
            right: 500,
            bottom: 240,
        },
    }
}

/// Returns a screen with `count` lines of 10 estimated tokens each, all above the field.
fn screen_with_lines(count: i32) -> ScreenText {
    let line = |index: i32| ScreenLine {
        text: "See you at the lake on Friday.".to_owned(),
        confidence: Confidence::CERTAIN,
        frame: ScreenRect {
            left: 0,
            top: 20 * index,
            right: 500,
            bottom: 20 * index + 18,
        },
    };
    ScreenText {
        lines: (0..count).map(line).collect(),
        caret: None,
        field: ScreenRect {
            left: 0,
            top: 20 * count + 100,
            right: 500,
            bottom: 20 * count + 140,
        },
    }
}

/// Returns the estimated tokens of the lines above the field in the prompt of `reaction`.
fn screen_tokens_in(reaction: &Reaction) -> u32 {
    let Reaction::Request(InferenceRequest::Complete { prompt, .. }) = reaction else {
        panic!("a key requests a completion: {reaction:?}");
    };
    let (_, above) = prompt
        .0
        .split_once("Above the field on my screen: “")
        .expect("the prompt has screen text");
    let (lines, _) = above.split_once('”').expect("the screen text ends with a quote");
    lines.lines().map(|line| estimated_tokens(line).0).sum()
}

fn answer_read(engine: &mut Engine, id: ScreenReadId, text: Option<ScreenText>) -> bool {
    engine.on_screen_read(ScreenRead { id, text }, &FakeDesktop::default())
}

fn engine_that_read(line: &str) -> Engine {
    let mut engine = Engine::default();
    let id = engine.start_screen_read().expect("no session runs");
    answer_read(&mut engine, id, Some(screen_with(line)));
    engine
}

#[test]
fn session_shows_the_screen_text_of_the_read_before_it() {
    let mut engine = engine_that_read("Dinner at 7?");

    let reaction = press(&mut engine, Key::Char('S'));

    assert_eq!(
        reaction,
        request_with(
            &[
                "Above the field on my screen: “Dinner at 7?”",
                "The last line above the field on my screen: “Dinner at 7?”"
            ],
            "S"
        )
    );
}

#[test]
fn long_screen_text_keeps_300_tokens_on_the_cpu_and_800_on_a_graphics_card() {
    let backends = [Backend::Cpu, Backend::Vulkan, Backend::Cuda, Backend::Metal];

    let kept_tokens = backends.map(|backend| {
        let mut engine = engine_with(&Settings {
            backend,
            ..Settings::default()
        });
        let id = engine.start_screen_read().expect("no session runs");
        answer_read(&mut engine, id, Some(screen_with_lines(100)));
        screen_tokens_in(&press(&mut engine, Key::Char('S')))
    });

    assert_eq!(kept_tokens, [300, 800, 800, 800]);
}

#[test]
fn session_that_starts_during_a_read_has_no_screen_text() {
    let mut engine = engine_that_read("Dinner at 7?");
    engine.start_screen_read();

    let reaction = press(&mut engine, Key::Char('S'));

    assert_eq!(reaction, request("S"));
}

#[test]
fn read_that_comes_during_a_session_goes_into_it() {
    let mut engine = Engine::default();
    let id = engine.start_screen_read().expect("no session runs");
    press(&mut engine, Key::Char('S'));
    answer_read(&mut engine, id, Some(screen_with("Dinner at 7?")));

    let reaction = press(&mut engine, Key::Char('u'));

    assert_eq!(
        reaction,
        request_with(
            &[
                "Above the field on my screen: “Dinner at 7?”",
                "The last line above the field on my screen: “Dinner at 7?”"
            ],
            "Su"
        )
    );
}

#[test]
fn screen_text_that_comes_during_a_session_keeps_room_for_the_completion() {
    let mut engine = Engine::default();
    let desktop = FakeDesktop::with_field_text(&"字".repeat(MAX_CHARS_BEFORE_CARET - 1));
    let id = engine.start_screen_read().expect("no session runs");
    engine.on_key(Key::Char('字'), Press::First, &desktop);
    engine.on_completion(completion(""));
    engine.on_screen_read(
        ScreenRead {
            id,
            text: Some(screen_with(&"屏".repeat(1100))),
        },
        &desktop,
    );

    let request = engine.prepare(&desktop);

    let Some(InferenceRequest::Complete { prompt, .. }) = request else {
        panic!("an idle session prepares its prompt: {request:?}");
    };
    let room = CONTEXT_TOKENS.0 - SuggestionLength::VeryLong.max_tokens().0;
    assert!(estimated_tokens(&prompt.0).0 <= room, "{prompt:?}");
}

#[test]
fn answer_to_an_older_read_is_dropped() {
    let mut engine = Engine::default();
    let older = engine.start_screen_read().expect("no session runs");
    engine.start_screen_read();

    let is_kept = answer_read(&mut engine, older, Some(screen_with("Dinner at 7?")));

    assert!(!is_kept);
}

#[test]
fn read_starts_during_a_session_with_stored_screen_text() {
    let mut engine = engine_that_read("Dinner at 7?");
    press(&mut engine, Key::Char('S'));

    let read = engine.start_screen_read();

    assert!(read.is_some());
}

#[test]
fn new_screen_text_replaces_the_stored_text_one_time() {
    let mut engine = engine_that_read("Dinner at 7?");
    press(&mut engine, Key::Char('S'));
    let id = engine
        .start_screen_read()
        .expect("the session uses stored screen text");
    answer_read(&mut engine, id, Some(screen_with("Lunch at 1?")));

    let reaction = press(&mut engine, Key::Char('u'));
    let next_read = engine.start_screen_read();

    assert_eq!(
        (reaction, next_read),
        (
            request_with(
                &[
                    "Above the field on my screen: “Lunch at 1?”",
                    "The last line above the field on my screen: “Lunch at 1?”"
                ],
                "Su"
            ),
            None
        )
    );
}

#[test]
fn same_screen_text_keeps_the_prompt() {
    let mut engine = engine_that_read("Dinner at 7?");
    press(&mut engine, Key::Char('S'));
    let id = engine
        .start_screen_read()
        .expect("the session uses stored screen text");
    answer_read(&mut engine, id, Some(screen_with("Dinner at 7?")));

    let reaction = press(&mut engine, Key::Char('u'));

    assert_eq!(
        reaction,
        request_with(
            &[
                "Above the field on my screen: “Dinner at 7?”",
                "The last line above the field on my screen: “Dinner at 7?”"
            ],
            "Su"
        )
    );
}

#[test]
fn failed_read_keeps_the_stored_screen_text() {
    let mut engine = engine_that_read("Dinner at 7?");
    press(&mut engine, Key::Char('S'));
    let id = engine
        .start_screen_read()
        .expect("the session uses stored screen text");
    answer_read(&mut engine, id, None);

    let reaction = press(&mut engine, Key::Char('u'));

    assert_eq!(
        reaction,
        request_with(
            &[
                "Above the field on my screen: “Dinner at 7?”",
                "The last line above the field on my screen: “Dinner at 7?”"
            ],
            "Su"
        )
    );
}

#[test]
fn read_starts_during_a_session_without_screen_text() {
    let mut engine = engine_waiting_for("S");

    let read = engine.start_screen_read();

    assert!(read.is_some());
}

#[test]
fn no_read_starts_while_a_read_of_the_session_runs() {
    let mut engine = engine_waiting_for("S");
    engine.start_screen_read();

    let read = engine.start_screen_read();

    assert_eq!(read, None);
}

#[test]
fn ocr_read_starts_after_the_newest_read() {
    let mut engine = Engine::default();
    let read = engine.start_screen_read().expect("no session runs");

    let ocr = engine.start_ocr_after(read, None, OcrLevel::Fast);

    assert!(ocr.is_some());
}

#[test]
fn no_ocr_read_starts_after_an_older_read() {
    let mut engine = Engine::default();
    let older = engine.start_screen_read().expect("no session runs");
    engine.start_screen_read();

    let ocr = engine.start_ocr_after(older, None, OcrLevel::Fast);

    assert_eq!(ocr, None);
}

#[test]
fn no_ocr_read_starts_after_an_ocr_read() {
    let mut engine = Engine::default();
    let read = engine.start_screen_read().expect("no session runs");
    let ocr = engine
        .start_ocr_after(read, None, OcrLevel::Fast)
        .expect("the read is the newest");

    let next = engine.start_ocr_after(ocr, None, OcrLevel::Fast);

    assert_eq!(next, None);
}

#[test]
fn answer_to_the_read_before_an_ocr_read_is_dropped() {
    let mut engine = Engine::default();
    let read = engine.start_screen_read().expect("no session runs");
    engine.start_ocr_after(read, None, OcrLevel::Fast);

    let is_kept = answer_read(&mut engine, read, Some(screen_with("Dinner at 7?")));

    assert!(!is_kept);
}

#[test]
fn session_that_got_an_answer_without_text_starts_no_other_read() {
    let mut engine = engine_waiting_for("S");
    let id = engine
        .start_screen_read()
        .expect("the session has no screen text");
    answer_read(&mut engine, id, None);

    let read = engine.start_screen_read();

    assert_eq!(read, None);
}

#[test]
fn forgotten_screen_text_is_not_used() {
    let mut engine = engine_that_read("Dinner at 7?");
    engine.forget_screen();

    let reaction = press(&mut engine, Key::Char('S'));

    assert_eq!(reaction, request("S"));
}

#[test]
fn answer_to_a_forgotten_read_is_dropped() {
    let mut engine = Engine::default();
    let id = engine.start_screen_read().expect("no session runs");
    engine.forget_screen();

    let is_kept = answer_read(&mut engine, id, Some(screen_with("Dinner at 7?")));

    assert!(!is_kept);
}

#[test]
fn text_with_few_characters_for_each_token_keeps_room_for_the_completion() {
    let mut engine = engine_that_read(&"屏".repeat(1100));
    let desktop = FakeDesktop::with_field_text(&"字".repeat(MAX_CHARS_BEFORE_CARET - 1));

    let reaction = engine.on_key(Key::Char('字'), Press::First, &desktop);

    let Reaction::Request(InferenceRequest::Complete { prompt, .. }) = reaction else {
        panic!("a key in an open field requests a completion: {reaction:?}");
    };
    let room = CONTEXT_TOKENS.0 - SuggestionLength::VeryLong.max_tokens().0;
    assert!(estimated_tokens(&prompt.0).0 <= room, "{prompt:?}");
}
