//! Tests of the apps and the websites where the user turned suggestions off.

use std::cell::Cell;

use super::fixtures::{FakeDesktop, MAIL, WEBMAIL, app_preparing_on_focus};
use crate::app::{App, BadgeCommand, BadgeEntry, BadgeItem, BadgeMenu, SettledWork, TrayCommand};
use crate::desktop::{CopiedText, Desktop, Field, FieldText, ScreenRead, ScreenText, Surroundings};
use crate::engine::Reaction;
use crate::key::Key;
use crate::settings::{
    HistoryAccess, HistoryScope, PersonalizationSettings, PromptPreparation, ScreenTextSettings, Settings,
    SuggestionPlaces,
};
use crate::test_support::{
    ACTIVE_BADGE, OFF_BADGE, PAUSED_BADGE, UiText, history_app, screen_text_app, suggestion_app,
    suggestion_site, writing_rule_breaks,
};

fn mail_places(is_on: bool) -> SuggestionPlaces {
    SuggestionPlaces {
        apps: vec![suggestion_app("com.apple.mail", "Mail", is_on)],
        ..SuggestionPlaces::default()
    }
}

fn webmail_places(is_app_on: bool, is_site_on: bool) -> SuggestionPlaces {
    SuggestionPlaces {
        apps: vec![suggestion_app("com.apple.Safari", "Safari", is_app_on)],
        sites: vec![suggestion_site("mail.example", is_site_on)],
    }
}

fn app_with(places: SuggestionPlaces) -> App {
    let mut app = App::new();
    app.use_settings(&Settings {
        suggestion_places: places,
        ..Settings::default()
    });
    app
}

fn app_preparing_on_focus_with(places: SuggestionPlaces) -> App {
    let mut app = App::new();
    app.use_settings(&Settings {
        suggestion_places: places,
        prompt_preparation: PromptPreparation::OnFocus,
        ..Settings::default()
    });
    app
}

#[test]
fn key_in_an_off_app_asks_for_no_suggestion() {
    let mut app = app_with(mail_places(false));

    let reaction = app.on_key(Key::Char('H'), &MAIL);

    assert_eq!((reaction, app.has_offer()), (Reaction::Hide, false));
}

#[test]
fn key_on_an_off_website_asks_for_no_suggestion() {
    let mut app = app_with(webmail_places(true, false));

    let reaction = app.on_key(Key::Char('H'), &WEBMAIL);

    assert_eq!((reaction, app.has_offer()), (Reaction::Hide, false));
}

#[test]
fn key_in_an_app_whose_switch_is_on_asks_for_a_suggestion() {
    let mut app = app_with(mail_places(true));

    let reaction = app.on_key(Key::Char('H'), &MAIL);

    assert!(matches!(reaction, Reaction::Request(_)));
}

