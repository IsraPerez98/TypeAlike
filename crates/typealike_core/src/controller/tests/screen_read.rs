//! Tests of the screen reads that the controller starts.

use super::harness::{Call, Harness};
use crate::controller::Delivery;
use crate::desktop::{OcrAvailability, ScreenRead, ScreenReadId, ScreenSource, ScreenText};
use crate::key::Key;
use crate::settings::{PromptPreparation, ScreenTextSettings, Settings};
use crate::settings_window::view::{ControlId, SettingsInput};
use crate::test_support::{mail_app, preparation_request};

#[test]
fn settled_input_in_a_text_field_reads_the_screen() {
    let mut harness = Harness::new(Settings::default());
    harness.focus_text_field_of("com.apple.mail", "Mail");

    harness.settle_input();

    assert!(matches!(harness.take_calls()[..], [Call::ReadScreen(..)]));
}

#[test]
fn first_key_in_a_text_field_reads_the_screen() {
    let mut harness = Harness::new(Settings::default());
    harness.focus_text_field_of("com.apple.mail", "Mail");

    harness.press(Key::Char('H'));

    let calls = harness.take_calls();
    assert!(
        calls
            .iter()
            .any(|call| matches!(call, Call::ReadScreen(_, ScreenSource::Accessibility))),
        "{calls:?}"
    );
}

#[test]
fn screen_answer_without_lines_reads_the_screen_with_ocr() {
    let mut harness = Harness::new(Settings::default());
    harness.shell.ocr = OcrAvailability::Ready;
    harness.focus_text_field_of("com.apple.mail", "Mail");
    let read = harness.settle_input_to_read_screen();

    harness.controller.on_delivery(
        Delivery::Screen(ScreenRead { id: read, text: None }),
        &mut harness.shell,
    );

    assert!(matches!(
        harness.take_calls()[..],
        [Call::ReadScreen(_, ScreenSource::Ocr(_))]
    ));
}

#[test]
fn screen_answer_prepares_the_prompt_on_focus() {
    let mut harness = Harness::new(Settings {
        prompt_preparation: PromptPreparation::OnFocus,
        ..Settings::default()
    });
    harness.focus_text_field_of("com.apple.mail", "Mail");
    let read = harness.settle_input_to_read_screen();

    harness.controller.on_delivery(
        Delivery::Screen(ScreenRead {
            id: read,
            text: Some(ScreenText::default()),
        }),
        &mut harness.shell,
    );

    assert_eq!(
        harness.last_request(),
        Some(preparation_request(
            &["Where I am typing: Format: email; App: Mail"],
            ""
        ))
    );
}

#[test]
fn app_switched_off_reads_no_screen() {
    let mut harness = Harness::new(Settings {
        screen_text: ScreenTextSettings {
            apps: vec![mail_app(true)],
            ..ScreenTextSettings::default()
        },
        ..Settings::default()
    });
    harness.focus_text_field_of("com.apple.mail", "Mail");
    let input = SettingsInput::Toggled {
        id: ControlId::from("screen-text-app:com.apple.mail".to_owned()),
        is_on: false,
    };
    harness.controller.on_settings_input(&input, &mut harness.shell);
    harness.take_calls();

    harness.settle_input();

    assert_eq!(harness.take_calls(), Vec::new());
}

/// Presses `key` and returns the read of the screen that the key started, if any.
fn key_read(harness: &mut Harness, key: Key) -> Option<ScreenReadId> {
    harness.press(key);
    harness.take_calls().into_iter().find_map(|call| match call {
        Call::ReadScreen(read, _) => Some(read),
        _ => None,
    })
}

#[test]
fn key_after_a_scroll_reads_the_screen_again() {
    let mut harness = Harness::new(Settings::default());
    harness.focus_text_field_of("com.apple.mail", "Mail");
    let read = key_read(&mut harness, Key::Char('H')).expect("the first key reads the screen");
    harness.controller.on_delivery(
        Delivery::Screen(ScreenRead {
            id: read,
            text: Some(ScreenText::default()),
        }),
        &mut harness.shell,
    );
    let read_without_scroll = key_read(&mut harness, Key::Char('i'));

    harness.scroll();
    let read_after_scroll = key_read(&mut harness, Key::Char('!'));

    assert_eq!(read_without_scroll, None);
    assert!(read_after_scroll.is_some());
}
