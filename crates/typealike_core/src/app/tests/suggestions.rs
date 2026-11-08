//! Tests of the suggestions, the prompt preparation and the pause of the app.

use super::fixtures::{
    FakeDesktop, MAIL, app_preparing_on_first_key, app_preparing_on_focus, app_showing_complete_suggestions,
    request, type_key,
};
use crate::app::{App, SettledWork, TrayCommand};
use crate::desktop::{CopiedText, Desktop, Field, FieldText, ScreenRead, Surroundings};
use crate::engine::Reaction;
use crate::history::HistoryEntry;
use crate::inference::{InferenceError, InferenceRequest, NextWordBias};
use crate::key::{Key, Press};
use crate::prompt::prompt_with_notes;
use crate::settings::{
    HistoryAccess, KeptTexts, PersonalizationSettings, PromptPreparation, ScreenTextAccess,
    ScreenTextSettings, Settings, SuggestionDisplay, WriterNotes,
};
use crate::test_support::{completion, preparation_request};

/// A field of an unknown app with this text before the caret.
struct EditedField(&'static str);

impl Desktop for EditedField {
    fn field(&self) -> Field {
        Field::Open
    }

    fn surroundings(&self) -> Surroundings {
        Surroundings {
            field_text: Some(FieldText {
                before_caret: self.0.to_owned(),
                after_caret: String::new(),
            }),
            ..Surroundings::default()
        }
    }

    fn copied_text(&self) -> Option<CopiedText> {
        None
    }
}

fn app_showing(suggestion: &str) -> App {
    let mut app = App::new();
    type_key(&mut app, 'B');
    app.on_completion(completion(suggestion));
    app
}

#[test]
fn active_app_requests_completion() {
    let mut app = App::new();

    let reaction = type_key(&mut app, 'B');

    assert_eq!(reaction, request("B"));
}

#[test]
fn completion_shows_in_active_app() {
    let mut app = App::new();
    type_key(&mut app, 'B');

    let reaction = app.on_completion(completion("est regards"));

    assert_eq!(reaction, Reaction::Show("est regards".to_owned()));
}

fn app_showing_words_early() -> App {
    let mut app = App::new();
    app.use_settings(&Settings {
        suggestion_display: SuggestionDisplay::WordByWord,
        ..Settings::default()
    });
    app
}

#[test]
fn partial_completion_shows_when_words_show_early() {
    let mut app = app_showing_words_early();
    type_key(&mut app, 'B');

    let reaction = app.on_partial_completion("est regards");

    assert_eq!(reaction, Some(Reaction::Show("est".to_owned())));
}

#[test]
fn partial_completion_shows_nothing_when_suggestions_show_complete() {
    let mut app = app_showing_complete_suggestions();
    type_key(&mut app, 'B');

    let reaction = app.on_partial_completion("est regards");

    assert_eq!(reaction, None);
}

#[test]
fn completion_after_a_partial_completion_that_shows_nothing_shows_all_its_text() {
    let mut app = app_showing_complete_suggestions();
    type_key(&mut app, 'B');
    app.on_partial_completion("est regards");

    let reaction = app.on_completion(completion("est regards"));

    assert_eq!(reaction, Reaction::Show("est regards".to_owned()));
}

#[test]
fn key_that_a_partial_completion_added_keeps_the_request_when_suggestions_show_complete() {
    let mut app = app_showing_complete_suggestions();
    type_key(&mut app, 'B');
    app.on_partial_completion("est regards");

    let reaction = type_key(&mut app, 'e');

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn excluded_field_gets_no_suggestion() {
    let mut app = App::new();

    let reaction = app.on_key(Key::Char('B'), Press::First, &FakeDesktop(Field::Excluded));

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn excluded_field_forgets_typed_context() {
    let mut app = app_showing("est");
    app.on_key(Key::Char('x'), Press::First, &FakeDesktop(Field::Excluded));

    let reaction = type_key(&mut app, 'r');

    assert_eq!(reaction, request("r"));
}

#[test]
fn settings_give_the_writer_notes() {
    let mut app = App::new();
    app.use_settings(&Settings {
        writer_notes: WriterNotes("I am a nurse.".to_owned()),
        ..Settings::default()
    });

    let reaction = type_key(&mut app, 'B');

    assert_eq!(
        reaction,
        Reaction::Request(InferenceRequest::Complete {
            prompt: prompt_with_notes("I am a nurse.", &[], "B"),
            max_tokens: Settings::default().suggestion_length_in_use().max_tokens(),
            word_bias: None,
            next_word_bias: NextWordBias::default(),
            min_log_probability: Some(Settings::default().suggestion_length_in_use().tail_cut()),
            typo_check: Settings::default().typo_check(),
            sentence_end: Settings::default().sentence_end,
            line_after: String::new(),
        })
    );
}

#[test]
fn settled_input_prepares_the_open_field() {
    let mut app = app_preparing_on_focus();

    let request = app.on_input_settled(&FakeDesktop(Field::Open));

    assert_eq!(request, Some(SettledWork::Prepare(preparation_request(&[], ""))));
}

#[test]
fn settled_input_prepares_nothing_when_preparation_waits_for_the_first_key() {
    let mut app = app_preparing_on_first_key();

    let request = app.on_input_settled(&FakeDesktop(Field::Open));

    assert_eq!(request, None);
}

#[test]
fn settled_input_prepares_no_excluded_field() {
    let mut app = app_preparing_on_focus();

    let request = app.on_input_settled(&FakeDesktop(Field::Excluded));

    assert_eq!(request, None);
}

#[test]
fn paused_app_prepares_nothing() {
    let mut app = app_preparing_on_focus();
    app.on_tray_command(TrayCommand::TogglePause);

    let request = app.on_input_settled(&FakeDesktop(Field::Open));

    assert_eq!(request, None);
}

#[test]
fn settled_input_after_enter_suggests_on_the_new_line() {
    let mut app = App::new();
    app.on_key(Key::Char(','), Press::First, &EditedField("Best regards"));
    app.on_key(Key::Enter, Press::First, &EditedField("Best regards,"));

    let work = app.on_input_settled(&EditedField("Best regards,\n"));

    assert_eq!(work, Some(SettledWork::Suggest(request("Best regards,\n"))));
}

#[test]
fn settled_input_takes_the_word_that_the_app_corrected() {
    let mut app = App::new();
    for character in "recieve ".chars() {
        app.on_key(Key::Char(character), Press::First, &EditedField("I "));
    }
    app.on_input_settled(&EditedField("I receive "));

    let reaction = app.on_key(Key::Char('t'), Press::First, &EditedField("I receive "));

    assert_eq!(reaction, request("I receive t"));
}

#[test]
fn settled_input_after_enter_that_emptied_the_field_prepares_a_new_session() {
    let mut app = app_preparing_on_focus();
    app.on_key(Key::Char('k'), Press::First, &EditedField("o"));
    app.on_key(Key::Enter, Press::First, &EditedField("ok"));

    let work = app.on_input_settled(&EditedField(""));

    assert_eq!(work, Some(SettledWork::Prepare(preparation_request(&[], ""))));
}

#[test]
fn context_lost_hides_suggestion() {
    let mut app = app_showing("est");

    let reaction = app.on_context_lost();

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn paused_app_does_not_suggest() {
    let mut app = App::new();
    app.on_tray_command(TrayCommand::TogglePause);

    let reaction = type_key(&mut app, 'B');

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn paused_app_lets_tab_through() {
    let mut app = app_showing("est");
    app.on_tray_command(TrayCommand::TogglePause);

    let reaction = app.on_key(Key::Tab, Press::First, &FakeDesktop(Field::Open));

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn paused_app_drops_completion() {
    let mut app = App::new();
    type_key(&mut app, 'B');
    app.on_tray_command(TrayCommand::TogglePause);

    let reaction = app.on_completion(completion("est"));

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn paused_app_drops_partial_completion() {
    let mut app = app_showing_words_early();
    type_key(&mut app, 'B');
    app.on_tray_command(TrayCommand::TogglePause);

    let reaction = app.on_partial_completion("est regards");

    assert_eq!(reaction, None);
}

#[test]
fn resumed_app_suggests_again() {
    let mut app = App::new();
    app.on_tray_command(TrayCommand::TogglePause);
    app.on_tray_command(TrayCommand::TogglePause);

    let reaction = type_key(&mut app, 'B');

    assert_eq!(reaction, request("B"));
}

#[test]
fn pausing_forgets_typed_context() {
    let mut app = app_showing("est");
    app.on_tray_command(TrayCommand::TogglePause);
    app.on_tray_command(TrayCommand::TogglePause);

    let reaction = type_key(&mut app, 'r');

    assert_eq!(reaction, request("r"));
}

fn app_keeping_all_texts() -> App {
    let mut app = App::new();
    app.use_settings(&Settings {
        personalization: PersonalizationSettings {
            kept_texts: KeptTexts::All,
            ..PersonalizationSettings::default()
        },
        ..Settings::default()
    });
    app
}

fn paused(mut app: App) -> App {
    app.on_tray_command(TrayCommand::TogglePause);
    app
}

#[test]
fn typing_while_paused_keeps_no_earlier_writing() {
    let apps = [app_keeping_all_texts(), paused(app_keeping_all_texts())];

    let kept = apps.map(|mut app| {
        "Hello Ana, the report is ready"
            .chars()
            .for_each(|character| drop(app.on_key(Key::Char(character), Press::First, &MAIL)));
        app.on_context_lost();
        app.history_to_save().map(<[_]>::len)
    });

    assert_eq!(kept, [Some(1), None]);
}

/// A field of the mail app with this text before the caret.
struct MailDraft(&'static str);

impl Desktop for MailDraft {
    fn field(&self) -> Field {
        Field::Open
    }

    fn surroundings(&self) -> Surroundings {
        Surroundings {
            app_id: Some("com.apple.mail".to_owned()),
            field_text: Some(FieldText {
                before_caret: self.0.to_owned(),
                after_caret: String::new(),
            }),
            ..Surroundings::default()
        }
    }

    fn copied_text(&self) -> Option<CopiedText> {
        None
    }
}

const BLUEFIN_MAIL: &str = "Dear Ana, the Bluefin budget review moves to Monday.";

fn app_with_four_mails() -> App {
    app_with_four_mails_and(ScreenTextAccess::Denied)
}

fn app_with_four_mails_and(screen_text: ScreenTextAccess) -> App {
    let mut app = App::new();
    app.use_settings(&Settings {
        personalization: PersonalizationSettings {
            history: HistoryAccess::On,
            ..PersonalizationSettings::default()
        },
        prompt_preparation: PromptPreparation::OnFocus,
        screen_text: ScreenTextSettings {
            access: screen_text,
            ..ScreenTextSettings::default()
        },
        ..Settings::default()
    });
    let texts = [
        BLUEFIN_MAIL,
        "Dear Ana, the garden party starts at noon.",
        "Dear Ana, the dentist moved my visit.",
        "Dear Ana, the train leaves at eight.",
    ];
    app.use_history(texts.map(mail_entry).to_vec());
    app
}

fn mail_entry(text: &str) -> HistoryEntry {
    HistoryEntry {
        text: text.to_owned(),
        app_id: "com.apple.mail".to_owned(),
        domain: None,
        written_at: None,
        reply_to: Vec::new(),
        typing_context: None,
    }
}

/// Types `draft` in the mail app, where a newer key replaces the request of each key.
fn type_draft(app: &mut App, draft: &'static str) {
    for (index, character) in draft.char_indices() {
        app.on_key(Key::Char(character), Press::First, &MailDraft(&draft[..index]));
        app.on_completion(Err(InferenceError::Superseded));
    }
}

#[test]
fn settle_after_a_new_block_of_typed_words_prepares_the_prompt_with_the_matching_examples() {
    let mut app = app_with_four_mails();
    let draft = "Hi Ana, a short update on the plan for the Bluefin budget ";
    type_draft(&mut app, draft);

    let work = app.on_input_settled(&MailDraft(draft));

    assert_eq!(
        work,
        Some(SettledWork::Prepare(preparation_request(
            &[
                "Where I am typing: Format: email",
                &format!("My earlier writing:\n“{BLUEFIN_MAIL}\n”")
            ],
            draft
        )))
    );
}

#[test]
fn settle_in_the_same_block_of_typed_words_prepares_nothing() {
    let mut app = app_with_four_mails();
    type_draft(&mut app, "Hi Ana, a ");

    let work = app.on_input_settled(&MailDraft("Hi Ana, a "));

    assert_eq!(work, None);
}

#[test]
fn screen_read_of_a_settle_after_a_new_block_of_typed_words_prepares_the_prompt_with_the_matching_examples() {
    let mut app = app_with_four_mails_and(ScreenTextAccess::Allowed);
    let draft = "Hi Ana, a short update on the plan for the Bluefin budget ";
    type_draft(&mut app, draft);
    let Some(SettledWork::ReadScreen { read, .. }) = app.on_input_settled(&MailDraft(draft)) else {
        panic!("a settle in a mail field reads the screen");
    };

    let work = app.on_screen_read(ScreenRead { id: read, text: None }, &MailDraft(draft));

    assert_eq!(
        work,
        Some(SettledWork::Prepare(preparation_request(
            &[
                "Where I am typing: Format: email",
                &format!("My earlier writing:\n“{BLUEFIN_MAIL}\n”")
            ],
            draft
        )))
    );
}
