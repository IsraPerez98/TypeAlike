//! Tests the earlier writing that a prompt shows and the texts that the history keeps.

use crate::history::HistoryEntry;
use crate::key::Key;
use crate::settings::{HistoryAccess, HistoryScope, KeptTexts, Settings};

use super::support::{
    FakeDesktop, GREETING, MAIL_APP, Suggestion, TYPED_TEXT, engine_with, engine_with_history, finish_typing,
    finish_typing_text, history_settings, mail_app, mail_entry, mail_field, request_with, type_text,
};

#[test]
fn earlier_writing_of_the_app_comes_before_the_text() {
    let mut engine = engine_with_history(
        &history_settings(HistoryAccess::On, Vec::new()),
        vec![mail_entry("Cheers,\nTom")],
    );

    let reaction = engine.on_key(Key::Char('h'), &mail_field());

    assert_eq!(
        reaction,
        request_with(
            &[
                "Where I am typing: Format: email",
                "My earlier writing:\n“Cheers,\nTom”"
            ],
            "h"
        )
    );
}

#[test]
fn earlier_writing_stays_out_when_the_history_or_the_app_is_off() {
    let settings = [
        history_settings(HistoryAccess::Off, Vec::new()),
        history_settings(HistoryAccess::On, vec![mail_app(HistoryScope::Off)]),
    ];

    let reactions = settings.map(|settings| {
        engine_with_history(&settings, vec![mail_entry("Cheers,\nTom")]).on_key(Key::Char('h'), &mail_field())
    });

    assert_eq!(
        reactions,
        [
            request_with(&["Where I am typing: Format: email"], "h"),
            request_with(&["Where I am typing: Format: email"], "h")
        ]
    );
}

const EARLIER_TEXT: &str = "Hi Ana,\n\nThe report is ready, and the numbers look goo";

fn kept_settings(kept_texts: KeptTexts) -> Settings {
    let mut settings = history_settings(HistoryAccess::On, Vec::new());
    settings.personalization.kept_texts = kept_texts;
    settings
}

#[test]
fn finished_text_with_an_accepted_suggestion_is_kept() {
    let mut engine = engine_with(&kept_settings(KeptTexts::AcceptedOnly));

    let saved = finish_typing(&mut engine, &mail_desktop(GREETING), Suggestion::Accepted);

    assert_eq!(saved, Some(vec![mail_entry(&format!("{EARLIER_TEXT}d."))]));
}

#[test]
fn finished_text_without_an_accepted_suggestion_is_kept_only_when_all_texts_are_kept() {
    let settings = [KeptTexts::AcceptedOnly, KeptTexts::All].map(kept_settings);

    let saved = settings.map(|settings| {
        finish_typing(
            &mut engine_with(&settings),
            &mail_desktop(GREETING),
            Suggestion::Ignored,
        )
    });

    assert_eq!(saved, [None, Some(vec![mail_entry(&format!("{EARLIER_TEXT}d"))])]);
}

#[test]
fn finished_text_is_not_kept_where_the_rules_forbid_it() {
    let cases = [
        (
            history_settings(HistoryAccess::Off, Vec::new()),
            mail_desktop(GREETING),
        ),
        (
            history_settings(HistoryAccess::On, vec![mail_app(HistoryScope::Off)]),
            mail_desktop(GREETING),
        ),
        (
            kept_settings(KeptTexts::All),
            FakeDesktop::with_field_text(GREETING).in_app("com.apple.Terminal"),
        ),
        (
            kept_settings(KeptTexts::All),
            FakeDesktop::with_field_text(GREETING),
        ),
        (
            kept_settings(KeptTexts::All),
            mail_desktop(GREETING).with_field_name("Subject"),
        ),
    ];

    let saved = cases.map(|(settings, desktop)| {
        finish_typing(&mut engine_with(&settings), &desktop, Suggestion::Accepted)
    });

    assert_eq!(saved, [None, None, None, None, None]);
}

fn long_field() -> String {
    "The terms of the lease are in force. ".repeat(6)
}

fn mail_desktop(before_caret: &str) -> FakeDesktop {
    FakeDesktop::with_field_text(before_caret).in_app(MAIL_APP)
}

#[test]
fn short_finished_text_is_not_kept() {
    let mut engine = engine_with(&kept_settings(KeptTexts::All));

    let saved = finish_typing_text(
        &mut engine,
        &mail_desktop(GREETING),
        "Thanks",
        Suggestion::Accepted,
    );

    assert_eq!(saved, None);
}

