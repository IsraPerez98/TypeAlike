//! Tests of the screen reads that the app starts and the answers that it uses.

use super::fixtures::{FakeDesktop, MAIL, TextField, app_preparing_on_first_key, app_preparing_on_focus};
use crate::app::{App, SettledWork};
use crate::desktop::{
    Confidence, Field, OcrAvailability, ScreenLine, ScreenRead, ScreenReadId, ScreenRect, ScreenSource,
    ScreenText,
};
use crate::history::HistoryEntry;
use crate::key::Key;
use crate::settings::{
    HistoryAccess, OcrLevel, PersonalizationSettings, PromptPreparation, ScreenTextAccess,
    ScreenTextSettings, Settings,
};
use crate::test_support::{completion, mail_app, preparation_request};

const MAIL_WITH_OCR: TextField = TextField {
    ocr: OcrAvailability::Ready,
    ..MAIL
};

fn screen_read(app: &mut App) -> ScreenReadId {
    match app.on_input_settled(&MAIL) {
        Some(SettledWork::ReadScreen {
            read,
            source: ScreenSource::Accessibility,
        }) => read,
        work => panic!("a settled text field reads the screen: {work:?}"),
    }
}

/// Starts the read of the screen for the session of the last key, and returns its id.
fn session_read(app: &mut App) -> ScreenReadId {
    match app.start_session_screen_read() {
        Some(SettledWork::ReadScreen { read, .. }) => read,
        work => panic!("the first key reads the screen: {work:?}"),
    }
}

/// Answers `read` without lines in a field where the OCR can run, and returns the id of the OCR
/// read that the answer starts.
fn ocr_read(app: &mut App, read: ScreenReadId) -> ScreenReadId {
    match app.on_screen_read(answer(read), &MAIL_WITH_OCR) {
        Some(SettledWork::ReadScreen { read: ocr, .. }) => ocr,
        work => panic!("an answer without lines starts an OCR read: {work:?}"),
    }
}

fn answer(read: ScreenReadId) -> ScreenRead {
    ScreenRead {
        id: read,
        text: Some(ScreenText::default()),
    }
}

#[test]
fn settled_text_field_reads_the_screen_with_each_preparation() {
    let mut first_key = App::new();
    let mut on_focus = app_preparing_on_focus();

    let works = [&mut first_key, &mut on_focus].map(|app| app.on_input_settled(&MAIL));

    assert!(
        works
            .iter()
            .all(|work| matches!(work, Some(SettledWork::ReadScreen { .. }))),
        "{works:?}"
    );
}

#[test]
fn settled_field_without_text_reads_no_screen() {
    let mut app = app_preparing_on_first_key();

    let work = app.on_input_settled(&FakeDesktop(Field::Open));

    assert_eq!(work, None);
}

#[test]
fn settled_password_field_reads_no_screen() {
    let mut app = App::new();

    let work = app.on_input_settled(&TextField {
        field: Field::Excluded,
        ..MAIL
    });

    assert_eq!(work, None);
}

#[test]
fn settled_field_of_a_password_manager_reads_no_screen() {
    let mut app = app_preparing_on_first_key();

    let work = app.on_input_settled(&TextField {
        app_id: "com.1password.1password",
        ..MAIL
    });

    assert_eq!(work, None);
}

#[test]
fn screen_text_off_prepares_on_focus_at_once() {
    let mut app = App::new();
    app.use_settings(&Settings {
        prompt_preparation: PromptPreparation::OnFocus,
        screen_text: ScreenTextSettings {
            access: ScreenTextAccess::Denied,
            ..ScreenTextSettings::default()
        },
        ..Settings::default()
    });

    let work = app.on_input_settled(&MAIL);

    assert_eq!(
        work,
        Some(SettledWork::Prepare(preparation_request(
            &["Where I am typing: Format: email"],
            ""
        )))
    );
}

fn answer_with(read: ScreenReadId, line: &str) -> ScreenRead {
    ScreenRead {
        id: read,
        text: Some(ScreenText {
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
        }),
    }
}

#[test]
fn settled_input_during_a_session_without_screen_text_reads_the_screen() {
    let mut app = App::new();
    app.on_key(Key::Char('H'), &MAIL);

    let work = app.on_input_settled(&MAIL);

    assert!(matches!(work, Some(SettledWork::ReadScreen { .. })), "{work:?}");
}

#[test]
fn settled_input_during_a_session_with_its_screen_text_reads_no_screen() {
    let mut app = App::new();
    app.on_key(Key::Char('H'), &MAIL);
    let read = session_read(&mut app);
    app.on_screen_read(answer_with(read, "Dinner at 7?"), &MAIL);

    let work = app.on_input_settled(&MAIL);

    assert_eq!(work, None);
}

