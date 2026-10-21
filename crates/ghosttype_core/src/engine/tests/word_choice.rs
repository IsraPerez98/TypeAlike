//! Tests the word bias, the phrases, the next words and the scope of each app.

use crate::desktop::FieldText;
use crate::engine::{Engine, Reaction};
use crate::history::{HistoryEntry, HistoryPool};
use crate::inference::{InferenceError, InferenceRequest};
use crate::key::Key;
use crate::settings::{HistoryAccess, HistoryApp, HistoryScope, Settings, WordChoice};

use super::support::{
    FakeDesktop, GREETING, engine_with, field_of, finish_typing, history_settings, mail_app, mail_entry,
    mail_field, show, type_text,
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
    let mut engine = engine_with(settings);
    engine.use_history(vec![mail_entry(&"reckon ".repeat(20))]);
    engine
}

#[test]
fn request_prefers_the_words_of_the_history() {
    let mut engine = engine_with_words(&word_choice_settings(HistoryAccess::On, Vec::new()));

    let reaction = engine.on_key(Key::Char('h'), &mail_field());

    assert_eq!(
        word_bias_of_request(reaction),
        Some(vec![("reckon".to_owned(), 1.0)])
    );
}

#[test]
fn request_has_no_word_bias_when_the_history_or_the_app_is_off() {
    let settings = [
        word_choice_settings(HistoryAccess::Off, Vec::new()),
        word_choice_settings(HistoryAccess::On, vec![mail_app(HistoryScope::Off)]),
    ];

    let biases = settings.map(|settings| {
        word_bias_of_request(engine_with_words(&settings).on_key(Key::Char('h'), &mail_field()))
    });

    assert_eq!(biases, [None, None]);
}

#[test]
fn new_word_choice_or_new_history_gives_a_new_version() {
    let mut engine = engine_with_words(&word_choice_settings(HistoryAccess::On, Vec::new()));
    let first = engine
        .tables
        .word_bias(&HistoryPool::Shared)
        .map(|bias| bias.version);
    engine.use_settings(&{
        let mut settings = word_choice_settings(HistoryAccess::On, Vec::new());
        settings.personalization.word_choice = WordChoice::Strong;
        settings
    });
    let second = engine
        .tables
        .word_bias(&HistoryPool::Shared)
        .map(|bias| bias.version);
    finish_typing(
        &mut engine,
        &field_of(Some("com.apple.mail"), None, GREETING),
        true,
    );

    let third = engine
        .tables
        .word_bias(&HistoryPool::Shared)
        .map(|bias| bias.version);

    assert!(first < second && second < third, "{first:?} {second:?} {third:?}");
}

fn engine_with_phrases(settings: &Settings) -> Engine {
    let mut engine = engine_with(settings);
    engine.use_history(vec![mail_entry("Let me know if it works."); 3]);
    engine
}

fn mail_field_around(before_caret: &str, after_caret: &str) -> FakeDesktop {
    let mut desktop = field_of(Some("com.apple.mail"), None, before_caret);
    desktop.surroundings.field_text = Some(FieldText {
        before_caret: before_caret.to_owned(),
        after_caret: after_caret.to_owned(),
    });
    desktop
}

#[test]
fn phrase_that_the_writer_repeats_shows_without_a_request() {
    let mut engine = engine_with_phrases(&word_choice_settings(HistoryAccess::On, Vec::new()));

    let reaction = engine.on_key(Key::Char(' '), &mail_field_around("Please let me know", ""));

    assert_eq!(reaction, show("if"));
}

