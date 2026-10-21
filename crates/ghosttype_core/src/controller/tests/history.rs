//! Tests of the earlier writing that the controller loads and saves.

use super::harness::{Harness, history_on, mail_app, mail_entry, mail_history_app};
use crate::controller::Timer;
use crate::desktop::FieldText;
use crate::history::HistoryEntry;
use crate::inference::{InferenceRequest, NextWordBias};
use crate::key::Key;
use crate::prompt::framed_prompt;
use crate::settings::{HistoryAccess, HistoryScope, PersonalizationSettings, ScreenTextSettings, Settings};
use crate::settings_window::general::CLIPBOARD;
use crate::settings_window::personalization::{DELETE, HISTORY};

#[test]
fn key_in_a_new_app_saves_nothing_before_the_input_settles() {
    let mut harness = Harness::new(Settings::default());
    harness.focus_text_field_of("com.apple.mail", "Mail");

    harness.press(Key::Char('H'));

    assert_eq!(harness.saved(), Vec::new());
}

#[test]
fn typing_in_a_new_app_adds_the_app_to_both_app_lists_and_keeps_it() {
    let mut harness = Harness::new(Settings::default());
    harness.focus_text_field_of("com.apple.mail", "Mail");
    harness.press(Key::Char('H'));
    harness.press(Key::Char('i'));

    harness
        .controller
        .on_timer(Timer::InputSettle, &mut harness.shell);

    assert_eq!(
        harness.saved(),
        vec![Settings {
            screen_text: ScreenTextSettings {
                apps: vec![mail_app(true)],
                ..ScreenTextSettings::default()
            },
            personalization: PersonalizationSettings {
                apps: vec![mail_history_app(HistoryScope::AllApps)],
                ..PersonalizationSettings::default()
            },
            ..Settings::default()
        }]
    );
}

#[test]
fn history_loads_at_start_only_when_it_is_on() {
    let settings = [Settings::default(), history_on()];

    let loads = settings.map(|settings| Harness::new(settings).history_loads());

    assert_eq!(loads, [0, 1]);
}

#[test]
fn turning_on_the_history_loads_it_once_and_keeps_the_setting() {
    let mut harness = Harness::new(Settings::default());

    harness.toggle(HISTORY, true);
    harness.toggle(HISTORY, false);
    harness.toggle(HISTORY, true);

    assert_eq!(harness.history_loads(), 1);
    assert_eq!(
        harness
            .saved()
            .first()
            .map(|settings| settings.personalization.history),
        Some(HistoryAccess::On)
    );
}

#[test]
fn finished_typing_is_saved_when_the_input_settles() {
    let mut harness = Harness::new(history_on());
    harness.focus_text_field_of("com.apple.mail", "Mail");
    harness.shell.surroundings.field_text = Some(FieldText {
        before_caret: "Hi Ana, ".to_owned(),
        after_caret: String::new(),
    });
    for character in "the report is ready and the numbers look good".chars() {
        harness.press(Key::Char(character));
    }
    harness.press(Key::Other);
    let saves_before_settle = harness.history_saves().len();

    harness.settle_input();

    assert_eq!(saves_before_settle, 0);
    assert_eq!(
        harness.history_saves(),
        vec![vec![mail_entry(
            "Hi Ana, the report is ready and the numbers look good"
        )]]
    );
}

#[test]
fn earlier_writing_of_the_store_goes_into_the_prompt() {
    let mut harness = Harness::with_kept_history(history_on(), vec![mail_entry("Cheers,\nTom")]);
    harness.focus_text_field_of("com.apple.mail", "Mail");

    harness.press(Key::Char('H'));

    assert_eq!(
        harness.last_request(),
        Some(InferenceRequest::Complete {
            prompt: framed_prompt(
                &[
                    "Where I am typing: Format: email; App: Mail",
                    "My earlier writing:\n“Cheers,\nTom”"
                ],
                "H"
            ),
            max_tokens: Settings::default().suggestion_length_in_use().max_tokens(),
            word_bias: None,
            next_word_bias: NextWordBias::default(),
            min_log_probability: Some(Settings::default().suggestion_length_in_use().tail_cut()),
            min_typed_log_probability: Settings::default().typo_rule.typed_letters_cut(),
            fixes_typos: true,
        })
    );
}

#[test]
fn app_turned_off_deletes_its_earlier_writing() {
    let notes = HistoryEntry {
        app_id: "com.apple.Notes".to_owned(),
        ..mail_entry("Shopping list")
    };
    let mut harness =
        Harness::with_kept_history(history_on(), vec![mail_entry("Cheers,\nTom"), notes.clone()]);

    harness.pick("personalization-app:com.apple.mail", 0);

    assert_eq!(harness.history_saves(), vec![vec![notes]]);
}

#[test]
fn app_turned_off_while_the_history_is_off_keeps_the_other_apps() {
    let notes = HistoryEntry {
        app_id: "com.apple.Notes".to_owned(),
        ..mail_entry("Shopping list")
    };
    let mut settings = history_on();
    settings.personalization.history = HistoryAccess::Off;
    let mut harness = Harness::with_kept_history(settings, vec![mail_entry("Cheers,\nTom"), notes.clone()]);

    harness.pick("personalization-app:com.apple.mail", 0);

    assert_eq!(harness.history_saves(), vec![vec![notes]]);
}

#[test]
fn second_press_of_the_delete_button_deletes_all_earlier_writing() {
    let mut harness = Harness::with_kept_history(history_on(), vec![mail_entry("Cheers,\nTom")]);

    harness.press_button(DELETE);
    let saves_after_first_press = harness.history_saves().len();
    harness.press_button(DELETE);

    assert_eq!(saves_after_first_press, 0);
    assert_eq!(harness.history_saves(), vec![Vec::new()]);
}

#[test]
fn other_input_cancels_the_first_press_of_the_delete_button() {
    let mut harness = Harness::new(history_on());

    harness.press_button(DELETE);
    harness.toggle(CLIPBOARD, true);
    harness.press_button(DELETE);

    assert_eq!(harness.history_saves(), Vec::<Vec<HistoryEntry>>::new());
}
