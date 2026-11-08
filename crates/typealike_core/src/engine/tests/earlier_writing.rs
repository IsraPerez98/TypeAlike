//! Tests the earlier writing that a prompt shows and the texts that the history keeps.

use crate::desktop::{Confidence, ScreenLine, ScreenRead, ScreenRect, ScreenText, Timestamp};
use crate::engine::Engine;
use crate::history::HistoryEntry;
use crate::key::{Key, Press};
use crate::settings::{HistoryAccess, HistoryScope, KeptTexts, ReplyWords, Settings};
use crate::test_support::{MAIL_APP, mail_entry, mail_history_app};

use super::support::{
    FakeDesktop, GREETING, Suggestion, TYPED_TEXT, engine_with, engine_with_history, finish_typing,
    finish_typing_text, history_settings, mail_field, request_with, type_text, type_text_in,
};

#[test]
fn earlier_writing_of_the_app_comes_before_the_text() {
    let mut engine = engine_with_history(
        &history_settings(HistoryAccess::On, Vec::new()),
        vec![mail_entry("Cheers,\nTom")],
    );

    let reaction = engine.on_key(Key::Char('h'), Press::First, &mail_field());

    assert_eq!(
        reaction,
        request_with(
            &[
                "Where I am typing: Format: email",
                "My earlier writing:\n“Cheers,\nTom\n”"
            ],
            "h"
        )
    );
}

