//! Tests of the menu of the badge at the caret.

use super::harness::{
    Call, Harness, history_on, mail_app, mail_entry, mail_history_app, suggestion_settings,
};
use crate::app::{BadgeCommand, Status, TrayCommand};
use crate::inference::InferenceRequest;
use crate::key::Key;
use crate::settings::{HistoryScope, Settings};

#[test]
fn badge_command_pauses_suggestions_and_unloads_the_model() {
    let mut harness = Harness::new(suggestion_settings());
    harness.press(Key::Char('B'));

    harness
        .controller
        .on_badge_command(BadgeCommand::TogglePause, &mut harness.shell);

    assert_eq!(
        (harness.controller.tray_view().status, harness.last_request()),
        (Status::Paused, Some(InferenceRequest::Unload))
    );
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
    harness
        .controller
        .on_tray_command(TrayCommand::TogglePause, &mut harness.shell);

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

#[test]
fn badge_scope_command_turns_off_the_app_and_deletes_its_text() {
    let mut harness = Harness::with_kept_history(history_on(), vec![mail_entry("Cheers,\nTom")]);
    harness.focus_text_field_of("com.apple.mail", "Mail");
    harness.press(Key::Char('H'));

    harness.controller.on_badge_command(
        BadgeCommand::SelectHistoryScope {
            app_id: "com.apple.mail".to_owned(),
            scope: HistoryScope::Off,
        },
        &mut harness.shell,
    );

    assert_eq!(
        (
            harness
                .saved()
                .last()
                .map(|settings| settings.personalization.apps.clone()),
            harness.history_saves().last().cloned(),
        ),
        (Some(vec![mail_history_app(HistoryScope::Off)]), Some(Vec::new()))
    );
}

#[test]
fn badge_scope_command_shows_the_new_scope_in_the_settings_window() {
    let mut harness = Harness::new(history_on());
    harness.focus_text_field_of("com.apple.mail", "Mail");
    harness.press(Key::Char('H'));
    harness.take_calls();

    harness.controller.on_badge_command(
        BadgeCommand::SelectHistoryScope {
            app_id: "com.apple.mail".to_owned(),
            scope: HistoryScope::ThisApp,
        },
        &mut harness.shell,
    );

    assert!(matches!(
        harness.take_calls().as_slice(),
        [Call::RenderSettings(_)]
    ));
}