#[test]
fn text_that_the_field_held_before_the_session_is_not_kept() {
    let mut engine = engine_with(&kept_settings(KeptTexts::All));

    let saved = finish_typing_text(
        &mut engine,
        &mail_desktop(&long_field()),
        TYPED_TEXT,
        Suggestion::Ignored,
    );

    assert_eq!(saved, Some(vec![mail_entry(&format!("{TYPED_TEXT}d"))]));
}

#[test]
fn a_few_words_that_the_user_adds_to_a_long_field_are_not_kept() {
    let mut engine = engine_with(&kept_settings(KeptTexts::All));

    let saved = finish_typing_text(
        &mut engine,
        &mail_desktop(&long_field()),
        "Thanks",
        Suggestion::Ignored,
    );

    assert_eq!(saved, None);
}

#[test]
fn draft_that_an_entry_holds_is_kept_with_the_new_text_in_one_entry() {
    let mut engine = engine_with_history(
        &kept_settings(KeptTexts::All),
        vec![mail_entry(long_field().trim())],
    );

    let saved = finish_typing_text(
        &mut engine,
        &mail_desktop(&long_field()),
        TYPED_TEXT,
        Suggestion::Ignored,
    );

    assert_eq!(
        saved,
        Some(vec![mail_entry(&format!("{}{TYPED_TEXT}d", long_field()))])
    );
}

#[test]
fn backspace_into_the_text_of_the_field_makes_it_count_less() {
    let mut engine = engine_with(&kept_settings(KeptTexts::All));
    let desktop = mail_desktop(&long_field());
    for _ in 0..190 {
        engine.on_key(Key::Backspace, &desktop);
    }

    let saved = finish_typing_text(&mut engine, &desktop, TYPED_TEXT, Suggestion::Ignored);

    assert_eq!(
        saved,
        Some(vec![mail_entry(&format!("{}{TYPED_TEXT}d", &long_field()[..32]))])
    );
}

#[test]
fn cut_of_the_start_for_the_token_limit_makes_the_text_of_the_field_count_less() {
    let mut engine = engine_with(&kept_settings(KeptTexts::All));
    let desktop = mail_desktop(&"word ".repeat(798));

    let saved = finish_typing_text(&mut engine, &desktop, TYPED_TEXT, Suggestion::Ignored);

    assert_eq!(saved, Some(vec![mail_entry(&format!("{TYPED_TEXT}d"))]));
}

#[test]
fn pasted_text_is_not_kept() {
    let mut engine = engine_with(&kept_settings(KeptTexts::All));
    finish_typing(&mut engine, &mail_desktop(GREETING), Suggestion::Ignored);
    let pasted = "Pasted line from a web page that the user did not write. ".repeat(3);
    let desktop = mail_desktop(&format!("{EARLIER_TEXT}d{pasted}"));

    let saved = finish_typing_text(&mut engine, &desktop, "", Suggestion::Ignored);

    assert_eq!(saved, None);
}

#[test]
fn session_with_two_lines_is_kept_in_one_entry() {
    let mut engine = engine_with(&kept_settings(KeptTexts::All));
    let first_line = "Hi Ana,";
    engine.prepare(&mail_desktop(""));
    type_text(&mut engine, first_line);
    engine.on_key(Key::Enter, &mail_desktop(""));
    let new_line = mail_desktop(&format!("{first_line}\n"));
    engine.suggest_on_new_line(&new_line);

    let saved = finish_typing_text(&mut engine, &new_line, TYPED_TEXT, Suggestion::Ignored);

    assert_eq!(
        saved,
        Some(vec![mail_entry(&format!("{first_line}\n{TYPED_TEXT}d"))])
    );
}

#[test]
fn history_to_save_comes_once_for_each_change() {
    let mut engine = engine_with(&kept_settings(KeptTexts::All));
    finish_typing(&mut engine, &mail_desktop(GREETING), Suggestion::Ignored);

    let saved_again = engine.history_to_save().is_some();

    assert!(!saved_again);
}

#[test]
fn forgetting_an_app_drops_its_texts() {
    let mut engine = engine_with_history(
        &kept_settings(KeptTexts::All),
        vec![
            mail_entry("Cheers,\nTom"),
            HistoryEntry {
                app_id: "com.apple.Notes".to_owned(),
                ..mail_entry("Shopping list")
            },
        ],
    );

    engine.forget_history_of(MAIL_APP);

    assert_eq!(
        engine.history_to_save().map(<[HistoryEntry]>::to_vec),
        Some(vec![HistoryEntry {
            app_id: "com.apple.Notes".to_owned(),
            ..mail_entry("Shopping list")
        }])
    );
}
