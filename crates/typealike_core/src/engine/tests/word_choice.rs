//! Tests the word bias, the phrases, the next words and the scope of each app.

use crate::desktop::Timestamp;
use crate::engine::{Engine, Reaction};
use crate::history::HistoryEntry;
use crate::inference::{InferenceError, InferenceRequest};
use crate::key::{Key, Press};
use crate::settings::{HistoryAccess, HistoryApp, HistoryScope, Settings, WordChoice};
use crate::test_support::{MAIL_APP, at_hour, completion, mail_entry, mail_history_app, on_own_days};

use super::support::{
    FakeDesktop, GREETING, Suggestion, engine_with_history, finish_typing, history_settings, mail_field,
    press, show, type_text,
};

fn word_choice_settings(history: HistoryAccess, apps: Vec<HistoryApp>) -> Settings {
    let mut settings = history_settings(history, apps);
    settings.personalization.word_choice = WordChoice::Medium;
    settings
}

fn word_bias_of_request(reaction: Reaction) -> Option<Vec<(String, f32)>> {
    match reaction {
        Reaction::Request(InferenceRequest::Complete { word_bias, .. }) => {
            word_bias.map(|bias| bias.words.to_vec())
        }
        _ => None,
    }
}

fn engine_with_words(settings: &Settings) -> Engine {
    engine_with_history(settings, on_own_days(vec![mail_entry("reckon reckon"); 10]))
}

#[test]
fn request_prefers_the_words_of_the_history() {
    let mut engine = engine_with_words(&word_choice_settings(HistoryAccess::On, Vec::new()));

    let reaction = engine.on_key(Key::Char('h'), Press::First, &mail_field());

    assert_eq!(
        word_bias_of_request(reaction),
        Some(vec![("reckon".to_owned(), 1.0)])
    );
}

#[test]
fn request_has_no_word_bias_when_the_history_or_the_app_is_off() {
    let settings = [
        word_choice_settings(HistoryAccess::Off, Vec::new()),
        word_choice_settings(HistoryAccess::On, vec![mail_history_app(HistoryScope::Off)]),
    ];

    let biases = settings.map(|settings| {
        word_bias_of_request(engine_with_words(&settings).on_key(Key::Char('h'), Press::First, &mail_field()))
    });

    assert_eq!(biases, [None, None]);
}

fn word_bias_version_of_request(reaction: Reaction) -> Option<u64> {
    match reaction {
        Reaction::Request(InferenceRequest::Complete { word_bias, .. }) => word_bias.map(|bias| bias.version),
        _ => None,
    }
}

#[test]
fn new_word_choice_gives_a_new_version() {
    let mut engine = engine_with_words(&word_choice_settings(HistoryAccess::On, Vec::new()));
    let before = word_bias_version_of_request(engine.on_key(Key::Char('h'), Press::First, &mail_field()));
    let mut strong_settings = word_choice_settings(HistoryAccess::On, Vec::new());
    strong_settings.personalization.word_choice = WordChoice::Strong;

    engine.use_settings(&strong_settings);

    let after = word_bias_version_of_request(engine.on_key(Key::Char('i'), Press::First, &mail_field()));
    assert!(before < after, "{before:?} {after:?}");
}

#[test]
fn new_history_gives_a_new_version() {
    let mut engine = engine_with_words(&word_choice_settings(HistoryAccess::On, Vec::new()));
    let before = word_bias_version_of_request(engine.on_key(Key::Char('h'), Press::First, &mail_field()));
    engine.on_completion(completion(""));

    finish_typing(
        &mut engine,
        &FakeDesktop::with_field_text(GREETING).in_app(MAIL_APP),
        Suggestion::Accepted,
    );

    let after = word_bias_version_of_request(engine.on_key(Key::Char('h'), Press::First, &mail_field()));
    assert!(before < after, "{before:?} {after:?}");
}

fn engine_with_phrases(settings: &Settings) -> Engine {
    engine_with_history(
        settings,
        on_own_days(vec![mail_entry("Let me know if it works."); 3]),
    )
}

fn mail_field_around(before_caret: &str, after_caret: &str) -> FakeDesktop {
    FakeDesktop::with_text_around(before_caret, after_caret).in_app(MAIL_APP)
}

#[test]
fn phrase_that_the_writer_repeats_shows_without_a_request() {
    let mut engine = engine_with_phrases(&word_choice_settings(HistoryAccess::On, Vec::new()));

    let reaction = engine.on_key(
        Key::Char(' '),
        Press::First,
        &mail_field_around("Please let me know", ""),
    );

    assert_eq!(reaction, show("if"));
}

/// Returns the engine with three copies of a phrase written at 22:00 of the UTC day 5.
fn engine_with_evening_phrase() -> Engine {
    let text = HistoryEntry {
        written_at: Some(at_hour(5, 22)),
        ..mail_entry("Let me know if it works.")
    };
    engine_with_history(
        &word_choice_settings(HistoryAccess::On, Vec::new()),
        vec![text; 3],
    )
}

