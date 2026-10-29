//! Tests of the hold of a quick Tab after the last part of a suggestion.

use super::harness::{Call, Harness, completed, field_around, ghost_at_caret, history_on};
use crate::controller::tab_hold::TAB_HOLD_DELAY;
use crate::controller::{FocusChange, Timer};
use crate::desktop::Surroundings;
use crate::inference::InferenceEvent;
use crate::key::{Disposition, Key};
use crate::settings::{PersonalizationSettings, Settings, SuggestionDisplay, WordChoice};
use crate::test_support::{MAIL_APP, active_badge, mail_entry, on_own_days};

/// Returns a harness after a Tab on the last part of a suggestion, while the next ghost text does
/// not show yet.
fn harness_after_the_last_part() -> Harness {
    let mut harness = Harness::new(Settings::default());
    harness.show("est");
    harness.press(Key::Tab);
    harness.shell.is_ghost_shown = false;
    harness.take_calls();
    harness
}

#[test]
fn tab_on_the_last_part_starts_the_tab_hold_timer() {
    let mut harness = Harness::new(Settings::default());
    harness.show("est");

    harness.press(Key::Tab);

    assert!(
        harness
            .take_calls()
            .contains(&Call::StartTimer(Timer::TabHold, TAB_HOLD_DELAY))
    );
}

#[test]
fn tab_on_a_part_with_a_rest_starts_no_tab_hold_timer() {
    let mut harness = Harness::new(Settings::default());
    harness.show("est regards");

    harness.press(Key::Tab);

    assert!(
        !harness
            .take_calls()
            .contains(&Call::StartTimer(Timer::TabHold, TAB_HOLD_DELAY))
    );
}

#[test]
fn quick_tab_after_the_last_part_is_swallowed() {
    let mut harness = harness_after_the_last_part();

    let disposition = harness.press(Key::Tab);

    assert_eq!(disposition, Disposition::Swallow);
}

#[test]
fn next_suggestion_shows_after_a_held_tab() {
    let mut harness = harness_after_the_last_part();

    harness.press(Key::Tab);
    harness.answer(completed(" regards"));
    harness.settle_caret();

    assert!(
        harness
            .take_calls()
            .contains(&ghost_at_caret(Some(" regards"), Some(active_badge())))
    );
}

#[test]
fn held_tab_does_not_type_the_next_suggestion() {
    let mut harness = harness_after_the_last_part();

    harness.press(Key::Tab);
    harness.answer(completed(" regards"));
    harness.settle_caret();

    assert!(
        !harness
            .take_calls()
            .contains(&Call::ReplaceText(0, " regards".to_owned()))
    );
}

#[test]
fn quick_tab_while_the_completion_still_runs_is_swallowed() {
    let settings = Settings {
        suggestion_display: SuggestionDisplay::WordByWord,
        ..Settings::default()
    };
    let mut harness = Harness::new(settings);
    harness.press(Key::Char('B'));
    harness.answer(InferenceEvent::Partial("est regards".to_owned()));
    harness.settle_caret();
    harness.press(Key::Tab);
    harness.shell.is_ghost_shown = false;

    let disposition = harness.press(Key::Tab);

    assert_eq!(disposition, Disposition::Swallow);
}

#[test]
fn quick_tab_after_the_last_part_of_a_phrase_is_swallowed() {
    let settings = Settings {
        personalization: PersonalizationSettings {
            word_choice: WordChoice::Medium,
            ..history_on().personalization
        },
        ..Settings::default()
    };
    let mut harness = Harness::with_kept_history(
        settings,
        on_own_days(vec![mail_entry("Let me know if it works."); 3]),
    );
    harness.shell.surroundings = Surroundings {
        app_id: Some(MAIL_APP.to_owned()),
        ..field_around("Please let me know", "")
    };
    harness.press(Key::Char(' '));
    harness.settle_caret();
    harness.press(Key::Tab);
    harness.shell.is_ghost_shown = false;

    let disposition = harness.press(Key::Tab);

    assert_eq!(disposition, Disposition::Swallow);
}

#[test]
fn tab_while_the_next_suggestion_shows_takes_its_first_part() {
    let mut harness = harness_after_the_last_part();
    harness.answer(completed(" regards to you"));
    harness.settle_caret();
    harness.shell.is_ghost_shown = true;
    harness.take_calls();

    harness.press(Key::Tab);

    assert!(
        harness
            .take_calls()
            .contains(&Call::ReplaceText(0, " regards".to_owned()))
    );
}

#[test]
fn other_keys_end_the_tab_hold() {
    let keys = [
        Key::Char(' '),
        Key::Backspace,
        Key::Escape,
        Key::Enter,
        Key::Dead,
        Key::Other,
    ];

    let dispositions = keys.map(|key| {
        let mut harness = harness_after_the_last_part();
        harness.press(key);
        harness.press(Key::Tab)
    });

    assert_eq!(dispositions, [Disposition::Pass; 6]);
}

#[test]
fn typed_key_stops_the_tab_hold_timer() {
    let mut harness = harness_after_the_last_part();

    harness.press(Key::Char(' '));

    assert!(harness.take_calls().contains(&Call::StopTimer(Timer::TabHold)));
}

#[test]
fn click_ends_the_tab_hold() {
    let mut harness = harness_after_the_last_part();
    harness.controller.on_context_lost(&mut harness.shell);

    let disposition = harness.press(Key::Tab);

    assert_eq!(disposition, Disposition::Pass);
}

#[test]
fn focus_change_ends_the_tab_hold() {
    let mut harness = harness_after_the_last_part();
    harness.shell.focus_change = FocusChange::Changed;

    let disposition = harness.press(Key::Tab);

    assert_eq!(disposition, Disposition::Pass);
}

#[test]
fn pause_ends_the_tab_hold() {
    let mut harness = harness_after_the_last_part();
    harness.toggle_pause();

    let disposition = harness.press(Key::Tab);

    assert_eq!(disposition, Disposition::Pass);
}

#[test]
fn tab_hold_timer_ends_the_tab_hold() {
    let mut harness = harness_after_the_last_part();
    harness.controller.on_timer(Timer::TabHold, &mut harness.shell);

    let disposition = harness.press(Key::Tab);

    assert_eq!(disposition, Disposition::Pass);
}

#[test]
fn completion_without_a_suggestion_ends_the_tab_hold() {
    let mut harness = harness_after_the_last_part();
    harness.answer(completed(""));

    let disposition = harness.press(Key::Tab);

    assert_eq!(disposition, Disposition::Pass);
}
