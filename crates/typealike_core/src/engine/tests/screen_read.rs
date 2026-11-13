//! Tests the reads of the screen text and how a session uses them.

use crate::desktop::{
    Confidence, MAX_CHARS_BEFORE_CARET, OcrArea, OcrRead, ScreenLine, ScreenRead, ScreenReadId, ScreenRect,
    ScreenText, Timestamp,
};
use crate::engine::{Engine, Reaction, ScreenAnswer};
use crate::excerpt::estimated_tokens;
use crate::inference::{Backend, InferenceRequest};
use crate::key::{Key, Press};
use crate::settings::{OcrLevel, PromptPreparation, Settings, SuggestionLength};
use crate::test_support::completion;

use super::support::{FakeDesktop, engine_waiting_for, engine_with, press, request, request_with};

const CHAT_APP: &str = "com.tinyspeck.slackmacgap";
const FAST_OCR: OcrRead = OcrRead {
    level: OcrLevel::Fast,
    area: OcrArea::Window,
};
const MINUTE: u64 = 60;
const BOOKING: &str = "The table is booked for 8:30 at Casa Lupa.";
const CAKE: &str = "Bring the cake from the bakery on Elm Street.";
const FLOWERS: &str = "Ana will pick up the flowers in the morning.";
const KIDS: &str = "Leo drives the kids to the party at noon.";
const QUESTION: &str = "What time was the dinner again?";
const DEADLINE: &str = "Tell me before six, please.";
const TRAIN: &str = "The train to Halden leaves at 7:40.";
const PLATFORM: &str = "It leaves from platform 4, not 6.";

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

/// Returns a screen of a chat with a line for each text and top of `rows`, all above the field.
fn chat_with(rows: &[(&str, i32)]) -> ScreenText {
    ScreenText {
        lines: rows
            .iter()
            .map(|&(text, top)| ScreenLine {
                text: text.to_owned(),
                confidence: Confidence::CERTAIN,
                frame: ScreenRect {
                    left: 0,
                    top,
                    right: 500,
                    bottom: top + 20,
                },
            })
            .collect(),
        caret: None,
        field: ScreenRect {
            left: 0,
            top: 200,
            right: 500,
            bottom: 240,
        },
    }
}

fn scrolled_up() -> ScreenText {
    chat_with(&[(BOOKING, 40), (CAKE, 80), (FLOWERS, 120), (KIDS, 160)])
}

/// The chat of [`scrolled_up`] after a scroll of two lines down.
fn scrolled_down() -> ScreenText {
    chat_with(&[(FLOWERS, 40), (KIDS, 80), (QUESTION, 120), (DEADLINE, 160)])
}

/// A field of a chat app at `seconds` after a fixed moment.
fn chat_at(seconds: u64) -> FakeDesktop {
    FakeDesktop::default()
        .in_app(CHAT_APP)
        .at(Timestamp(1_793_890_800 + seconds))
}

/// Gives each text of `reads` to the engine as the answer to a new read, with its desktop.
fn read_screens<const COUNT: usize>(engine: &mut Engine, reads: [(ScreenText, FakeDesktop); COUNT]) {
    for (text, desktop) in reads {
        let id = engine.start_screen_read().expect("no session runs");
        engine.on_screen_read(ScreenRead { id, text: Some(text) }, &desktop);
    }
}

/// Returns the lines above the field that the prompt shows after the key `S` in `desktop`.
fn lines_above_after_key(engine: &mut Engine, desktop: &FakeDesktop) -> Vec<String> {
    lines_above_in(&engine.on_key(Key::Char('S'), Press::First, desktop))
}

/// Returns the lines above the field in the prompt of `reaction`.
fn lines_above_in(reaction: &Reaction) -> Vec<String> {
    let Reaction::Request(InferenceRequest::Complete { prompt, .. }) = reaction else {
        panic!("a key requests a completion: {reaction:?}");
    };
    let (_, above) = prompt
        .0
        .split_once("Above the field on my screen: “")
        .expect("the prompt has screen text");
    let (lines, _) = above.split_once('”').expect("the screen text ends with a quote");
    lines.lines().map(str::to_owned).collect()
}

/// Returns the estimated tokens of the lines above the field in the prompt of `reaction`.
fn screen_tokens_in(reaction: &Reaction) -> u32 {
    lines_above_in(reaction)
        .iter()
        .map(|line| estimated_tokens(line).0)
        .sum()
}

const CHAT_FIELD_TOP: i32 = 3000;

