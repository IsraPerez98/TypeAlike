//! Tests of the settings window and its panes.

use super::harness::{Call, Harness, completed, loaded, model_file, suggestion_settings};
use crate::controller::Delivery;
use crate::desktop::EnergySaver;
use crate::hub::{HubEvent, HubRequest};
use crate::inference::{Backend, InferenceRequest, SentenceEnd};
use crate::key::Key;
use crate::localization::UiLanguage;
use crate::settings::{ClipboardAccess, ScreenTextAccess, ScreenTextSettings, Settings, SuggestionPlaces};
use crate::settings_window::general::{BACKEND, CLIPBOARD, PREPARATION, SUGGESTION_MODEL, WRITER_NAME};
use crate::settings_window::models::SEARCH;
use crate::settings_window::playground::MODEL;
use crate::settings_window::screen_text::SCREEN_TEXT;
use crate::settings_window::view::{ControlId, Row, SettingsInput, SettingsView};
use crate::test_support::{mail_app, suggestion_app, suggestion_site, writing_rule_breaks};

#[test]
fn settings_window_follows_the_writing_rules() {
    let mut harness = Harness::new(Settings::default());

    let view = harness.shown_settings();

    assert_eq!(writing_rule_breaks(&view.texts()), Vec::<String>::new());
}

#[test]
fn settings_window_with_its_warnings_follows_the_writing_rules() {
    let settings = Settings {
        sentence_end: SentenceEnd::Continue,
        ..Settings::default()
    };
    let mut harness = Harness::with_energy_saver(settings, EnergySaver::On);

    let view = harness.shown_settings();

    assert_eq!(writing_rule_breaks(&view.texts()), Vec::<String>::new());
}

#[test]
fn general_change_saves_settings() {
    let mut harness = Harness::new(Settings::default());

    harness.toggle(CLIPBOARD, true);

    assert_eq!(
        harness.saved(),
        vec![Settings {
            clipboard_access: ClipboardAccess::Allowed,
            ..Settings::default()
        }]
    );
}

#[test]
fn general_change_gives_clipboard_access_to_shell() {
    let mut harness = Harness::new(Settings::default());

    harness.toggle(CLIPBOARD, true);

    assert!(
        harness
            .take_calls()
            .contains(&Call::UseClipboardAccess(ClipboardAccess::Allowed))
    );
}

#[test]
fn general_change_applies_to_suggestions() {
    let mut harness = Harness::new(Settings::default());
    harness.toggle(PREPARATION, true);

    harness.settle_input();

    assert!(matches!(
        harness.last_request(),
        Some(InferenceRequest::Complete { .. })
    ));
}

#[test]
fn unchanged_general_setting_saves_nothing() {
    let mut harness = Harness::new(Settings::default());

    harness.toggle(CLIPBOARD, false);

    assert_eq!(harness.saved(), Vec::new());
}

#[test]
fn general_change_renders_settings() {
    let mut harness = Harness::new(Settings::default());

    harness.toggle(CLIPBOARD, true);

    assert!(matches!(
        harness.take_calls().last(),
        Some(Call::RenderSettings(_))
    ));
}

#[test]
fn backend_change_loads_suggestion_model_again() {
    let mut harness = Harness::new(suggestion_settings());
    harness.answer(loaded());

    harness.pick(BACKEND, 1);

    assert_eq!(
        harness.last_request(),
        Some(InferenceRequest::Load {
            file: model_file(),
            backend: Backend::Vulkan,
        })
    );
}

#[test]
fn picking_suggestion_model_loads_it() {
    let mut harness = Harness::new(Settings::default());
    harness.install(model_file());

    harness.pick(SUGGESTION_MODEL, 1);

    assert_eq!(
        harness.last_request(),
        Some(InferenceRequest::Load {
            file: model_file(),
            backend: Backend::Cpu,
        })
    );
}

#[test]
fn pick_that_does_nothing_renders_settings() {
    let mut harness = Harness::new(Settings::default());

    harness.pick(SUGGESTION_MODEL, 0);

    assert!(matches!(
        harness.take_calls().as_slice(),
        [Call::RenderSettings(_)]
    ));
}

