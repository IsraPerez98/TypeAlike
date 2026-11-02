//! Tests of the badge at the caret and its menu.

use super::fixtures::{FakeDesktop, MAIL, WEBMAIL, type_key};
use crate::app::{App, BadgeCommand, BadgeEntry, BadgeItem, BadgeMenu, BadgeSettings, TrayCommand};
use crate::desktop::Field;
use crate::key::{Key, Press};
use crate::localization::UiLanguage;
use crate::settings::{
    HistoryAccess, HistoryScope, PersonalizationSettings, ScreenTextAccess, ScreenTextSettings,
    SuggestionPlaces,
};
use crate::test_support::{active_badge, english, mail_app, mail_history_app, paused_badge};

#[test]
fn new_app_shows_no_badge() {
    let app = App::new();

    assert_eq!(app.badge_view(&english()), None);
}

#[test]
fn typing_in_open_field_shows_active_badge() {
    let mut app = App::new();

    type_key(&mut app, 'B');

    assert_eq!(app.badge_view(&english()), Some(active_badge()));
}

#[test]
fn typing_in_excluded_field_hides_badge() {
    let mut app = App::new();
    type_key(&mut app, 'B');

    app.on_key(Key::Char('x'), Press::First, &FakeDesktop(Field::Excluded));

    assert_eq!(app.badge_view(&english()), None);
}

#[test]
fn context_lost_hides_badge() {
    let mut app = App::new();
    type_key(&mut app, 'B');

    app.on_context_lost();

    assert_eq!(app.badge_view(&english()), None);
}

#[test]
fn pausing_turns_the_badge_paused() {
    let mut app = App::new();
    type_key(&mut app, 'B');

    app.on_tray_command(TrayCommand::TogglePause);

    assert_eq!(app.badge_view(&english()), Some(paused_badge()));
}

#[test]
fn paused_app_shows_paused_badge_in_open_field() {
    let mut app = App::new();
    app.on_tray_command(TrayCommand::TogglePause);

    type_key(&mut app, 'B');

    assert_eq!(app.badge_view(&english()), Some(paused_badge()));
}

fn mail_screen_text(access: ScreenTextAccess, is_on: bool) -> ScreenTextSettings {
    ScreenTextSettings {
        access,
        apps: vec![mail_app(is_on)],
        ..ScreenTextSettings::default()
    }
}

fn mail_personalization(history: HistoryAccess, scope: HistoryScope) -> PersonalizationSettings {
    PersonalizationSettings {
        history,
        apps: vec![mail_history_app(scope)],
        ..PersonalizationSettings::default()
    }
}

fn pause_entry(label: &str) -> BadgeEntry {
    BadgeEntry::Item(BadgeItem {
        label: label.to_owned(),
        command: BadgeCommand::TogglePause,
        is_checked: false,
        is_enabled: true,
    })
}

fn scope_item(label: &str, scope: HistoryScope, is_checked: bool) -> BadgeItem {
    BadgeItem {
        label: label.to_owned(),
        command: BadgeCommand::SelectHistoryScope {
            app_id: "com.apple.mail".to_owned(),
            scope,
        },
        is_checked,
        is_enabled: true,
    }
}

/// The label of each entry, and whether the user can pick it.
fn entry_states(menu: &BadgeMenu) -> Vec<(&str, bool)> {
    menu.entries
        .iter()
        .filter_map(|entry| match entry {
            BadgeEntry::Item(item) => Some((item.label.as_str(), item.is_enabled)),
            BadgeEntry::Submenu {
                label, is_enabled, ..
            } => Some((label.as_str(), *is_enabled)),
            BadgeEntry::Separator => None,
        })
        .collect()
}

#[test]
fn badge_menu_switches_the_screen_text_and_the_earlier_writing_of_the_typed_app() {
    let mut app = App::new();
    app.on_key(Key::Char('H'), Press::First, &MAIL);

    let menu = app.badge_menu(
        &BadgeSettings {
            suggestion_places: &SuggestionPlaces::default(),
            screen_text: &mail_screen_text(ScreenTextAccess::Allowed, true),
            personalization: &mail_personalization(HistoryAccess::On, HistoryScope::ThisApp),
        },
        &english(),
    );

    assert_eq!(
        menu,
        Some(BadgeMenu {
            entries: vec![
                pause_entry("Pause suggestions"),
                BadgeEntry::Separator,
                BadgeEntry::Item(BadgeItem {
                    label: "Use screen text in Mail".to_owned(),
                    command: BadgeCommand::SwitchScreenText {
                        app_id: "com.apple.mail".to_owned(),
                        is_on: false,
                    },
                    is_checked: true,
                    is_enabled: true,
                }),
                BadgeEntry::Submenu {
                    label: "Earlier writing in Mail".to_owned(),
                    items: vec![
                        scope_item("Off", HistoryScope::Off, false),
                        scope_item("Only this app", HistoryScope::ThisApp, true),
                        scope_item("All apps", HistoryScope::AllApps, false),
                    ],
                    is_enabled: true,
                },
            ],
            language: UiLanguage::English,
        })
    );
}