#[test]
fn phrase_shows_only_when_the_line_after_the_caret_is_empty() {
    let texts_after = ["", "\nBye", " today"];

    let shows = texts_after.map(|text_after| {
        let mut engine = engine_with_phrases(&word_choice_settings(HistoryAccess::On, Vec::new()));
        engine.on_key(
            Key::Char(' '),
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
        word_choice_settings(HistoryAccess::On, vec![mail_app(HistoryScope::Off)]),
    ];

    let requests = settings.map(|settings| {
        let reaction = engine_with_phrases(&settings)
            .on_key(Key::Char(' '), &mail_field_around("Please let me know", ""));
        matches!(reaction, Reaction::Request(_))
    });

    assert_eq!(requests, [true, true, true]);
}

fn next_word_bias_after(settings: &Settings, before_caret: &str) -> Vec<(String, f32)> {
    let mut engine = engine_with(settings);
    engine.use_history(vec![
        mail_entry("Let me know if it works."),
        mail_entry("Let me know when you can."),
    ]);
    match engine.on_key(Key::Char(' '), &mail_field_around(before_caret, "")) {
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
        word_choice_settings(HistoryAccess::On, vec![mail_app(HistoryScope::Off)]),
    ];

    let next = settings.map(|settings| next_word_bias_after(&settings, "Please let me know"));

    assert_eq!(next, [vec![], vec![], vec![]]);
}

#[test]
fn completion_of_an_older_request_keeps_the_phrase() {
    let mut engine = engine_with_phrases(&word_choice_settings(HistoryAccess::On, Vec::new()));
    engine.on_key(Key::Char('n'), &mail_field_around("Please let me k", ""));
    type_text(&mut engine, "ow");

    let reaction = engine.on_completion(Err(InferenceError::Superseded));

    assert_eq!(reaction, show(" if"));
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
        let mut engine = engine_with(&pool_settings(scopes));
        engine.use_history(vec![chat_entry("See you at the station at six")]);
        let reaction = engine.on_key(Key::Char('h'), &field_of(Some(OTHER_CHAT_APP), None, ""));
        matches!(reaction, Reaction::Request(InferenceRequest::Complete { prompt, .. })
            if prompt.0.contains("See you at the station"))
    });

    assert_eq!(shared, [true, false, false]);
}

#[test]
fn only_apps_with_all_apps_give_phrases_to_each_other() {
    let shown = pool_cases().map(|scopes| {
        let mut engine = engine_with(&pool_settings(scopes));
        engine.use_history(vec![chat_entry("Let me know if it works."); 3]);
        let desktop = field_of(Some(OTHER_CHAT_APP), None, "Please let me know");
        engine.on_key(Key::Char(' '), &desktop) == show("if")
    });

    assert_eq!(shown, [true, false, false]);
}

#[test]
fn only_apps_with_all_apps_give_next_words_and_word_bias_to_each_other() {
    let biases = pool_cases().map(|scopes| {
        let mut engine = engine_with(&pool_settings(scopes));
        engine.use_history(vec![
            chat_entry("Let me know if it works. Reckon reckon reckon."),
            chat_entry("Let me know when you can."),
        ]);
        let desktop = field_of(Some(OTHER_CHAT_APP), None, "Please let me know");
        match engine.on_key(Key::Char(' '), &desktop) {
            Reaction::Request(InferenceRequest::Complete {
                word_bias,
                next_word_bias,
                ..
            }) => (word_bias.is_some(), !next_word_bias.0.is_empty()),
            _ => (false, false),
        }
    });

    assert_eq!(biases, [(true, true), (false, false), (false, false)]);
}

#[test]
fn app_with_only_this_app_uses_its_own_texts() {
    let mut engine = engine_with(&pool_settings((HistoryScope::ThisApp, HistoryScope::AllApps)));
    engine.use_history(vec![chat_entry("Let me know if it works."); 3]);

    let reaction = engine.on_key(
        Key::Char(' '),
        &field_of(Some(CHAT_APP), None, "Please let me know"),
    );

    assert_eq!(reaction, show("if"));
}

#[test]
fn new_scope_of_an_app_builds_the_tables_again() {
    let mut engine = engine_with(&pool_settings((HistoryScope::AllApps, HistoryScope::AllApps)));
    engine.use_history(vec![chat_entry("Let me know if it works."); 3]);
    engine.use_settings(&pool_settings((HistoryScope::ThisApp, HistoryScope::AllApps)));

    let reaction = engine.on_key(
        Key::Char(' '),
        &field_of(Some(OTHER_CHAT_APP), None, "Please let me know"),
    );

    assert!(matches!(reaction, Reaction::Request(_)), "{reaction:?}");
}
