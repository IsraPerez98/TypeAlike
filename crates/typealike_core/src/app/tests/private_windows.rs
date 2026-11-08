//! Tests of the private windows of a browser, where suggestions never show.

use super::fixtures::app_preparing_on_focus;
use crate::app::{App, BadgeCommand, BadgeEntry, BadgeItem, BadgeMenu, BadgeSettings};
use crate::desktop::{CopiedText, Desktop, Field, FieldText, Surroundings};
use crate::engine::Reaction;
use crate::key::{Key, Press};
use crate::settings::{
    HistoryAccess, HistoryScope, PersonalizationSettings, PrivateWindowRule, ScreenTextSettings, Settings,
    SuggestionPlaces,
};
use crate::test_support::{
    UiText, english, history_app, off_badge, screen_text_app, suggestion_app, suggestion_site,
    writing_rule_breaks,
};

/// A text field of a page in a window of Chrome with the title `title`.
struct ChromeWindow {
    title: &'static str,
}

const PRIVATE_WINDOW: ChromeWindow = ChromeWindow {
    title: "Trip notes - Google Chrome (Incognito)",
};

const NORMAL_WINDOW: ChromeWindow = ChromeWindow {
    title: "Trip notes - Google Chrome",
};

impl Desktop for ChromeWindow {
    fn field(&self) -> Field {
        Field::Open
    }

    fn surroundings(&self) -> Surroundings {
        Surroundings {
            app_id: Some("com.google.Chrome".to_owned()),
            window_title: Some(self.title.to_owned()),
            domain: Some("notes.example".to_owned()),
            field_text: Some(FieldText::default()),
            ..Surroundings::default()
        }
    }

    fn copied_text(&self) -> Option<CopiedText> {
        None
    }
}

#[test]
fn key_in_a_private_window_asks_for_no_suggestion() {
    let mut app = App::new();

    let reaction = app.on_key(Key::Char('H'), Press::First, &PRIVATE_WINDOW);

    assert_eq!((reaction, app.has_offer()), (Reaction::Hide, false));
}

#[test]
fn key_in_a_normal_window_of_the_browser_asks_for_a_suggestion() {
    let mut app = App::new();

    let reaction = app.on_key(Key::Char('H'), Press::First, &NORMAL_WINDOW);

    assert!(matches!(reaction, Reaction::Request(_)));
}

#[test]
fn typing_in_a_private_window_shows_the_off_badge() {
    let mut app = App::new();

    app.on_key(Key::Char('H'), Press::First, &PRIVATE_WINDOW);

    assert_eq!(app.badge_view(&english()), Some(off_badge()));
}

#[test]
fn settled_input_in_a_private_window_reads_no_screen_and_prepares_nothing() {
    let mut app = app_preparing_on_focus();

    let work = app.on_input_settled(&PRIVATE_WINDOW);

    assert_eq!(work, None);
}

#[test]
fn key_in_a_private_window_asks_for_a_suggestion_when_the_rule_keeps_suggestions() {
    let mut app = App::new();
    app.use_settings(&Settings {
        suggestion_places: SuggestionPlaces {
            private_windows: PrivateWindowRule::KeepSuggestions,
            ..SuggestionPlaces::default()
        },
        ..Settings::default()
    });

    let reaction = app.on_key(Key::Char('H'), Press::First, &PRIVATE_WINDOW);

    assert!(matches!(reaction, Reaction::Request(_)));
}

fn chrome_places(private_windows: PrivateWindowRule) -> SuggestionPlaces {
    SuggestionPlaces {
        apps: vec![suggestion_app("com.google.Chrome", "Chrome", true)],
        sites: vec![suggestion_site("notes.example", true)],
        private_windows,
    }
}

fn app_with(places: &SuggestionPlaces) -> App {
    let mut app = App::new();
    app.use_settings(&Settings {
        suggestion_places: places.clone(),
        ..Settings::default()
    });
    app
}

fn badge_menu_of(app: &App, places: &SuggestionPlaces) -> BadgeMenu {
    app.badge_menu(
        &BadgeSettings {
            suggestion_places: places,
            screen_text: &ScreenTextSettings::default(),
            personalization: &PersonalizationSettings::default(),
        },
        &english(),
    )
    .expect("a key in an open field shows the badge")
}

