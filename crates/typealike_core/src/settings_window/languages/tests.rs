use super::*;
use crate::settings_window::view::Permille;
use crate::test_support::{english, picked, writing_rule_breaks};

const CHILE: &str = "es-CL";

/// A Windows computer with only English, which the user set to English and Spanish of Chile.
fn os() -> OsLanguages {
    OsLanguages {
        preferred: vec!["en-US".to_owned(), "es-CL".to_owned()],
        available: vec!["en-US".to_owned(), "en-GB".to_owned()],
    }
}

fn catalog() -> Vec<DownloadableDictionary> {
    ["en-US", "es", "es-CL", "fr"]
        .into_iter()
        .map(|language| DownloadableDictionary {
            language: language.to_owned(),
            size: ByteSize(835_000),
        })
        .collect()
}

fn strings(texts: &[&str]) -> Vec<String> {
    texts.iter().map(|text| (*text).to_owned()).collect()
}

/// A pane with the `languages` that the user picked, after the hub listed the `installed`
/// dictionaries.
fn pane_with(languages: &[&str], installed: &[&str]) -> LanguagesPane {
    let settings = SpellingSettings {
        languages: Some(strings(languages)),
    };
    let mut pane = LanguagesPane::new(settings, os(), catalog());
    pane.on_event(DictionaryEvent::Installed(strings(installed)));
    pane
}

fn item(pane: &LanguagesPane, title: &str) -> Item {
    pane.languages_section(&english())
        .rows
        .into_iter()
        .find_map(|row| match row {
            Row::Item(item) if item.title == title => Some(item),
            _ => None,
        })
        .expect("the section shows the language")
}

fn add_picker(pane: &LanguagesPane) -> Picker {
    pane.languages_section(&english())
        .rows
        .into_iter()
        .find_map(|row| match row {
            Row::Picker(picker) => Some(picker),
            _ => None,
        })
        .expect("the section has the menu")
}

fn pressed(id: &str) -> SettingsInput {
    SettingsInput::Pressed(ControlId::from(id.to_owned()))
}

fn download(language: &str) -> DictionaryRequest {
    DictionaryRequest::Download {
        language: language.to_owned(),
    }
}

fn load(language: &str) -> DictionaryRequest {
    DictionaryRequest::Load {
        language: language.to_owned(),
    }
}

#[test]
fn languages_follow_the_os_languages_that_the_os_has_a_dictionary_for() {
    let os = OsLanguages {
        preferred: strings(&["es-CL", "ja-JP", "en-US", "en-PH"]),
        available: strings(&["en-US", "es-ES"]),
    };
    let pane = LanguagesPane::new(SpellingSettings::default(), os, catalog());

    let languages = pane.languages();

    assert_eq!(languages, strings(&["es", "en-US"]));
}

#[test]
fn languages_that_the_user_picked_replace_the_os_languages() {
    let pane = pane_with(&[CHILE], &[]);

    let languages = pane.languages();

    assert_eq!(languages, strings(&[CHILE]));
}

#[test]
fn start_lists_the_downloaded_dictionaries_only_when_the_hub_has_dictionaries() {
    let with_hub = LanguagesPane::new(SpellingSettings::default(), os(), catalog());
    let without_hub = LanguagesPane::new(SpellingSettings::default(), os(), Vec::new());

    let requests = (with_hub.start(), without_hub.start());

    assert_eq!(requests, (vec![DictionaryRequest::ListInstalled], Vec::new()));
}

#[test]
fn list_of_the_hub_reads_the_downloaded_dictionary_and_downloads_a_missing_one() {
    let settings = SpellingSettings {
        languages: Some(strings(&["en-US", CHILE, "fr"])),
    };
    let mut pane = LanguagesPane::new(settings, os(), catalog());

    let outcome = pane.on_event(DictionaryEvent::Installed(strings(&[CHILE])));

    assert_eq!(outcome.requests, [load(CHILE), download("fr")]);
}

#[test]
fn list_of_the_hub_downloads_nothing_for_the_languages_of_the_os() {
    let mut pane = LanguagesPane::new(SpellingSettings::default(), os(), catalog());

    let outcome = pane.on_event(DictionaryEvent::Installed(Vec::new()));

    assert_eq!(outcome, LanguagesOutcome::default());
}