#[test]
fn tab_in_an_off_app_reaches_the_app() {
    let mut app = app_with(mail_places(false));
    app.on_key(Key::Char('H'), &MAIL);

    let reaction = app.on_key(Key::Tab, &MAIL);

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn settled_input_in_an_off_app_reads_no_screen_and_prepares_nothing() {
    let mut app = app_preparing_on_focus_with(mail_places(false));

    let work = app.on_input_settled(&MAIL);

    assert_eq!(work, None);
}

#[test]
fn settled_input_on_an_off_website_reads_no_screen_and_prepares_nothing() {
    let mut app = app_preparing_on_focus_with(webmail_places(true, false));

    let work = app.on_input_settled(&WEBMAIL);

    assert_eq!(work, None);
}

#[test]
fn settled_input_in_an_on_app_reads_the_screen() {
    let mut app = app_preparing_on_focus_with(mail_places(true));

    let work = app.on_input_settled(&MAIL);

    assert!(matches!(work, Some(SettledWork::ReadScreen { .. })));
}

#[test]
fn screen_answer_after_the_app_turned_off_prepares_nothing() {
    let mut app = app_preparing_on_focus();
    let Some(SettledWork::ReadScreen { read, .. }) = app.on_input_settled(&MAIL) else {
        panic!("a settled text field reads the screen");
    };
    app.use_settings(&Settings {
        suggestion_places: mail_places(false),
        prompt_preparation: PromptPreparation::OnFocus,
        ..Settings::default()
    });

    let work = app.on_screen_read(
        ScreenRead {
            id: read,
            text: Some(ScreenText::default()),
        },
        &MAIL,
    );

    assert_eq!(work, None);
}

#[test]
fn turning_off_the_app_of_the_session_ends_the_session() {
    let mut app = app_with(mail_places(true));
    app.on_key(Key::Char('H'), &MAIL);

    app.use_settings(&Settings {
        suggestion_places: mail_places(false),
        ..Settings::default()
    });

    assert_eq!((app.text_before_caret(), app.has_offer()), ("", false));
}

#[test]
fn turning_off_the_app_of_the_session_shows_the_off_badge_and_keeps_the_app() {
    let mut app = app_with(mail_places(true));
    app.on_key(Key::Char('H'), &MAIL);

    app.use_settings(&Settings {
        suggestion_places: mail_places(false),
        ..Settings::default()
    });

    assert_eq!(
        (
            app.badge_view(),
            app.typed_app().map(|typed| typed.app_id.as_str())
        ),
        (Some(OFF_BADGE), Some("com.apple.mail"))
    );
}

#[test]
fn adding_an_app_to_the_list_keeps_the_session() {
    let mut app = App::new();
    app.on_key(Key::Char('H'), &MAIL);

    app.use_settings(&Settings {
        suggestion_places: mail_places(true),
        ..Settings::default()
    });

    assert_eq!(app.text_before_caret(), "H");
}

#[test]
fn key_in_an_app_turned_on_again_asks_for_a_suggestion() {
    let mut app = app_with(mail_places(false));
    app.on_key(Key::Char('H'), &MAIL);
    app.use_settings(&Settings {
        suggestion_places: mail_places(true),
        ..Settings::default()
    });

    let reaction = app.on_key(Key::Char('i'), &MAIL);

    assert!(matches!(reaction, Reaction::Request(_)));
}

#[test]
fn typing_in_an_off_app_shows_the_off_badge() {
    let mut app = app_with(mail_places(false));

    app.on_key(Key::Char('H'), &MAIL);

    assert_eq!(app.badge_view(), Some(OFF_BADGE));
}

#[test]
fn other_keys_in_an_off_app_keep_the_off_badge() {
    let mut app = app_with(mail_places(false));
    app.on_key(Key::Char('H'), &MAIL);

    app.on_key(Key::Other, &MAIL);

    assert_eq!(app.badge_view(), Some(OFF_BADGE));
}

#[test]
fn turning_the_app_on_again_shows_the_active_badge() {
    let mut app = app_with(mail_places(false));
    app.on_key(Key::Char('H'), &MAIL);

    app.use_settings(&Settings {
        suggestion_places: mail_places(true),
        ..Settings::default()
    });

    assert_eq!(app.badge_view(), Some(ACTIVE_BADGE));
}

#[test]
fn paused_badge_shows_in_an_off_app_while_suggestions_are_paused() {
    let mut app = app_with(mail_places(false));
    app.on_tray_command(TrayCommand::TogglePause);

    app.on_key(Key::Char('H'), &MAIL);

    assert_eq!(app.badge_view(), Some(PAUSED_BADGE));
}

#[test]
fn excluded_field_in_an_off_app_shows_no_badge() {
    let mut app = app_with(mail_places(false));
    app.on_key(Key::Char('H'), &MAIL);

    app.on_key(Key::Char('x'), &FakeDesktop(Field::Excluded));

    assert_eq!((app.badge_view(), app.typed_app()), (None, None));
}

#[test]
fn typing_in_an_off_app_names_the_app_and_the_website() {
    let mut app = app_with(webmail_places(false, true));

    app.on_key(Key::Char('H'), &WEBMAIL);

    assert_eq!(
        (
            app.typed_app().map(|typed| typed.app_id.as_str()),
            app.typed_host()
        ),
        (Some("com.apple.Safari"), Some("mail.example"))
    );
}

fn suggestion_item(label: &str, command: BadgeCommand, is_checked: bool) -> BadgeEntry {
    BadgeEntry::Item(BadgeItem {
        label: label.to_owned(),
        command,
        is_checked,
        is_enabled: true,
    })
}

fn badge_menu_of(app: &App, places: &SuggestionPlaces) -> Option<BadgeMenu> {
    app.badge_menu(
        places,
        &ScreenTextSettings::default(),
        &PersonalizationSettings::default(),
    )
}

#[test]
fn badge_menu_switches_the_suggestions_of_the_app_and_of_the_website() {
    let places = webmail_places(true, true);
    let mut app = app_with(places.clone());
    app.on_key(Key::Char('H'), &WEBMAIL);

    let menu = badge_menu_of(&app, &places);

    assert_eq!(
        menu.map(|menu| menu.entries),
        Some(vec![
            suggestion_item("Pause suggestions", BadgeCommand::TogglePause, false),
            BadgeEntry::Separator,
            suggestion_item(
                "Show suggestions in Safari",
                BadgeCommand::SwitchAppSuggestions {
                    app_id: "com.apple.Safari".to_owned(),
                    is_on: false,
                },
                true
            ),
            suggestion_item(
                "Show suggestions on mail.example",
                BadgeCommand::SwitchSiteSuggestions {
                    host: "mail.example".to_owned(),
                    is_on: false,
                },
                true
            ),
        ])
    );
}

#[test]
fn badge_menu_in_an_off_app_turns_its_suggestions_on_again() {
    let places = mail_places(false);
    let mut app = app_with(places.clone());
    app.on_key(Key::Char('H'), &MAIL);

    let menu = badge_menu_of(&app, &places);

    assert_eq!(
        menu.and_then(|menu| menu.entries.get(2).cloned()),
        Some(suggestion_item(
            "Show suggestions in Mail",
            BadgeCommand::SwitchAppSuggestions {
                app_id: "com.apple.mail".to_owned(),
                is_on: true,
            },
            false
        ))
    );
}

#[test]
fn badge_menu_leaves_out_a_website_that_the_list_does_not_have() {
    let places = mail_places(true);
    let mut app = app_with(places.clone());
    app.on_key(Key::Char('H'), &WEBMAIL);

    let menu = badge_menu_of(&app, &places);

    assert_eq!(
        menu.map(|menu| menu.entries),
        Some(vec![suggestion_item(
            "Pause suggestions",
            BadgeCommand::TogglePause,
            false
        )])
    );
}

/// A field that counts the reads of its surroundings.
#[derive(Default)]
struct CountingField {
    reads: Cell<usize>,
}

impl Desktop for CountingField {
    fn field(&self) -> Field {
        Field::Open
    }

    fn surroundings(&self) -> Surroundings {
        self.reads.set(self.reads.get() + 1);
        Surroundings {
            app_id: Some("com.apple.mail".to_owned()),
            field_text: Some(FieldText::default()),
            ..Surroundings::default()
        }
    }

    fn copied_text(&self) -> Option<CopiedText> {
        None
    }
}

#[test]
fn first_key_reads_the_surroundings_one_time() {
    let mut app = app_with(mail_places(true));
    let field = CountingField::default();

    app.on_key(Key::Char('H'), &field);

    assert_eq!(field.reads.get(), 1);
}

#[test]
fn key_that_starts_no_session_reads_no_surroundings() {
    let mut app = app_with(mail_places(true));
    let field = CountingField::default();

    app.on_key(Key::Other, &field);

    assert_eq!(field.reads.get(), 0);
}

fn mail_screen_text() -> ScreenTextSettings {
    ScreenTextSettings {
        apps: vec![
            screen_text_app("com.apple.mail", "Mail", true),
            screen_text_app("com.apple.Safari", "Safari", true),
        ],
        ..ScreenTextSettings::default()
    }
}

fn mail_personalization() -> PersonalizationSettings {
    PersonalizationSettings {
        history: HistoryAccess::On,
        apps: vec![
            history_app("com.apple.mail", "Mail", HistoryScope::AllApps),
            history_app("com.apple.Safari", "Safari", HistoryScope::AllApps),
        ],
        ..PersonalizationSettings::default()
    }
}

/// The label of each entry that runs a command, and whether the user can pick it.
fn item_states(menu: &BadgeMenu) -> Vec<(&str, bool)> {
    menu.items()
        .map(|item| (item.label.as_str(), item.is_enabled))
        .collect()
}

fn is_scope_menu_enabled(menu: &BadgeMenu) -> Option<bool> {
    menu.entries.iter().find_map(|entry| match entry {
        BadgeEntry::Submenu { is_enabled, .. } => Some(*is_enabled),
        _ => None,
    })
}

#[test]
fn badge_menu_in_an_off_app_dims_the_screen_text_and_the_earlier_writing_of_the_app() {
    let places = mail_places(false);
    let mut app = app_with(places.clone());
    app.on_key(Key::Char('H'), &MAIL);

    let menu = app
        .badge_menu(&places, &mail_screen_text(), &mail_personalization())
        .expect("a key in an open field shows the badge");

    assert_eq!(
        (item_states(&menu), is_scope_menu_enabled(&menu)),
        (
            vec![
                ("Pause suggestions", true),
                ("Show suggestions in Mail", true),
                ("Use screen text in Mail", false),
                ("Off", false),
                ("Only this app", false),
                ("All apps", false),
            ],
            Some(false)
        )
    );
}

#[test]
fn badge_menu_on_an_off_website_dims_the_screen_text_and_the_earlier_writing_of_the_app() {
    let places = webmail_places(true, false);
    let mut app = app_with(places.clone());
    app.on_key(Key::Char('H'), &WEBMAIL);

    let menu = app
        .badge_menu(&places, &mail_screen_text(), &mail_personalization())
        .expect("a key in an open field shows the badge");

    assert_eq!(
        (item_states(&menu), is_scope_menu_enabled(&menu)),
        (
            vec![
                ("Pause suggestions", true),
                ("Show suggestions in Safari", true),
                ("Show suggestions on mail.example", true),
                ("Use screen text in Safari", false),
                ("Off", false),
                ("Only this app", false),
                ("All apps", false),
            ],
            Some(false)
        )
    );
}

#[test]
fn badge_menu_in_an_on_place_keeps_the_screen_text_and_the_earlier_writing_of_the_app() {
    let places = webmail_places(true, true);
    let mut app = app_with(places.clone());
    app.on_key(Key::Char('H'), &WEBMAIL);

    let menu = app
        .badge_menu(&places, &mail_screen_text(), &mail_personalization())
        .expect("a key in an open field shows the badge");

    assert_eq!(
        (
            item_states(&menu).iter().all(|&(_, is_enabled)| is_enabled),
            is_scope_menu_enabled(&menu)
        ),
        (true, Some(true))
    );
}

/// The labels of the entries of `menu` and of its submenus.
fn menu_texts(menu: &BadgeMenu) -> Vec<UiText> {
    let submenu_labels = menu.entries.iter().filter_map(|entry| match entry {
        BadgeEntry::Submenu { label, .. } => Some(label),
        BadgeEntry::Item(_) | BadgeEntry::Separator => None,
    });
    let item_labels = menu.items().map(|item| &item.label);
    submenu_labels.chain(item_labels).map(UiText::name).collect()
}

#[test]
fn badge_menu_follows_the_writing_rules() {
    let places = webmail_places(true, true);
    let mut app = app_with(places.clone());
    app.on_key(Key::Char('H'), &WEBMAIL);

    let menu = app
        .badge_menu(&places, &mail_screen_text(), &mail_personalization())
        .expect("a key in an open field shows the badge");

    assert_eq!(writing_rule_breaks(&menu_texts(&menu)), Vec::<String>::new());
}
