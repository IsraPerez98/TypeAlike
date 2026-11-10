//! Tests of the languages that the user writes, the dictionaries that check them, and the switch
//! of the typos in the setup.

use crate::controller::Delivery;
use crate::hub::ByteSize;
use crate::settings::{Settings, Setup, SpellingSettings, TypoRule};
use crate::settings_window::languages::ADD_LANGUAGE;
use crate::spelling::{DictionaryEvent, DictionaryRequest};

use super::harness::Harness;

const CHILE: &str = "es-CL";

fn strings(texts: &[&str]) -> Vec<String> {
    texts.iter().map(|text| (*text).to_owned()).collect()
}

fn settings_with(languages: &[&str]) -> Settings {
    Settings {
        spelling: SpellingSettings {
            languages: Some(strings(languages)),
        },
        ..Settings::default()
    }
}

fn deliver(harness: &mut Harness, event: DictionaryEvent) {
    harness
        .controller
        .on_delivery(Delivery::Dictionary(event), &mut harness.shell);
}

#[test]
fn start_opens_the_os_dictionaries_and_lists_the_downloaded_dictionaries() {
    let harness = Harness::new(Settings::default());

    let started = (harness.opened_dictionaries(), harness.dictionary_requests());

    assert_eq!(
        started,
        (vec![strings(&["en-US"])], vec![DictionaryRequest::ListInstalled])
    );
}

#[test]
fn added_language_downloads_its_dictionary_and_keeps_the_list() {
    let mut harness = Harness::new(Settings::default());
    deliver(&mut harness, DictionaryEvent::Installed(Vec::new()));

    harness.pick(ADD_LANGUAGE, 1);

    let saved = harness
        .saved()
        .pop()
        .and_then(|settings| settings.spelling.languages);
    assert_eq!(
        (harness.dictionary_requests().pop(), saved),
        (
            Some(DictionaryRequest::Download {
                language: CHILE.to_owned()
            }),
            Some(strings(&["en-US", CHILE]))
        )
    );
}

#[test]
fn downloaded_dictionary_checks_words_with_the_os_dictionary_after_its_read() {
    let mut harness = Harness::new(settings_with(&["en-US", CHILE]));
    deliver(&mut harness, DictionaryEvent::Installed(Vec::new()));
    deliver(
        &mut harness,
        DictionaryEvent::DownloadFinished {
            language: CHILE.to_owned(),
            result: Ok(ByteSize(835_000)),
        },
    );

    deliver(
        &mut harness,
        DictionaryEvent::Loaded {
            language: CHILE.to_owned(),
            result: Ok(()),
        },
    );

    let opened = harness.opened_dictionaries();
    assert_eq!(
        (harness.dictionary_requests().pop(), &opened[opened.len() - 2..]),
        (
            Some(DictionaryRequest::Load {
                language: CHILE.to_owned()
            }),
            &[strings(&["en-US"]), strings(&[CHILE])][..]
        )
    );
}

#[test]
fn reset_lists_the_languages_of_the_os_again() {
    let mut harness = Harness::new(settings_with(&[CHILE]));
    harness.press_button("reset-settings");

    harness.press_button("reset-settings");

    let saved = harness.saved().pop().map(|settings| settings.spelling);
    assert_eq!(saved, Some(SpellingSettings::default()));
}

#[test]
fn switch_of_the_setup_turns_off_the_rule_for_typos() {
    let mut harness = Harness::new(Settings {
        setup: Setup::Needed,
        ..Settings::default()
    });
    harness.controller.on_launch(&mut harness.shell);

    harness.toggle("setup-typos", false);

    let saved = harness.saved().pop().map(|settings| settings.typo_rule);
    assert_eq!(saved, Some(TypoRule::Suggest));
}