#[test]
fn playground_load_keeps_suggestion_model() {
    let mut harness = Harness::with_playground_page(Settings::default());
    harness.install(model_file());
    harness.pick(MODEL, 1);

    harness.answer(loaded());

    assert_eq!(harness.saved(), vec![suggestion_settings()]);
}

#[test]
fn playground_answer_renders_settings() {
    let mut harness = Harness::new(suggestion_settings());

    harness.answer(loaded());

    assert!(matches!(
        harness.take_calls().as_slice(),
        [Call::RenderSettings(_)]
    ));
}

#[test]
fn search_asks_hub() {
    let mut harness = Harness::new(Settings::default());
    let search = SettingsInput::Submitted {
        id: ControlId::new(SEARCH),
        text: "qwen".to_owned(),
    };

    harness.controller.on_settings_input(&search, &mut harness.shell);

    assert_eq!(
        harness.hub_requests(),
        vec![HubRequest::Search {
            query: "qwen".to_owned(),
        }]
    );
}

#[test]
fn hub_answer_renders_settings() {
    let mut harness = Harness::new(Settings::default());

    harness
        .controller
        .on_delivery(Delivery::Hub(HubEvent::Installed(Vec::new())), &mut harness.shell);

    assert!(matches!(
        harness.take_calls().as_slice(),
        [Call::RenderSettings(_)]
    ));
}

#[test]
fn permission_button_asks_for_the_ocr_permission() {
    let mut harness = Harness::new(Settings::default());

    harness.controller.on_settings_input(
        &SettingsInput::Pressed(ControlId::new("screen-text-ocr-permission")),
        &mut harness.shell,
    );

    assert!(harness.take_calls().contains(&Call::AskForOcrPermission));
}

#[test]
fn screen_text_switch_keeps_the_general_settings() {
    let mut harness = Harness::new(Settings {
        clipboard_access: ClipboardAccess::Allowed,
        ..Settings::default()
    });

    harness.toggle(SCREEN_TEXT, false);

    assert_eq!(
        harness.saved(),
        vec![Settings {
            clipboard_access: ClipboardAccess::Allowed,
            screen_text: ScreenTextSettings {
                access: ScreenTextAccess::Denied,
                ..ScreenTextSettings::default()
            },
            ..Settings::default()
        }]
    );
}

#[test]
fn general_setting_keeps_the_screen_text_apps() {
    let mut harness = Harness::new(Settings::default());
    harness.focus_text_field_of("com.apple.mail", "Mail");
    harness.show("est regards");
    harness.settle_input();

    harness.toggle(CLIPBOARD, true);

    assert_eq!(
        harness.saved().pop().map(|settings| settings.screen_text.apps),
        Some(vec![mail_app(true)])
    );
}

#[test]
fn shown_suggestion_adds_the_app_and_the_website_to_the_apps_and_websites_pane() {
    let mut harness = Harness::new(Settings::default());
    harness.focus_text_field_of("com.apple.mail", "Mail");
    harness.shell.surroundings.domain = Some("mail.example".to_owned());
    harness.show("est regards");

    harness.settle_input();

    assert_eq!(
        harness.saved().pop().map(|settings| settings.suggestion_places),
        Some(SuggestionPlaces {
            apps: vec![suggestion_app("com.apple.mail", "Mail", true)],
            sites: vec![suggestion_site("mail.example", true)],
        })
    );
}

#[test]
fn key_without_a_suggestion_adds_no_app_and_no_website() {
    let mut harness = Harness::new(Settings::default());
    harness.focus_text_field_of("com.apple.mail", "Mail");
    harness.shell.surroundings.domain = Some("mail.example".to_owned());
    harness.press(Key::Char('H'));

    harness.settle_input();

    assert_eq!(harness.saved(), Vec::new());
}

#[test]
fn suggestion_without_a_caret_to_show_it_at_adds_no_app_and_no_website() {
    let mut harness = Harness::new(Settings::default());
    harness.focus_text_field_of("game.exe", "Game");
    harness.shell.surroundings.domain = Some("game.example".to_owned());
    harness.shell.is_ghost_shown = false;
    harness.press(Key::Char('w'));
    harness.answer(completed("ith"));
    harness.run_out_caret_checks();

    harness.settle_input();

    assert_eq!(harness.saved(), Vec::new());
}

