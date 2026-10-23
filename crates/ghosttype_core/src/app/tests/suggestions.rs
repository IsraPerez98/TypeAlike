//! Tests of the suggestions, the prompt preparation and the pause of the app.

use super::fixtures::{FakeDesktop, app_preparing_on_focus, request, type_key};
use crate::app::{App, SettledWork, TrayCommand};
use crate::desktop::{CopiedText, Desktop, Field, FieldText, Surroundings};
use crate::engine::Reaction;
use crate::inference::{InferenceRequest, NextWordBias};
use crate::key::Key;
use crate::prompt::prompt_with_notes;
use crate::settings::{Settings, SuggestionDisplay, WriterNotes};
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
    let mut app = App::new();
    type_key(&mut app, 'B');

    let reaction = app.on_partial_completion("est regards");

    assert_eq!(reaction, None);
}

#[test]
fn completion_after_a_partial_completion_that_shows_nothing_shows_all_its_text() {
    let mut app = App::new();
    type_key(&mut app, 'B');
    app.on_partial_completion("est regards");

    let reaction = app.on_completion(completion("est regards"));

    assert_eq!(reaction, Reaction::Show("est regards".to_owned()));
}

#[test]
fn key_that_a_partial_completion_added_keeps_the_request_when_suggestions_show_complete() {
    let mut app = App::new();
    type_key(&mut app, 'B');
    app.on_partial_completion("est regards");

    let reaction = type_key(&mut app, 'e');

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn excluded_field_gets_no_suggestion() {
    let mut app = App::new();

    let reaction = app.on_key(Key::Char('B'), &FakeDesktop(Field::Excluded));

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn excluded_field_forgets_typed_context() {
    let mut app = app_showing("est");
    app.on_key(Key::Char('x'), &FakeDesktop(Field::Excluded));

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
    let mut app = App::new();

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
    app.on_key(Key::Char(','), &EditedField("Best regards"));
    app.on_key(Key::Enter, &EditedField("Best regards,"));

    let work = app.on_input_settled(&EditedField("Best regards,\n"));

    assert_eq!(work, Some(SettledWork::Suggest(request("Best regards,\n"))));
}

#[test]
fn settled_input_after_enter_that_emptied_the_field_prepares_a_new_session() {
    let mut app = app_preparing_on_focus();
    app.on_key(Key::Char('k'), &EditedField("o"));
    app.on_key(Key::Enter, &EditedField("ok"));

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

    let reaction = app.on_key(Key::Tab, &FakeDesktop(Field::Open));

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