#[test]
fn first_key_starts_a_read_when_the_session_uses_stored_text() {
    let mut app = App::new();
    let read = screen_read(&mut app);
    app.on_screen_read(answer_with(read, "Dinner at 7?"), &MAIL);
    app.on_key(Key::Char('H'), &MAIL);

    let work = app.start_session_screen_read();

    assert!(
        matches!(
            work,
            Some(SettledWork::ReadScreen {
                source: ScreenSource::Accessibility,
                ..
            })
        ),
        "{work:?}"
    );
}

#[test]
fn session_without_stored_text_reads_at_the_first_key() {
    let mut app = App::new();
    app.on_key(Key::Char('H'), &MAIL);

    let work = app.start_session_screen_read();

    assert!(
        matches!(
            work,
            Some(SettledWork::ReadScreen {
                source: ScreenSource::Accessibility,
                ..
            })
        ),
        "{work:?}"
    );
}

#[test]
fn no_read_starts_at_the_first_key_in_an_app_that_is_off() {
    let mut app = App::new();
    app.use_settings(&Settings {
        screen_text: ScreenTextSettings {
            apps: vec![mail_app(false)],
            ..ScreenTextSettings::default()
        },
        ..Settings::default()
    });
    app.on_key(Key::Char('H'), &MAIL);

    let work = app.start_session_screen_read();

    assert_eq!(work, None);
}

#[test]
fn no_read_starts_at_the_first_key_in_a_field_without_readable_text() {
    let mut app = App::new();
    app.on_key(
        Key::Char('H'),
        &TextField {
            has_readable_text: false,
            ..MAIL
        },
    );

    let work = app.start_session_screen_read();

    assert_eq!(work, None);
}

#[test]
fn no_read_starts_without_a_session() {
    let mut app = App::new();

    let work = app.start_session_screen_read();

    assert_eq!(work, None);
}

#[test]
fn settled_input_during_a_session_prepares_nothing() {
    let mut app = App::new();
    app.use_settings(&Settings {
        prompt_preparation: PromptPreparation::OnFocus,
        screen_text: ScreenTextSettings {
            access: ScreenTextAccess::Denied,
            ..ScreenTextSettings::default()
        },
        ..Settings::default()
    });
    app.on_key(Key::Char('H'), &MAIL);
    app.on_completion(completion(""));

    let work = app.on_input_settled(&MAIL);

    assert_eq!(work, None);
}

#[test]
fn answer_during_a_session_prepares_its_prompt_with_the_screen_text() {
    let mut app = app_preparing_on_focus();
    app.on_key(Key::Char('H'), &MAIL);
    app.on_completion(completion(""));
    let read = screen_read(&mut app);

    let request = app.on_screen_read(answer_with(read, "Dinner at 7?"), &MAIL);

    assert_eq!(
        request,
        Some(SettledWork::Prepare(preparation_request(
            &[
                "Where I am typing: Format: email",
                "Above the field on my screen: “Dinner at 7?”"
            ],
            "H"
        )))
    );
}

#[test]
fn answer_during_a_running_completion_prepares_nothing() {
    let mut app = app_preparing_on_focus();
    app.on_key(Key::Char('H'), &MAIL);
    let read = screen_read(&mut app);

    let request = app.on_screen_read(answer_with(read, "Dinner at 7?"), &MAIL);

    assert_eq!(request, None);
}

#[test]
fn answer_to_the_screen_read_prepares_on_focus() {
    let mut app = app_preparing_on_focus();
    let read = screen_read(&mut app);

    let request = app.on_screen_read(answer(read), &MAIL);

    assert_eq!(
        request,
        Some(SettledWork::Prepare(preparation_request(
            &["Where I am typing: Format: email"],
            ""
        )))
    );
}

#[test]
fn answer_to_the_screen_read_prepares_nothing_before_the_first_key() {
    let mut app = app_preparing_on_first_key();
    let read = screen_read(&mut app);

    let request = app.on_screen_read(answer(read), &MAIL);

    assert_eq!(request, None);
}

fn app_with_earlier_writing(history: HistoryAccess, app_id: &str) -> App {
    let mut app = App::new();
    app.use_settings(&Settings {
        personalization: PersonalizationSettings {
            history,
            ..PersonalizationSettings::default()
        },
        prompt_preparation: PromptPreparation::OnFirstKey,
        ..Settings::default()
    });
    app.use_history(vec![HistoryEntry {
        text: "Dear Ana, the report is ready.".to_owned(),
        app_id: app_id.to_owned(),
        domain: None,
        written_at: None,
    }]);
    app
}