fn phrase_shows_at(engine: &mut Engine, now: Timestamp) -> bool {
    let desktop = mail_field_around("Please let me know", "").at(now);
    let shows = engine.on_key(Key::Char(' '), Press::First, &desktop) == show("if");
    engine.reset();
    shows
}

#[test]
fn phrase_of_one_evening_shows_until_a_day_later_also_after_midnight() {
    let clocks = [at_hour(6, 2), at_hour(6, 21), at_hour(7, 12)];

    let shows = clocks.map(|now| phrase_shows_at(&mut engine_with_evening_phrase(), now));

    assert_eq!(shows, [true, true, false]);
}

#[test]
fn engine_builds_the_tables_again_when_the_clock_moves_on() {
    let mut engine = engine_with_evening_phrase();

    let shows = [at_hour(6, 2), at_hour(7, 12)].map(|now| phrase_shows_at(&mut engine, now));

    assert_eq!(shows, [true, false]);
}

#[test]
fn phrase_shows_only_when_the_line_after_the_caret_is_empty() {
    let texts_after = ["", "\nBye", " today"];

    let shows = texts_after.map(|text_after| {
        let mut engine = engine_with_phrases(&word_choice_settings(HistoryAccess::On, Vec::new()));
        engine.on_key(
            Key::Char(' '),
            Press::First,
            &mail_field_around("Please let me know", text_after),
        ) == show("if")
    });

    assert_eq!(shows, [true, true, false]);
}

#[test]
fn phrase_needs_the_word_choice_the_history_and_the_app() {
    let settings = [
        history_settings(HistoryAccess::On, Vec::new()),
        word_choice_settings(HistoryAccess::Off, Vec::new()),
        word_choice_settings(HistoryAccess::On, vec![mail_history_app(HistoryScope::Off)]),
    ];

    let requests = settings.map(|settings| {
        let reaction = engine_with_phrases(&settings).on_key(
            Key::Char(' '),
            Press::First,
            &mail_field_around("Please let me know", ""),
        );
        matches!(reaction, Reaction::Request(_))
    });

    assert_eq!(requests, [true, true, true]);
}

fn next_word_bias_after(settings: &Settings, before_caret: &str) -> Vec<(String, f32)> {
    let mut engine = engine_with_history(
        settings,
        on_own_days([
            mail_entry("Let me know if it works."),
            mail_entry("Let me know when you can."),
        ]),
    );
    match engine.on_key(Key::Char(' '), Press::First, &mail_field_around(before_caret, "")) {
        Reaction::Request(InferenceRequest::Complete { next_word_bias, .. }) => next_word_bias.0,
        _ => Vec::new(),
    }
}

#[test]
fn request_prefers_the_words_that_the_writer_wrote_after_the_last_words() {
    let settings = word_choice_settings(HistoryAccess::On, Vec::new());

    let next = next_word_bias_after(&settings, "Please let me know");

    assert_eq!(next, [("if".to_owned(), 0.5), ("when".to_owned(), 0.5)]);
}

#[test]
fn request_has_no_next_word_bias_without_the_word_choice_the_history_or_the_app() {
    let settings = [
        history_settings(HistoryAccess::On, Vec::new()),
        word_choice_settings(HistoryAccess::Off, Vec::new()),
        word_choice_settings(HistoryAccess::On, vec![mail_history_app(HistoryScope::Off)]),
    ];

    let next = settings.map(|settings| next_word_bias_after(&settings, "Please let me know"));

    assert_eq!(next, [vec![], vec![], vec![]]);
}

#[test]
fn completion_of_an_older_request_keeps_the_phrase() {
    let mut engine = engine_with_phrases(&word_choice_settings(HistoryAccess::On, Vec::new()));
    engine.on_key(
        Key::Char('n'),
        Press::First,
        &mail_field_around("Please let me k", ""),
    );
    type_text(&mut engine, "ow");

    let reaction = engine.on_completion(Err(InferenceError::Superseded));

    assert_eq!(reaction, show(" if"));
}

#[test]
fn earlier_suggestion_at_the_caret_comes_before_a_phrase() {
    let mut engine = engine_with_phrases(&word_choice_settings(HistoryAccess::On, Vec::new()));
    engine.on_key(
        Key::Char(' '),
        Press::First,
        &mail_field_around("Please let me", ""),
    );
    engine.on_completion(completion("know if it works"));
    type_text(&mut engine, "know x");

    let reaction = press(&mut engine, Key::Backspace);

    assert_eq!(reaction, show("if it works"));
}

const CHAT_APP: &str = "net.whatsapp.WhatsApp";
const OTHER_CHAT_APP: &str = "com.tinyspeck.slackmacgap";

