//! Tests of the screen reads that the controller starts.

use super::harness::{Call, Harness};
use crate::controller::{Delivery, SCROLL_SETTLE_DELAY, Timer};
use crate::desktop::{
    FieldLabels, FieldText, OcrAvailability, ScreenRead, ScreenReadId, ScreenSource, ScreenText, Surroundings,
};
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

/// Scrolls in the focused field, stops the scroll, and returns the reads of the screen that the
/// stop started.
fn scroll_stop_reads(harness: &mut Harness) -> Vec<(ScreenReadId, ScreenSource)> {
    harness.scroll();
    harness.take_calls();
    harness
        .controller
        .on_timer(Timer::ScrollSettle, &mut harness.shell);
    harness
        .take_calls()
        .into_iter()
        .filter_map(|call| match call {
            Call::ReadScreen(read, source) => Some((read, source)),
            _ => None,
        })
        .collect()
}

#[test]
fn scroll_starts_the_timer_of_its_stop_again() {
    let mut harness = Harness::new(Settings::default());
    harness.focus_text_field_of("com.tinyspeck.slackmacgap", "Slack");
    harness.scroll();
    harness.take_calls();

    harness.scroll();

    assert_eq!(
        harness.take_calls(),
        vec![Call::StartTimer(Timer::ScrollSettle, SCROLL_SETTLE_DELAY)]
    );
}

#[test]
fn stop_of_a_scroll_in_a_chat_or_an_email_app_reads_the_tree_once() {
    let places = [("com.tinyspeck.slackmacgap", "Slack"), ("com.apple.mail", "Mail")];

    let sources = places.map(|(app_id, app_name)| {
        let mut harness = Harness::new(Settings::default());
        harness.focus_text_field_of(app_id, app_name);
        scroll_stop_reads(&mut harness)
            .into_iter()
            .map(|(_, source)| source)
            .collect::<Vec<_>>()
    });

    assert_eq!(
        sources,
        [[ScreenSource::Accessibility], [ScreenSource::Accessibility]]
    );
}

#[test]
fn stop_of_a_scroll_reads_no_screen_in_another_place_or_where_the_screen_text_is_off() {
    let mail_off = Settings {
        screen_text: ScreenTextSettings {
            apps: vec![mail_app(false)],
            ..ScreenTextSettings::default()
        },
        ..Settings::default()
    };
    let places = [
        (Settings::default(), "com.apple.TextEdit", "TextEdit"),
        (mail_off, "com.apple.mail", "Mail"),
    ];

    let reads = places.map(|(settings, app_id, app_name)| {
        let mut harness = Harness::new(settings);
        harness.focus_text_field_of(app_id, app_name);
        scroll_stop_reads(&mut harness)
    });

    assert_eq!(reads, [Vec::new(), Vec::new()]);
}

#[test]
fn key_after_the_read_of_a_scroll_stop_reads_the_screen_no_more() {
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
    let [(stop_read, _)] = scroll_stop_reads(&mut harness)[..] else {
        panic!("the stop of the scroll reads the screen");
    };
    harness.controller.on_delivery(
        Delivery::Screen(ScreenRead {
            id: stop_read,
            text: Some(ScreenText::default()),
        }),
        &mut harness.shell,
    );

    let read_after_stop = key_read(&mut harness, Key::Char('i'));

    assert_eq!(read_after_stop, None);
}

/// The fields of Mail that get no suggestions: the address field and a search field.
fn mail_fields_without_suggestions() -> [Surroundings; 2] {
    let mail = Surroundings {
        app_id: Some("com.apple.mail".to_owned()),
        app_name: Some("Mail".to_owned()),
        field_text: Some(FieldText::default()),
        ..Surroundings::default()
    };
    [
        Surroundings {
            field_labels: FieldLabels {
                caption: Some("To:".to_owned()),
                ..FieldLabels::default()
            },
            ..mail.clone()
        },
        Surroundings {
            is_search_field: true,
            ..mail
        },
    ]
}

#[test]
fn settled_input_reads_no_screen_in_a_field_without_suggestions() {
    let reads = mail_fields_without_suggestions().map(|surroundings| {
        let mut harness = Harness::new(Settings::default());
        harness.shell.surroundings = surroundings;
        harness.settle_input();
        harness
            .take_calls()
            .into_iter()
            .filter(|call| matches!(call, Call::ReadScreen(..)))
            .count()
    });

    assert_eq!(reads, [0, 0]);
}

#[test]
fn first_key_reads_no_screen_in_a_field_without_suggestions() {
    let reads = mail_fields_without_suggestions().map(|surroundings| {
        let mut harness = Harness::new(Settings::default());
        harness.shell.surroundings = surroundings;
        key_read(&mut harness, Key::Char('H'))
    });

    assert_eq!(reads, [None, None]);
}

#[test]
fn stop_of_a_scroll_reads_no_screen_in_a_field_without_suggestions() {
    let reads = mail_fields_without_suggestions().map(|surroundings| {
        let mut harness = Harness::new(Settings::default());
        harness.shell.surroundings = surroundings;
        scroll_stop_reads(&mut harness)
    });

    assert_eq!(reads, [Vec::new(), Vec::new()]);
}
