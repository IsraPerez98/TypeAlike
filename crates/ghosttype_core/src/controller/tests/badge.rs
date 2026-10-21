//! Tests of the menu of the badge at the caret.

use super::harness::{Call, Harness, history_on, suggestion_settings};
use crate::app::{BadgeCommand, Status};
use crate::inference::InferenceRequest;
use crate::key::Key;
use crate::settings::{HistoryScope, Settings};
use crate::test_support::{mail_app, mail_entry, mail_history_app};

#[test]
fn badge_pause_command_pauses_suggestions() {
    let mut harness = Harness::new(suggestion_settings());
    harness.press(Key::Char('B'));

    harness
        .controller
        .on_badge_command(BadgeCommand::TogglePause, &mut harness.shell);

    assert_eq!(harness.controller.tray_view().status, Status::Paused);
}

#[test]
fn badge_pause_command_unloads_the_model() {
    let mut harness = Harness::new(suggestion_settings());
    harness.press(Key::Char('B'));

    harness
        .controller
        .on_badge_command(BadgeCommand::TogglePause, &mut harness.shell);

    assert_eq!(harness.last_request(), Some(InferenceRequest::Unload));
}

/// The labels of the entries of the badge menu that run a command, top to bottom.
fn badge_item_labels(harness: &Harness) -> Vec<String> {
    harness
        .controller
        .badge_menu()
        .expect("the badge shows")
        .items()
        .map(|item| item.label.clone())
        .collect()
}

#[test]
fn badge_menu_offers_the_switches_of_the_app_where_the_user_types() {
    let mut harness = Harness::new(history_on());
    harness.focus_text_field_of("com.apple.mail", "Mail");

    harness.press(Key::Char('H'));

    assert_eq!(
        badge_item_labels(&harness),
        [
            "Pause suggestions",
            "Use screen text in Mail",
            "Off",
            "Only this app",
            "All apps"
        ]
    );
}

#[test]
fn badge_menu_while_paused_offers_the_switches_of_the_app_where_the_user_types() {
    let mut harness = Harness::new(history_on());
    harness.focus_text_field_of("com.apple.mail", "Mail");
    harness.toggle_pause();

    harness.press(Key::Char('H'));

    assert_eq!(
        badge_item_labels(&harness).first().map(String::as_str),
        Some("Resume suggestions")
    );
    assert_eq!(badge_item_labels(&harness).len(), 5);
}

#[test]
fn badge_screen_text_command_turns_off_the_app_and_keeps_the_setting() {
    let mut harness = Harness::new(Settings::default());
    harness.focus_text_field_of("com.apple.mail", "Mail");
    harness.press(Key::Char('H'));

    harness.controller.on_badge_command(
        BadgeCommand::SwitchScreenText {
            app_id: "com.apple.mail".to_owned(),
            is_on: false,
        },
        &mut harness.shell,
    );

    assert_eq!(
        harness
            .saved()
            .last()
            .map(|settings| settings.screen_text.apps.clone()),
        Some(vec![mail_app(false)])
    );
}

/// Picks `scope` for Mail in the menu of the badge.
fn select_mail_scope(harness: &mut Harness, scope: HistoryScope) {
    harness.controller.on_badge_command(
        BadgeCommand::SelectHistoryScope {
            app_id: "com.apple.mail".to_owned(),
            scope,
        },
        &mut harness.shell,
    );
}

#[test]
fn badge_scope_command_turns_off_the_app() {
    let mut harness = Harness::with_kept_history(history_on(), vec![mail_entry("Cheers,\nTom")]);
    harness.focus_text_field_of("com.apple.mail", "Mail");
    harness.press(Key::Char('H'));

    select_mail_scope(&mut harness, HistoryScope::Off);

    assert_eq!(
        harness
            .saved()
            .last()
            .map(|settings| settings.personalization.apps.clone()),
        Some(vec![mail_history_app(HistoryScope::Off)])
    );
}

#[test]
fn badge_scope_command_deletes_the_text_of_the_app() {
    let mut harness = Harness::with_kept_history(history_on(), vec![mail_entry("Cheers,\nTom")]);
    harness.focus_text_field_of("com.apple.mail", "Mail");
    harness.press(Key::Char('H'));

    select_mail_scope(&mut harness, HistoryScope::Off);

    assert_eq!(harness.history_saves().last().cloned(), Some(Vec::new()));
}

#[test]
fn badge_scope_command_shows_the_new_scope_in_the_settings_window() {
    let mut harness = Harness::new(history_on());
    harness.focus_text_field_of("com.apple.mail", "Mail");
    harness.press(Key::Char('H'));
    harness.take_calls();

    select_mail_scope(&mut harness, HistoryScope::ThisApp);

    assert!(matches!(
        harness.take_calls().as_slice(),
        [Call::RenderSettings(_)]
    ));
}