#[test]
fn adding_a_language_that_the_os_has_keeps_the_list_and_downloads_nothing() {
    let mut pane = pane_with(&["en-US"], &[]);

    let outcome = pane.on_action(LanguagesAction::Add("en-GB".to_owned()));

    assert_eq!(
        (outcome, pane.languages()),
        (
            LanguagesOutcome {
                is_settings_change: true,
                is_dictionary_change: true,
                requests: Vec::new(),
            },
            strings(&["en-US", "en-GB"])
        )
    );
}

#[test]
fn adding_a_language_that_the_os_lacks_downloads_its_dictionary() {
    let mut pane = pane_with(&["en-US"], &[]);

    let outcome = pane.on_action(LanguagesAction::Add(CHILE.to_owned()));

    let shown = item(&pane, "Español (Chile)");
    assert_eq!(
        (outcome.requests, shown.subtitle, shown.accessory),
        (
            vec![download(CHILE)],
            "0% downloaded".to_owned(),
            Accessory::Downloading {
                progress: Permille(0),
                cancel: Button {
                    id: ControlId::new("language-cancel:es-CL"),
                    label: "Cancel download".to_owned(),
                    is_enabled: true,
                },
            }
        )
    );
}

#[test]
fn adding_a_language_with_a_downloaded_dictionary_reads_it() {
    let mut pane = pane_with(&["en-US"], &[CHILE]);

    let outcome = pane.on_action(LanguagesAction::Add(CHILE.to_owned()));

    assert_eq!(outcome.requests, [load(CHILE)]);
}

#[test]
fn adding_a_language_before_the_hub_listed_its_dictionaries_waits_for_the_list() {
    let settings = SpellingSettings {
        languages: Some(strings(&["en-US"])),
    };
    let mut pane = LanguagesPane::new(settings, os(), catalog());
    pane.on_action(LanguagesAction::Add(CHILE.to_owned()));

    let outcome = pane.on_event(DictionaryEvent::Installed(Vec::new()));

    assert_eq!(outcome.requests, [download(CHILE)]);
}

#[test]
fn finished_download_reads_the_dictionary_and_the_read_dictionary_checks_the_language() {
    let mut pane = pane_with(&["en-US", CHILE], &[]);
    let finished = pane.on_event(DictionaryEvent::DownloadFinished {
        language: CHILE.to_owned(),
        result: Ok(ByteSize(835_000)),
    });

    let loaded = pane.on_event(DictionaryEvent::Loaded {
        language: CHILE.to_owned(),
        result: Ok(()),
    });

    assert_eq!(
        (
            finished.requests,
            loaded.is_dictionary_change,
            pane.dictionaries()
        ),
        (
            vec![load(CHILE)],
            true,
            Dictionaries {
                os: strings(&["en-US"]),
                downloaded: strings(&[CHILE]),
            }
        )
    );
}

#[test]
fn language_without_its_dictionary_yet_uses_the_os_dictionary_of_its_language() {
    let os = OsLanguages {
        preferred: Vec::new(),
        available: strings(&["es-ES"]),
    };
    let settings = SpellingSettings {
        languages: Some(strings(&[CHILE])),
    };
    let mut pane = LanguagesPane::new(settings, os, catalog());
    pane.on_event(DictionaryEvent::Installed(Vec::new()));

    let dictionaries = pane.dictionaries();

    assert_eq!(
        dictionaries,
        Dictionaries {
            os: strings(&["es-ES"]),
            downloaded: Vec::new(),
        }
    );
}

#[test]
fn progress_of_the_download_shows_in_the_item() {
    let mut pane = pane_with(&["en-US", CHILE], &[]);

    pane.on_event(DictionaryEvent::DownloadProgress {
        language: CHILE.to_owned(),
        received: ByteSize(417_500),
    });

    let shown = item(&pane, "Español (Chile)");
    assert_eq!(
        (shown.subtitle, shown.detail),
        ("50% downloaded".to_owned(), "835 KB".to_owned())
    );
}

#[test]
fn failed_download_shows_why_and_a_retry_that_downloads_again() {
    let mut pane = pane_with(&["en-US", CHILE], &[]);
    pane.on_event(DictionaryEvent::DownloadFinished {
        language: CHILE.to_owned(),
        result: Err(HubError::Offline),
    });
    let shown = item(&pane, "Español (Chile)");
    let retry = pane
        .action_for(&pressed("language-retry:es-CL"))
        .expect("the retry acts");

    let outcome = pane.on_action(retry);

    assert_eq!(
        (shown.subtitle, outcome.requests),
        (
            "TypeAlike cannot download the dictionary. Check your internet connection.".to_owned(),
            vec![download(CHILE)]
        )
    );
}