/// Returns a chat with `count` messages of about 14 estimated tokens each, the newest just above
/// the field. Each message of a chat with one more message is one line higher.
fn chat_screen(count: i32) -> ScreenText {
    let message = |index: i32| {
        let top = CHAT_FIELD_TOP - 20 * (count - index);
        ScreenLine {
            text: format!("Message {index}: see you at the lake on Friday."),
            confidence: Confidence::CERTAIN,
            frame: ScreenRect {
                left: 0,
                top,
                right: 500,
                bottom: top + 18,
            },
        }
    };
    ScreenText {
        lines: (0..count).map(message).collect(),
        caret: None,
        field: ScreenRect {
            left: 0,
            top: CHAT_FIELD_TOP,
            right: 500,
            bottom: CHAT_FIELD_TOP + 40,
        },
    }
}

fn chat_field() -> FakeDesktop {
    FakeDesktop::default().in_app(CHAT_APP)
}

fn graphics_card() -> Settings {
    Settings {
        backend: Backend::Metal,
        ..Settings::default()
    }
}

fn prompt_of(reaction: Reaction) -> String {
    let Reaction::Request(InferenceRequest::Complete { prompt, .. }) = reaction else {
        panic!("a key requests a completion: {reaction:?}");
    };
    prompt.0
}

fn read_screen(engine: &mut Engine, desktop: &FakeDesktop, screen: ScreenText) {
    let id = engine
        .start_screen_read()
        .expect("the engine can read the screen");
    engine.on_screen_read(
        ScreenRead {
            id,
            text: Some(screen),
        },
        desktop,
    );
}

/// Returns the prompts of the keys `S` and `u` in `desktop`. A read of `first` comes before `S`,
/// and a read of `second` comes between the keys.
fn prompts_around_a_read(desktop: &FakeDesktop, first: ScreenText, second: ScreenText) -> (String, String) {
    let mut engine = engine_with(&graphics_card());
    read_screen(&mut engine, desktop, first);
    let before = prompt_of(engine.on_key(Key::Char('S'), Press::First, desktop));
    read_screen(&mut engine, desktop, second);
    let after = prompt_of(engine.on_key(Key::Char('u'), Press::First, desktop));
    (before, after)
}

/// Returns the prompt of the keys of `typed` in `desktop` after one read of `screen`.
fn prompt_after_one_read(desktop: &FakeDesktop, screen: ScreenText, typed: &str) -> String {
    let mut engine = engine_with(&graphics_card());
    read_screen(&mut engine, desktop, screen);
    let reaction = typed
        .chars()
        .map(|character| engine.on_key(Key::Char(character), Press::First, desktop))
        .last()
        .expect("the text is not empty");
    prompt_of(reaction)
}