#[test]
fn badge_menu_dims_the_entries_of_the_app_while_their_main_switches_are_off() {
    let mut app = App::new();
    app.on_key(Key::Char('H'), Press::First, &MAIL);

    let menu = app
        .badge_menu(
            &BadgeSettings {
                suggestion_places: &SuggestionPlaces::default(),
                screen_text: &mail_screen_text(ScreenTextAccess::Denied, true),
                personalization: &mail_personalization(HistoryAccess::Off, HistoryScope::AllApps),
            },
            &english(),
        )
        .expect("a key in an open field shows the badge");

    assert_eq!(
        entry_states(&menu),
        [
            ("Pause suggestions", true),
            ("Use screen text in Mail", false),
            ("Earlier writing in Mail", false),
        ]
    );
}

#[test]
fn badge_menu_offers_only_the_pause_in_an_app_without_an_id() {
    let mut app = App::new();
    type_key(&mut app, 'H');

    let menu = app.badge_menu(
        &BadgeSettings {
            suggestion_places: &SuggestionPlaces::default(),
            screen_text: &mail_screen_text(ScreenTextAccess::Allowed, true),
            personalization: &mail_personalization(HistoryAccess::On, HistoryScope::AllApps),
        },
        &english(),
    );

    assert_eq!(
        menu,
        Some(BadgeMenu {
            entries: vec![pause_entry("Pause suggestions")],
            language: UiLanguage::English,
        })
    );
}

#[test]
fn badge_menu_leaves_out_the_entries_of_an_app_that_the_lists_do_not_have() {
    let mut app = App::new();
    app.on_key(Key::Char('H'), Press::First, &MAIL);

    let menu = app.badge_menu(
        &BadgeSettings {
            suggestion_places: &SuggestionPlaces::default(),
            screen_text: &ScreenTextSettings::default(),
            personalization: &PersonalizationSettings::default(),
        },
        &english(),
    );

    assert_eq!(
        menu,
        Some(BadgeMenu {
            entries: vec![pause_entry("Pause suggestions")],
            language: UiLanguage::English,
        })
    );
}

#[test]
fn paused_badge_menu_offers_the_resume_and_the_entries_of_the_app_of_the_field() {
    let mut app = App::new();
    app.on_tray_command(TrayCommand::TogglePause);
    app.on_key(Key::Char('H'), Press::First, &MAIL);

    let menu = app
        .badge_menu(
            &BadgeSettings {
                suggestion_places: &SuggestionPlaces::default(),
                screen_text: &mail_screen_text(ScreenTextAccess::Allowed, true),
                personalization: &mail_personalization(HistoryAccess::On, HistoryScope::AllApps),
            },
            &english(),
        )
        .expect("a key in an open field shows the badge");

    assert_eq!(
        entry_states(&menu),
        [
            ("Resume suggestions", true),
            ("Use screen text in Mail", true),
            ("Earlier writing in Mail", true),
        ]
    );
}

#[test]
fn no_badge_menu_shows_without_a_badge() {
    let app = App::new();

    let menu = app.badge_menu(
        &BadgeSettings {
            suggestion_places: &SuggestionPlaces::default(),
            screen_text: &mail_screen_text(ScreenTextAccess::Allowed, true),
            personalization: &mail_personalization(HistoryAccess::On, HistoryScope::AllApps),
        },
        &english(),
    );

    assert_eq!(menu, None);
}

#[test]
fn typing_while_paused_names_the_app_of_the_field() {
    let mut app = App::new();
    app.on_tray_command(TrayCommand::TogglePause);

    app.on_key(Key::Char('H'), Press::First, &MAIL);

    assert_eq!(
        app.typed_app().map(|typed| typed.app_id.as_str()),
        Some("com.apple.mail")
    );
}

#[test]
fn typing_names_the_website_of_the_session() {
    let mut app = App::new();

    app.on_key(Key::Char('H'), Press::First, &WEBMAIL);

    assert_eq!(app.typed_host(), Some("mail.example"));
}

#[test]
fn typing_while_paused_names_the_website_of_the_field() {
    let mut app = App::new();
    app.on_tray_command(TrayCommand::TogglePause);

    app.on_key(Key::Char('H'), Press::First, &WEBMAIL);

    assert_eq!(app.typed_host(), Some("mail.example"));
}

#[test]
fn lost_context_forgets_the_website_of_the_paused_field() {
    let mut app = App::new();
    app.on_tray_command(TrayCommand::TogglePause);
    app.on_key(Key::Char('H'), Press::First, &WEBMAIL);

    app.on_context_lost();

    assert_eq!(app.typed_host(), None);
}

#[test]
fn lost_context_forgets_the_app_of_the_paused_field() {
    let mut app = App::new();
    app.on_tray_command(TrayCommand::TogglePause);
    app.on_key(Key::Char('H'), Press::First, &MAIL);

    app.on_context_lost();

    assert_eq!(app.typed_app(), None);
}

#[test]
fn paused_app_shows_no_badge_in_excluded_field() {
    let mut app = App::new();
    app.on_tray_command(TrayCommand::TogglePause);

    app.on_key(Key::Char('B'), Press::First, &FakeDesktop(Field::Excluded));

    assert_eq!(app.badge_view(&english()), None);
}