fn app_with(app_id: &str, scope: HistoryScope) -> HistoryApp {
    HistoryApp {
        app_id: app_id.to_owned(),
        app_name: app_id.to_owned(),
        app_path: None,
        scope,
    }
}

fn chat_entry(text: &str) -> HistoryEntry {
    HistoryEntry {
        app_id: CHAT_APP.to_owned(),
        ..mail_entry(text)
    }
}

/// The settings of each case of the pools: the chat app, then the other chat app.
fn pool_cases() -> [(HistoryScope, HistoryScope); 3] {
    [
        (HistoryScope::AllApps, HistoryScope::AllApps),
        (HistoryScope::ThisApp, HistoryScope::AllApps),
        (HistoryScope::AllApps, HistoryScope::ThisApp),
    ]
}

fn pool_settings((chat, other_chat): (HistoryScope, HistoryScope)) -> Settings {
    word_choice_settings(
        HistoryAccess::On,
        vec![app_with(CHAT_APP, chat), app_with(OTHER_CHAT_APP, other_chat)],
    )
}

#[test]
fn only_apps_with_all_apps_give_examples_to_each_other() {
    let shared = pool_cases().map(|scopes| {
        let mut engine = engine_with_history(
            &pool_settings(scopes),
            vec![chat_entry("See you at the station at six")],
        );
        let reaction = engine.on_key(
            Key::Char('h'),
            Press::First,
            &FakeDesktop::with_field_text("").in_app(OTHER_CHAT_APP),
        );
        matches!(reaction, Reaction::Request(InferenceRequest::Complete { prompt, .. })
            if prompt.0.contains("See you at the station"))
    });

    assert_eq!(shared, [true, false, false]);
}

#[test]
fn only_apps_with_all_apps_give_phrases_to_each_other() {
    let shown = pool_cases().map(|scopes| {
        let mut engine = engine_with_history(
            &pool_settings(scopes),
            on_own_days(vec![chat_entry("Let me know if it works."); 3]),
        );
        let desktop = FakeDesktop::with_field_text("Please let me know").in_app(OTHER_CHAT_APP);
        engine.on_key(Key::Char(' '), Press::First, &desktop) == show("if")
    });

    assert_eq!(shown, [true, false, false]);
}

/// The request that a key after "Please let me know" in the other chat app gets, with the earlier
/// writing of the chat app and the pools of `scopes`.
fn request_in_other_chat_app(scopes: (HistoryScope, HistoryScope)) -> Reaction {
    let mut engine = engine_with_history(
        &pool_settings(scopes),
        on_own_days([
            chat_entry("Let me know if it works. Reckon."),
            chat_entry("Let me know when you can. Reckon."),
            chat_entry("Reckon reckon."),
        ]),
    );
    let desktop = FakeDesktop::with_field_text("Please let me know").in_app(OTHER_CHAT_APP);
    engine.on_key(Key::Char(' '), Press::First, &desktop)
}

#[test]
fn only_apps_with_all_apps_give_word_bias_to_each_other() {
    let biases = pool_cases().map(|scopes| word_bias_of_request(request_in_other_chat_app(scopes)).is_some());

    assert_eq!(biases, [true, false, false]);
}

#[test]
fn only_apps_with_all_apps_give_next_words_to_each_other() {
    let next_words = pool_cases().map(|scopes| {
        matches!(
            request_in_other_chat_app(scopes),
            Reaction::Request(InferenceRequest::Complete { next_word_bias, .. }) if !next_word_bias.0.is_empty()
        )
    });

    assert_eq!(next_words, [true, false, false]);
}

#[test]
fn app_with_only_this_app_uses_its_own_texts() {
    let mut engine = engine_with_history(
        &pool_settings((HistoryScope::ThisApp, HistoryScope::AllApps)),
        on_own_days(vec![chat_entry("Let me know if it works."); 3]),
    );

    let reaction = engine.on_key(
        Key::Char(' '),
        Press::First,
        &FakeDesktop::with_field_text("Please let me know").in_app(CHAT_APP),
    );

    assert_eq!(reaction, show("if"));
}

#[test]
fn new_scope_of_an_app_builds_the_tables_again() {
    let mut engine = engine_with_history(
        &pool_settings((HistoryScope::AllApps, HistoryScope::AllApps)),
        on_own_days(vec![chat_entry("Let me know if it works."); 3]),
    );
    engine.use_settings(&pool_settings((HistoryScope::ThisApp, HistoryScope::AllApps)));

    let reaction = engine.on_key(
        Key::Char(' '),
        Press::First,
        &FakeDesktop::with_field_text("Please let me know").in_app(OTHER_CHAT_APP),
    );

    assert!(matches!(reaction, Reaction::Request(_)), "{reaction:?}");
}