fn checked_item(label: &str, command: BadgeCommand) -> BadgeEntry {
    BadgeEntry::Item(BadgeItem {
        label: label.to_owned(),
        command,
        is_checked: true,
        is_enabled: true,
    })
}

#[test]
fn badge_menu_in_a_private_window_offers_the_switch_that_stops_suggestions_there() {
    let places = chrome_places(PrivateWindowRule::StopSuggestions);
    let mut app = app_with(&places);
    app.on_key(Key::Char('H'), Press::First, &PRIVATE_WINDOW);

    let menu = badge_menu_of(&app, &places);

    assert_eq!(
        menu.entries,
        [
            BadgeEntry::Item(BadgeItem {
                label: "Pause suggestions".to_owned(),
                command: BadgeCommand::TogglePause,
                is_checked: false,
                is_enabled: true,
            }),
            BadgeEntry::Separator,
            checked_item(
                "Stop suggestions in private windows",
                BadgeCommand::SwitchPrivateWindows(PrivateWindowRule::KeepSuggestions)
            ),
            checked_item(
                "Show suggestions in Chrome",
                BadgeCommand::SwitchAppSuggestions {
                    app_id: "com.google.Chrome".to_owned(),
                    is_on: false,
                }
            ),
            checked_item(
                "Show suggestions on notes.example",
                BadgeCommand::SwitchSiteSuggestions {
                    host: "notes.example".to_owned(),
                    is_on: false,
                }
            ),
        ]
    );
}

#[test]
fn badge_menu_in_a_private_window_dims_the_screen_text_and_the_earlier_writing_of_the_app() {
    let places = chrome_places(PrivateWindowRule::StopSuggestions);
    let mut app = app_with(&places);
    app.on_key(Key::Char('H'), Press::First, &PRIVATE_WINDOW);

    let menu = app
        .badge_menu(
            &BadgeSettings {
                suggestion_places: &places,
                screen_text: &ScreenTextSettings {
                    apps: vec![screen_text_app("com.google.Chrome", "Chrome", true)],
                    ..ScreenTextSettings::default()
                },
                personalization: &PersonalizationSettings {
                    history: HistoryAccess::On,
                    apps: vec![history_app("com.google.Chrome", "Chrome", HistoryScope::AllApps)],
                    ..PersonalizationSettings::default()
                },
            },
            &english(),
        )
        .expect("a key in an open field shows the badge");

    assert_eq!(
        menu.items()
            .map(|item| (item.label.as_str(), item.is_enabled))
            .collect::<Vec<_>>(),
        [
            ("Pause suggestions", true),
            ("Stop suggestions in private windows", true),
            ("Show suggestions in Chrome", true),
            ("Show suggestions on notes.example", true),
            ("Use screen text in Chrome", false),
            ("Off", false),
            ("Only this app", false),
            ("All apps", false),
        ]
    );
}

#[test]
fn badge_menu_in_a_private_window_leaves_out_the_switch_while_the_rule_keeps_suggestions() {
    let places = chrome_places(PrivateWindowRule::KeepSuggestions);
    let mut app = app_with(&places);
    app.on_key(Key::Char('H'), Press::First, &PRIVATE_WINDOW);

    let menu = badge_menu_of(&app, &places);

    assert_eq!(
        menu.items().map(|item| item.label.as_str()).collect::<Vec<_>>(),
        [
            "Pause suggestions",
            "Show suggestions in Chrome",
            "Show suggestions on notes.example"
        ]
    );
}

#[test]
fn badge_menu_in_a_private_window_follows_the_writing_rules() {
    let mut app = App::new();
    app.on_key(Key::Char('H'), Press::First, &PRIVATE_WINDOW);

    let menu = badge_menu_of(&app, &SuggestionPlaces::default());

    let labels: Vec<UiText> = menu
        .items()
        .map(|item| UiText::name(item.label.as_str()))
        .collect();
    assert_eq!(writing_rule_breaks(&labels), Vec::<String>::new());
}