fn answer_read(engine: &mut Engine, id: ScreenReadId, text: Option<ScreenText>) -> ScreenAnswer {
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
fn long_screen_text_keeps_more_tokens_on_a_graphics_card_or_when_the_prompt_is_prepared_early() {
    let cases = [
        (Backend::Cpu, PromptPreparation::OnFirstKey, 300),
        (Backend::Cpu, PromptPreparation::OnFocus, 500),
        (Backend::Vulkan, PromptPreparation::OnFirstKey, 800),
        (Backend::Cuda, PromptPreparation::OnFocus, 800),
        (Backend::Metal, PromptPreparation::OnFirstKey, 800),
        (Backend::Metal, PromptPreparation::OnFocus, 800),
    ];

    let kept_tokens = cases.map(|(backend, prompt_preparation, _)| {
        let mut engine = engine_with(&Settings {
            backend,
            prompt_preparation,
            ..Settings::default()
        });
        let id = engine.start_screen_read().expect("no session runs");
        answer_read(&mut engine, id, Some(screen_with_lines(100)));
        screen_tokens_in(&press(&mut engine, Key::Char('S')))
    });

    assert_eq!(kept_tokens, cases.map(|(_, _, tokens)| tokens));
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
    let room = Backend::Cpu.context_tokens().0 - SuggestionLength::VeryLong.max_tokens().0;
    assert!(estimated_tokens(&prompt.0).0 <= room, "{prompt:?}");
}

#[test]
fn answer_to_an_older_read_is_dropped() {
    let mut engine = Engine::default();
    let older = engine.start_screen_read().expect("no session runs");
    engine.start_screen_read();

    let answer = answer_read(&mut engine, older, Some(screen_with("Dinner at 7?")));

    assert_eq!(answer, ScreenAnswer::Dropped);
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

    let ocr = engine.start_ocr_after(read, None, FAST_OCR);

    assert!(ocr.is_some());
}

#[test]
fn no_ocr_read_starts_after_an_older_read() {
    let mut engine = Engine::default();
    let older = engine.start_screen_read().expect("no session runs");
    engine.start_screen_read();

    let ocr = engine.start_ocr_after(older, None, FAST_OCR);

    assert_eq!(ocr, None);
}

#[test]
fn no_ocr_read_starts_after_an_ocr_read() {
    let mut engine = Engine::default();
    let read = engine.start_screen_read().expect("no session runs");
    let ocr = engine
        .start_ocr_after(read, None, FAST_OCR)
        .expect("the read is the newest");

    let next = engine.start_ocr_after(ocr, None, FAST_OCR);

    assert_eq!(next, None);
}

#[test]
fn answer_to_the_read_before_an_ocr_read_is_dropped() {
    let mut engine = Engine::default();
    let read = engine.start_screen_read().expect("no session runs");
    engine.start_ocr_after(read, None, FAST_OCR);

    let answer = answer_read(&mut engine, read, Some(screen_with("Dinner at 7?")));

    assert_eq!(answer, ScreenAnswer::Dropped);
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

    let answer = answer_read(&mut engine, id, Some(screen_with("Dinner at 7?")));

    assert_eq!(answer, ScreenAnswer::Dropped);
}

#[test]
fn new_chat_message_keeps_the_start_of_the_screen_text() {
    let (before, after) = prompts_around_a_read(&chat_field(), chat_screen(80), chat_screen(81));

    let (old_lines, _) = before
        .split_once("”\nThe last line above the field")
        .expect("the prompt has screen text above the field");
    assert!(after.starts_with(old_lines), "{before}\n\n{after}");
}

#[test]
fn chat_in_another_field_chooses_the_nearest_lines_again() {
    let desktop = chat_field();
    let mut engine = engine_with(&graphics_card());
    read_screen(&mut engine, &desktop, chat_screen(80));
    engine.on_key(Key::Char('S'), Press::First, &desktop);
    engine.discard_session();
    engine.forget_screen();
    read_screen(&mut engine, &desktop, chat_screen(81));

    let prompt = prompt_of(engine.on_key(Key::Char('S'), Press::First, &desktop));

    assert_eq!(prompt, prompt_after_one_read(&desktop, chat_screen(81), "S"));
}

#[test]
fn screen_text_outside_a_chat_or_an_email_app_chooses_the_nearest_lines_at_each_read() {
    let desktop = FakeDesktop::default();

    let (_, after) = prompts_around_a_read(&desktop, chat_screen(80), chat_screen(81));

    assert_eq!(after, prompt_after_one_read(&desktop, chat_screen(81), "Su"));
}

/// Returns the estimated tokens of the prompt after a key in a field of `before_caret` on
/// `backend`, after a read of a screen with `line`.
fn prompt_tokens_on(backend: Backend, line: &str, before_caret: &str) -> u32 {
    let mut engine = engine_with(&Settings {
        backend,
        ..Settings::default()
    });
    let id = engine.start_screen_read().expect("no session runs");
    answer_read(&mut engine, id, Some(screen_with(line)));
    let desktop = FakeDesktop::with_field_text(before_caret);
    let reaction = engine.on_key(Key::Char('字'), Press::First, &desktop);
    let Reaction::Request(InferenceRequest::Complete { prompt, .. }) = reaction else {
        panic!("a key in an open field requests a completion: {reaction:?}");
    };
    estimated_tokens(&prompt.0).0
}

#[test]
fn text_with_few_characters_for_each_token_keeps_room_for_the_completion_on_each_backend() {
    let backends = [Backend::Cpu, Backend::Metal];
    let longest_field = "字".repeat(MAX_CHARS_BEFORE_CARET - 1);

    let fits = backends.map(|backend| {
        let room = backend.context_tokens().0 - SuggestionLength::VeryLong.max_tokens().0;
        prompt_tokens_on(backend, &"屏".repeat(1100), &longest_field) <= room
    });

    assert_eq!(fits, [true, true]);
}

fn texts(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|&line| line.to_owned()).collect()
}

fn lines_of_scrolled_down() -> Vec<String> {
    texts(&[FLOWERS, KIDS, QUESTION, DEADLINE])
}

#[test]
fn scroll_keeps_the_lines_that_left_the_screen_above_the_new_lines() {
    let mut engine = Engine::default();
    read_screens(
        &mut engine,
        [(scrolled_up(), chat_at(0)), (scrolled_down(), chat_at(MINUTE))],
    );

    let lines = lines_above_after_key(&mut engine, &chat_at(MINUTE));

    assert_eq!(lines, texts(&[BOOKING, CAKE, FLOWERS, KIDS, QUESTION, DEADLINE]));
}