#[test]
fn typing_while_paused_adds_no_app_and_no_website() {
    let mut harness = Harness::new(Settings::default());
    harness.toggle_pause();
    harness.focus_text_field_of("com.apple.mail", "Mail");
    harness.shell.surroundings.domain = Some("mail.example".to_owned());
    harness.press(Key::Char('H'));

    harness.settle_input();

    assert_eq!(harness.saved(), Vec::new());
}

#[test]
fn website_switch_keeps_its_setting() {
    let mut harness = Harness::new(Settings {
        suggestion_places: SuggestionPlaces {
            sites: vec![suggestion_site("bank.example", true)],
            ..SuggestionPlaces::default()
        },
        ..Settings::default()
    });

    harness.toggle("suggestions-site:bank.example", false);

    assert_eq!(
        harness
            .saved()
            .pop()
            .map(|settings| settings.suggestion_places.sites),
        Some(vec![suggestion_site("bank.example", false)])
    );
}

/// Whether the user can change the setting of the row `id` on the page `title` of `view`.
fn is_row_enabled(view: &SettingsView, title: &str, id: &str) -> Option<bool> {
    let page = view.pages.iter().find(|page| page.title == title)?;
    match page.row(id)? {
        Row::AppSwitch(switch) => Some(switch.is_enabled),
        Row::AppPicker(picker) => Some(picker.is_enabled),
        _ => None,
    }
}

#[test]
fn app_with_suggestions_off_is_dimmed_in_the_screen_text_and_personalization_panes() {
    let mut harness = Harness::new(Settings::default());
    harness.focus_text_field_of("com.apple.mail", "Mail");
    harness.show("est regards");
    harness.toggle("suggestions-app:com.apple.mail", false);

    let view = harness.shown_settings();

    assert_eq!(
        [
            is_row_enabled(&view, "Copied and screen text", "screen-text-app:com.apple.mail"),
            is_row_enabled(&view, "Personalization", "personalization-app:com.apple.mail"),
        ],
        [Some(false), Some(false)]
    );
}

#[test]
fn panes_show_in_the_language_of_the_user() {
    let mut harness = Harness::with_ui_language(Settings::default(), UiLanguage::Spanish);

    let view = harness.shown_settings();

    assert_eq!(
        view.pages
            .iter()
            .map(|page| page.title.as_str())
            .collect::<Vec<_>>(),
        [
            "General",
            "Apps y sitios web",
            "Texto copiado y de la pantalla",
            "Personalización",
            "Ahorro de energía",
            "Modelos"
        ]
    );
}

#[test]
fn panes_show_in_the_order_of_the_navigation() {
    let mut harness = Harness::new(Settings::default());

    let view = harness.shown_settings();

    assert_eq!(
        view.pages
            .iter()
            .map(|page| page.title.as_str())
            .collect::<Vec<_>>(),
        [
            "General",
            "Apps and websites",
            "Copied and screen text",
            "Personalization",
            "Energy saving",
            "Models"
        ]
    );
}

#[test]
fn settings_of_the_general_pane_show_on_the_page_of_their_topic() {
    let mut harness = Harness::new(Settings::default());

    let view = harness.shown_settings();

    let page_of = |id: &str| {
        view.pages
            .iter()
            .find(|page| page.row(id).is_some())
            .map(|page| page.title.as_str())
    };
    assert_eq!(
        [CLIPBOARD, WRITER_NAME, PREPARATION, SUGGESTION_MODEL, BACKEND].map(page_of),
        [
            Some("Copied and screen text"),
            Some("Personalization"),
            Some("Energy saving"),
            Some("Models"),
            Some("Models")
        ]
    );
}

#[test]
fn playground_pane_shows_last_in_a_build_with_the_playground() {
    let mut harness = Harness::with_playground_page(Settings::default());

    let view = harness.shown_settings();

    assert_eq!(
        view.pages.last().map(|page| page.title.as_str()),
        Some("Playground")
    );
}