#[test]
fn answer_to_the_screen_read_prepares_a_place_with_earlier_writing_before_the_first_key() {
    let mut app = app_with_earlier_writing(HistoryAccess::On, "com.apple.mail");
    let read = screen_read(&mut app);

    let request = app.on_screen_read(answer(read), &MAIL);

    assert_eq!(
        request,
        Some(SettledWork::Prepare(preparation_request(
            &[
                "Where I am typing: Format: email",
                "My earlier writing:\n“Dear Ana, the report is ready.”"
            ],
            ""
        )))
    );
}

#[test]
fn answer_to_the_screen_read_prepares_nothing_in_a_place_without_earlier_writing() {
    let mut app = app_with_earlier_writing(HistoryAccess::On, "com.apple.Notes");
    let read = screen_read(&mut app);

    let request = app.on_screen_read(answer(read), &MAIL);

    assert_eq!(request, None);
}

#[test]
fn earlier_writing_that_is_off_prepares_nothing_before_the_first_key() {
    let mut app = app_with_earlier_writing(HistoryAccess::Off, "com.apple.mail");
    let read = screen_read(&mut app);

    let request = app.on_screen_read(answer(read), &MAIL);

    assert_eq!(request, None);
}

#[test]
fn answer_after_a_lost_context_is_dropped() {
    let mut app = app_preparing_on_focus();
    let read = screen_read(&mut app);
    app.on_context_lost();

    let request = app.on_screen_read(answer(read), &MAIL);

    assert_eq!(request, None);
}

#[test]
fn answer_without_lines_starts_an_ocr_read() {
    let mut app = App::new();
    let read = screen_read(&mut app);

    let work = app.on_screen_read(answer(read), &MAIL_WITH_OCR);

    assert!(
        matches!(
            work,
            Some(SettledWork::ReadScreen {
                source: ScreenSource::Ocr(_),
                ..
            })
        ),
        "{work:?}"
    );
}

#[test]
fn ocr_read_uses_the_level_of_the_settings() {
    let mut app = App::new();
    app.use_settings(&Settings {
        screen_text: ScreenTextSettings {
            ocr_level: OcrLevel::Accurate,
            ..ScreenTextSettings::default()
        },
        ..Settings::default()
    });
    let read = screen_read(&mut app);

    let work = app.on_screen_read(answer(read), &MAIL_WITH_OCR);

    assert!(
        matches!(
            work,
            Some(SettledWork::ReadScreen {
                source: ScreenSource::Ocr(OcrLevel::Accurate),
                ..
            })
        ),
        "{work:?}"
    );
}

#[test]
fn answer_with_lines_starts_no_ocr_read() {
    let mut app = app_preparing_on_first_key();
    let read = screen_read(&mut app);

    let work = app.on_screen_read(answer_with(read, "Dinner at 7?"), &MAIL_WITH_OCR);

    assert_eq!(work, None);
}

#[test]
fn answer_without_lines_starts_no_ocr_read_when_the_ocr_cannot_run() {
    let mut app = app_preparing_on_first_key();
    let read = screen_read(&mut app);

    let work = app.on_screen_read(
        answer(read),
        &TextField {
            ocr: OcrAvailability::NotPermitted,
            ..MAIL
        },
    );

    assert_eq!(work, None);
}

#[test]
fn answer_of_the_ocr_without_lines_starts_no_other_read() {
    let mut app = app_preparing_on_first_key();
    let read = screen_read(&mut app);
    let ocr = ocr_read(&mut app, read);

    let work = app.on_screen_read(answer(ocr), &MAIL_WITH_OCR);

    assert_eq!(work, None);
}

#[test]
fn answer_of_the_ocr_goes_into_the_prompt() {
    let mut app = app_preparing_on_focus();
    let read = screen_read(&mut app);
    let ocr = ocr_read(&mut app, read);

    let work = app.on_screen_read(answer_with(ocr, "Dinner at 7?"), &MAIL_WITH_OCR);

    assert_eq!(
        work,
        Some(SettledWork::Prepare(preparation_request(
            &[
                "Where I am typing: Format: email",
                "Above the field on my screen: “Dinner at 7?”"
            ],
            ""
        )))
    );
}

#[test]
fn typing_names_the_typed_app() {
    let mut app = App::new();

    app.on_key(Key::Char('H'), &MAIL);

    assert_eq!(
        app.typed_app().map(|typed| typed.app_id.as_str()),
        Some("com.apple.mail")
    );
}