#[test]
fn cancel_of_a_download_removes_the_language_and_stops_the_download() {
    let mut pane = pane_with(&["en-US", CHILE], &[]);
    let cancel = pane
        .action_for(&pressed("language-cancel:es-CL"))
        .expect("the cancel acts");

    let outcome = pane.on_action(cancel);

    assert_eq!(
        (outcome.requests, pane.languages()),
        (
            vec![DictionaryRequest::CancelDownload {
                language: CHILE.to_owned()
            }],
            strings(&["en-US"])
        )
    );
}

#[test]
fn removing_a_language_with_a_downloaded_dictionary_deletes_it() {
    let mut pane = pane_with(&["en-US", CHILE], &[CHILE]);
    let remove = pane
        .action_for(&pressed("language-remove:es-CL"))
        .expect("the remove acts");

    let outcome = pane.on_action(remove);

    assert_eq!(
        (outcome.requests, pane.dictionaries().downloaded),
        (
            vec![DictionaryRequest::Delete {
                language: CHILE.to_owned()
            }],
            Vec::new()
        )
    );
}

#[test]
fn removing_a_language_of_the_os_keeps_the_other_languages() {
    let mut pane = LanguagesPane::new(SpellingSettings::default(), os(), catalog());

    let outcome = pane.on_action(LanguagesAction::Remove("en-US".to_owned()));

    assert_eq!(
        (outcome.is_settings_change, pane.settings().languages.clone()),
        (true, Some(Vec::new()))
    );
}

#[test]
fn menu_offers_the_other_languages_with_a_dictionary_by_name_after_its_first_entry() {
    let pane = pane_with(&["en-US"], &[]);

    let choices = add_picker(&pane).choices;

    assert_eq!(
        choices,
        strings(&[
            "Choose a language",
            "English (United Kingdom)",
            "Español",
            "Español (Chile)",
            "Français"
        ])
    );
}

#[test]
fn pick_in_the_menu_adds_that_language_and_the_first_entry_adds_none() {
    let pane = pane_with(&["en-US"], &[]);

    let actions = [0, 3].map(|index| pane.action_for(&picked(ADD_LANGUAGE, index)));

    assert_eq!(actions, [None, Some(LanguagesAction::Add(CHILE.to_owned()))]);
}

#[test]
fn item_of_a_language_of_the_os_has_a_remove_button_and_no_size() {
    let pane = pane_with(&["en-US"], &[]);

    let shown = item(&pane, "English (United States)");

    assert_eq!(
        shown,
        Item {
            title: "English (United States)".to_owned(),
            subtitle: String::new(),
            detail: String::new(),
            accessory: Accessory::Action(Button {
                id: ControlId::new("language-remove:en-US"),
                label: "Remove".to_owned(),
                is_enabled: true,
            }),
        }
    );
}

#[test]
fn language_without_any_dictionary_tells_that_typealike_cannot_check_it() {
    let pane = LanguagesPane::new(
        SpellingSettings {
            languages: Some(strings(&["ar"])),
        },
        os(),
        catalog(),
    );

    let shown = item(&pane, "العربية");

    assert_eq!(
        shown.subtitle,
        "TypeAlike cannot check the spelling of this language."
    );
}

#[test]
fn description_of_the_menu_tells_about_the_downloads_only_when_the_hub_has_dictionaries() {
    let with_hub = pane_with(&["en-US"], &[]);
    let without_hub = LanguagesPane::new(SpellingSettings::default(), os(), Vec::new());

    let descriptions = [with_hub, without_hub].map(|pane| add_picker(&pane).description);

    assert_eq!(
        descriptions,
        [
            "When this computer has no dictionary for a language, TypeAlike downloads one.",
            "",
        ]
    );
}

#[test]
fn texts_of_the_page_keep_the_writing_rules() {
    let mut pane = pane_with(&["en-US", CHILE, "ar"], &[]);
    pane.on_event(DictionaryEvent::DownloadFinished {
        language: CHILE.to_owned(),
        result: Err(HubError::Storage),
    });

    let shown = pane.page(Vec::new(), &english());

    assert_eq!(writing_rule_breaks(&shown.texts()), Vec::<String>::new());
}