#[test]
fn read_that_shares_fewer_than_two_long_lines_starts_a_new_conversation() {
    let short_lines = chat_with(&[("Sounds good", 40), ("See you then", 80), (BOOKING, 120)]);
    let reads = [
        (scrolled_up(), chat_with(&[(TRAIN, 120), (PLATFORM, 160)])),
        (
            scrolled_up(),
            chat_with(&[(FLOWERS, 40), (TRAIN, 120), (PLATFORM, 160)]),
        ),
        (
            short_lines,
            chat_with(&[
                ("Sounds good", 40),
                ("See you then", 80),
                (TRAIN, 120),
                (PLATFORM, 160),
            ]),
        ),
    ];

    let lines = reads.map(|(earlier, newer)| {
        let mut engine = Engine::default();
        read_screens(&mut engine, [(earlier, chat_at(0)), (newer, chat_at(MINUTE))]);
        lines_above_after_key(&mut engine, &chat_at(MINUTE))
    });

    assert_eq!(
        lines,
        [
            texts(&[TRAIN, PLATFORM]),
            texts(&[FLOWERS, TRAIN, PLATFORM]),
            texts(&["Sounds good", "See you then", TRAIN, PLATFORM]),
        ]
    );
}

#[test]
fn lines_that_left_the_screen_go_ten_minutes_after_the_read_that_showed_them() {
    let read_and_key_times = [(11 * MINUTE, 11 * MINUTE), (MINUTE, 11 * MINUTE)];

    let lines = read_and_key_times.map(|(read_at, key_at)| {
        let mut engine = Engine::default();
        read_screens(
            &mut engine,
            [(scrolled_up(), chat_at(0)), (scrolled_down(), chat_at(read_at))],
        );
        lines_above_after_key(&mut engine, &chat_at(key_at))
    });

    assert_eq!(lines, [lines_of_scrolled_down(), lines_of_scrolled_down()]);
}

#[test]
fn desktop_without_a_clock_keeps_no_lines_that_left_the_screen() {
    let mut engine = Engine::default();
    let desktop = FakeDesktop::default().in_app(CHAT_APP);
    read_screens(
        &mut engine,
        [
            (scrolled_up(), FakeDesktop::default().in_app(CHAT_APP)),
            (scrolled_down(), FakeDesktop::default().in_app(CHAT_APP)),
        ],
    );

    let lines = lines_above_after_key(&mut engine, &desktop);

    assert_eq!(lines, lines_of_scrolled_down());
}

#[test]
fn another_field_drops_the_lines_that_left_the_screen() {
    let mut engine = Engine::default();
    read_screens(&mut engine, [(scrolled_up(), chat_at(0))]);
    engine.forget_screen();
    read_screens(&mut engine, [(scrolled_down(), chat_at(MINUTE))]);

    let lines = lines_above_after_key(&mut engine, &chat_at(MINUTE));

    assert_eq!(lines, lines_of_scrolled_down());
}

#[test]
fn place_outside_a_chat_or_an_email_app_shows_only_the_newest_read() {
    let mut engine = Engine::default();
    let document_at = |seconds| FakeDesktop::default().at(Timestamp(1_793_890_800 + seconds));
    read_screens(
        &mut engine,
        [
            (scrolled_up(), document_at(0)),
            (scrolled_down(), document_at(MINUTE)),
        ],
    );

    let lines = lines_above_after_key(&mut engine, &document_at(MINUTE));

    assert_eq!(lines, lines_of_scrolled_down());
}

#[test]
fn lines_on_the_screen_fill_the_budget_before_the_lines_that_left_it() {
    let mut engine = engine_with(&Settings {
        backend: Backend::Cpu,
        prompt_preparation: PromptPreparation::OnFirstKey,
        ..Settings::default()
    });
    let long_line = "word ".repeat(174);
    read_screens(
        &mut engine,
        [
            (
                chat_with(&[(FLOWERS, 40), (KIDS, 80), (&long_line, 160)]),
                chat_at(0),
            ),
            (scrolled_down(), chat_at(MINUTE)),
        ],
    );

    let lines = lines_above_after_key(&mut engine, &chat_at(MINUTE));

    assert!(lines[0].starts_with("… word word"), "{lines:?}");
    assert_eq!(lines[1..], lines_of_scrolled_down());
}