#[test]
fn earlier_writing_stays_out_when_the_history_or_the_app_is_off() {
    let settings = [
        history_settings(HistoryAccess::Off, Vec::new()),
        history_settings(HistoryAccess::On, vec![mail_history_app(HistoryScope::Off)]),
    ];

    let reactions = settings.map(|settings| {
        engine_with_history(&settings, vec![mail_entry("Cheers,\nTom")]).on_key(
            Key::Char('h'),
            Press::First,
            &mail_field(),
        )
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
fn kept_text_has_the_time_when_its_session_started() {
    let mut engine = engine_with(&kept_settings(KeptTexts::All));
    let desktop = mail_desktop(GREETING).at(Timestamp(1_793_199_600));

    let saved = finish_typing(&mut engine, &desktop, Suggestion::Ignored);

    assert_eq!(
        saved,
        Some(vec![HistoryEntry {
            written_at: Some(Timestamp(1_793_199_600)),
            ..mail_entry(&format!("{EARLIER_TEXT}d"))
        }])
    );
}

#[test]
fn kept_text_has_the_typing_context_of_its_window() {
    let mut engine = engine_with(&kept_settings(KeptTexts::All));
    let mut desktop = mail_desktop(GREETING);
    desktop.surroundings.app_name = Some("Mail".to_owned());
    desktop.surroundings.window_title = Some("Re: Garden party".to_owned());

    let saved = finish_typing(&mut engine, &desktop, Suggestion::Ignored);

    assert_eq!(
        saved,
        Some(vec![HistoryEntry {
            typing_context: Some("garden party".to_owned()),
            ..mail_entry(&format!("{EARLIER_TEXT}d"))
        }])
    );
}

const CHAT_APP: &str = "net.whatsapp.WhatsApp";

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

fn read_screen(engine: &mut Engine, line: &str) {
    let id = engine.start_screen_read().expect("no session waits for a read");
    engine.on_screen_read(
        ScreenRead {
            id,
            text: Some(screen_with(line)),
        },
        &FakeDesktop::default(),
    );
}

fn chat_entry(text: &str, reply_to: &[&str]) -> HistoryEntry {
    HistoryEntry {
        app_id: CHAT_APP.to_owned(),
        reply_to: reply_to.iter().map(|&word| word.to_owned()).collect(),
        ..mail_entry(text)
    }
}

#[test]
fn kept_chat_text_has_the_words_of_the_message_that_it_answers() {
    let mut engine = engine_with(&kept_settings(KeptTexts::All));
    read_screen(&mut engine, "Standup in 5? Same room");
    let desktop = FakeDesktop::with_field_text(GREETING).in_app(CHAT_APP);

    let saved = finish_typing(&mut engine, &desktop, Suggestion::Ignored);

    assert_eq!(
        saved,
        Some(vec![chat_entry(
            &format!("{EARLIER_TEXT}d"),
            &["room", "same", "standup"]
        )])
    );
}

#[test]
fn kept_chat_text_answers_the_message_of_the_screen_read_of_its_session() {
    let mut engine = engine_with(&kept_settings(KeptTexts::All));
    read_screen(&mut engine, "Lunch at noon?");
    let desktop = FakeDesktop::with_field_text(GREETING).in_app(CHAT_APP);
    engine.on_key(Key::Char('S'), Press::First, &desktop);
    let id = engine
        .start_screen_read()
        .expect("the session has no read of its own");
    engine.on_screen_read(
        ScreenRead {
            id,
            text: Some(screen_with("Standup in 5? Same room")),
        },
        &desktop,
    );

    let saved = finish_typing(&mut engine, &desktop, Suggestion::Ignored);

    assert_eq!(
        saved,
        Some(vec![chat_entry(
            &format!("{GREETING}S{TYPED_TEXT}d"),
            &["room", "same", "standup"]
        )])
    );
}

fn reply_words_settings(reply_words: ReplyWords) -> Settings {
    let mut settings = kept_settings(KeptTexts::All);
    settings.personalization.reply_words = reply_words;
    settings
}

#[test]
fn kept_chat_text_has_no_reply_words_while_they_are_off() {
    let mut engine = engine_with(&reply_words_settings(ReplyWords::Off));
    read_screen(&mut engine, "Standup in 5? Same room");
    let desktop = FakeDesktop::with_field_text(GREETING).in_app(CHAT_APP);

    let saved = finish_typing(&mut engine, &desktop, Suggestion::Ignored);

    assert_eq!(saved, Some(vec![chat_entry(&format!("{EARLIER_TEXT}d"), &[])]));
}

#[test]
fn turning_off_the_reply_words_drops_them_from_the_earlier_writing() {
    let mut engine = engine_with_history(
        &reply_words_settings(ReplyWords::On),
        vec![chat_entry("omw, two minutes", &["standup"])],
    );

    engine.use_settings(&reply_words_settings(ReplyWords::Off));

    assert_eq!(
        engine.history_to_save(),
        Some([chat_entry("omw, two minutes", &[])].as_slice())
    );
}

#[test]
fn earlier_writing_keeps_no_reply_words_while_they_are_off() {
    let mut engine = engine_with(&reply_words_settings(ReplyWords::Off));

    engine.use_history(vec![chat_entry("omw, two minutes", &["standup"])]);

    assert_eq!(
        engine.history_to_save(),
        Some([chat_entry("omw, two minutes", &[])].as_slice())
    );
}

#[test]
fn earlier_writing_without_reply_words_needs_no_save() {
    let mut engine = engine_with(&reply_words_settings(ReplyWords::Off));

    engine.use_history(vec![chat_entry("omw, two minutes", &[])]);

    assert_eq!(engine.history_to_save(), None);
}

#[test]
fn finished_text_is_not_kept_where_the_rules_forbid_it() {
    let cases = [
        (
            history_settings(HistoryAccess::Off, Vec::new()),
            mail_desktop(GREETING),
        ),
        (
            history_settings(HistoryAccess::On, vec![mail_history_app(HistoryScope::Off)]),
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
        (kept_settings(KeptTexts::All), terminal_of_an_editor()),
        (
            kept_settings(KeptTexts::All),
            FakeDesktop::default().in_app(EDITOR_APP),
        ),
    ];

    let saved = cases.map(|(settings, desktop)| {
        finish_typing(&mut engine_with(&settings), &desktop, Suggestion::Accepted)
    });

    assert_eq!(saved, [None, None, None, None, None, None, None]);
}

#[test]
fn text_of_an_editor_that_hides_its_text_is_kept() {
    let mut engine = engine_with(&kept_settings(KeptTexts::All));

    let saved = finish_typing(&mut engine, &editor_with_hidden_text(), Suggestion::Ignored);

    assert_eq!(saved.map(|entries| entries.len()), Some(1));
}

const EDITOR_APP: &str = "com.microsoft.VSCode";

/// The field of a code editor that shows its text only on the screen, as the editor of VS Code
/// does while its screen reader mode is off.
fn editor_with_hidden_text() -> FakeDesktop {
    let mut desktop = FakeDesktop::default().in_app(EDITOR_APP);
    desktop.surroundings.has_unreadable_text = true;
    desktop
}

/// The input box of a terminal panel in a code editor, which hides the line of its caret.
fn terminal_of_an_editor() -> FakeDesktop {
    let mut desktop = editor_with_hidden_text();
    desktop.surroundings.is_caret_line_hidden = true;
    desktop
}

fn long_field() -> String {
    "The terms of the lease are in force. ".repeat(6)
}

fn mail_desktop(before_caret: &str) -> FakeDesktop {
    FakeDesktop::with_field_text(before_caret).in_app(MAIL_APP)
}

#[test]
fn finished_text_of_one_word_is_not_kept() {
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
fn message_that_the_user_types_again_is_kept_again() {
    let mut engine = engine_with(&kept_settings(KeptTexts::All));
    finish_typing(&mut engine, &mail_desktop(""), Suggestion::Ignored);

    let saved = finish_typing(&mut engine, &mail_desktop(""), Suggestion::Ignored);

    let message = mail_entry(&format!("{TYPED_TEXT}d"));
    assert_eq!(saved, Some(vec![message.clone(), message]));
}

fn press_repeatedly(engine: &mut Engine, key: Key, count: usize, desktop: &FakeDesktop) {
    for _ in 0..count {
        engine.on_key(key, Press::First, desktop);
    }
}

#[test]
fn backspace_into_the_text_of_the_field_makes_it_count_less() {
    let mut engine = engine_with(&kept_settings(KeptTexts::All));
    let desktop = mail_desktop(&long_field());
    press_repeatedly(&mut engine, Key::Backspace, 190, &desktop);

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
    engine.on_key(Key::Enter, Press::First, &mail_desktop(""));
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

const LETTER: &str = "The report is ready. Sorry for the delay";

/// Types [`LETTER`] in the mail app, then presses a shortcut that the app handles after the engine,
/// such as Option+Backspace.
fn engine_after_a_shortcut() -> Engine {
    let mut engine = engine_with(&kept_settings(KeptTexts::All));
    type_text_in(&mut engine, &mail_desktop(""), LETTER);
    engine.on_key(Key::Other, Press::First, &mail_desktop(LETTER));
    engine
}

#[test]
fn text_of_a_session_that_a_shortcut_ended_is_kept_as_far_as_its_field_still_holds_it() {
    let next_fields = [
        mail_desktop("The report is ready. Sorry for the "),
        FakeDesktop::with_text_around("The report is ready. Sorry for the de", "lay").in_app(MAIL_APP),
        FakeDesktop::with_text_around("", "").in_app(MAIL_APP),
        mail_field(),
        mail_desktop("").with_field_name("Subject"),
    ];

    let saved = next_fields.map(|next_field| {
        let mut engine = engine_after_a_shortcut();
        engine.on_key(Key::Char('x'), Press::First, &next_field);
        engine.history_to_save().map(<[HistoryEntry]>::to_vec)
    });

    assert_eq!(
        saved,
        [
            Some(vec![mail_entry("The report is ready. Sorry for the")]),
            Some(vec![mail_entry(LETTER)]),
            None,
            Some(vec![mail_entry(LETTER)]),
            Some(vec![mail_entry(LETTER)]),
        ]
    );
}

#[test]
fn text_of_a_session_waits_for_the_last_shortcut_in_its_field() {
    let mut engine = engine_after_a_shortcut();
    engine.on_key(
        Key::Other,
        Press::First,
        &mail_desktop("The report is ready. Sorry for the "),
    );
    engine.on_key(
        Key::Other,
        Press::First,
        &mail_desktop("The report is ready. Sorry for "),
    );

    engine.on_key(
        Key::Char('x'),
        Press::First,
        &mail_desktop("The report is ready. "),
    );

    assert_eq!(
        engine.history_to_save().map(<[HistoryEntry]>::to_vec),
        Some(vec![mail_entry("The report is ready.")])
    );
}

#[test]
fn text_of_a_session_that_a_shortcut_ended_is_kept_whole_at_a_click() {
    let mut engine = engine_after_a_shortcut();

    engine.reset();

    assert_eq!(
        engine.history_to_save().map(<[HistoryEntry]>::to_vec),
        Some(vec![mail_entry(LETTER)])
    );
}

#[test]
fn text_that_a_shortcut_deleted_before_the_input_settled_is_not_kept_at_a_click() {
    let mut engine = engine_after_a_shortcut();
    engine.follow_key_ended_session(&mail_desktop("The report is ready. Sorry for the "));

    engine.reset();

    assert_eq!(
        engine.history_to_save().map(<[HistoryEntry]>::to_vec),
        Some(vec![mail_entry("The report is ready. Sorry for the")])
    );
}
