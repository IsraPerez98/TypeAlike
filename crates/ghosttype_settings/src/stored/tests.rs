//! Tests the conversion of each setting to and from the format of the settings file.

use super::*;
use ghosttype_core::settings::{
    EnergySaverRule, EnergySavingSettings, GameRule, HistoryAccess, HistoryApp, HistoryScope, IdleTimeout,
    KeptTexts, OcrLevel, PersonalizationSettings, ScreenTextAccess, ScreenTextApp, ScreenTextSettings,
    SuggestionApp, SuggestionPlaces, SuggestionSite, WordChoice,
};

fn model_file() -> ModelFile {
    ModelFile {
        repo: "owner/repo".to_owned(),
        name: "model.gguf".to_owned(),
    }
}

fn history_apps_of(json: &str) -> Vec<(String, HistoryScope)> {
    parse(json)
        .unwrap()
        .personalization
        .apps
        .into_iter()
        .map(|app| (app.app_id, app.scope))
        .collect()
}

#[test]
fn backend_reads_from_its_name() {
    let files = [
        r#"{ "backend": "cpu" }"#,
        r#"{ "backend": "vulkan" }"#,
        r#"{ "backend": "cuda" }"#,
        r#"{ "backend": "metal" }"#,
    ];

    let backends = files.map(|json| parse(json).map(|settings| settings.backend).ok());

    assert_eq!(
        backends,
        [
            Some(Backend::Cpu),
            Some(Backend::Vulkan),
            Some(Backend::Cuda),
            Some(Backend::Metal)
        ]
    );
}

#[test]
fn suggestion_model_reads_from_its_repo_and_name() {
    let json = r#"{ "suggestion_model": { "repo": "owner/repo", "name": "model.gguf" } }"#;

    let suggestion_model = parse(json).ok().and_then(|settings| settings.suggestion_model);

    assert_eq!(suggestion_model, Some(model_file()));
}

#[test]
fn writer_name_reads_from_its_text() {
    let json = r#"{ "writer_name": "Sara Novak" }"#;

    let name = parse(json).map(|settings| settings.writer_name).ok();

    assert_eq!(name, Some(WriterName("Sara Novak".to_owned())));
}

#[test]
fn writer_notes_read_from_their_text() {
    let json = r#"{ "writer_notes": "I am a nurse." }"#;

    let notes = parse(json).map(|settings| settings.writer_notes).ok();

    assert_eq!(notes, Some(WriterNotes("I am a nurse.".to_owned())));
}

#[test]
fn missing_writer_notes_are_the_default_notes() {
    let notes = parse("{}").map(|settings| settings.writer_notes).ok();

    assert_eq!(notes, Some(WriterNotes::default()));
}

#[test]
fn empty_writer_notes_stay_empty() {
    let json = r#"{ "writer_notes": "" }"#;

    let notes = parse(json).map(|settings| settings.writer_notes).ok();

    assert_eq!(notes, Some(WriterNotes(String::new())));
}

#[test]
fn system_prompt_of_an_older_version_becomes_the_writer_notes() {
    let files = [
        r#"{ "system_prompt": "Formal Spanish." }"#,
        r#"{ "system_prompt": "Formal Spanish.", "writer_notes": "" }"#,
    ];

    let notes = files.map(|json| parse(json).ok().map(|settings| settings.writer_notes.0));

    assert_eq!(
        notes.each_ref().map(Option::as_deref),
        [Some("Formal Spanish."); 2]
    );
}

#[test]
fn writer_notes_win_over_an_older_system_prompt() {
    let json = r#"{ "system_prompt": "Formal Spanish.", "writer_notes": "I am a nurse." }"#;

    let notes = parse(json).map(|settings| settings.writer_notes).ok();

    assert_eq!(notes, Some(WriterNotes("I am a nurse.".to_owned())));
}

#[test]
fn saved_file_has_no_system_prompt() {
    let json = to_json(&Settings {
        writer_notes: WriterNotes("Formal Spanish.".to_owned()),
        ..Settings::default()
    });

    assert!(!json.contains("system_prompt"), "{json}");
}

#[test]
fn clipboard_access_reads_from_its_name() {
    let files = [
        r#"{ "clipboard_access": "denied" }"#,
        r#"{ "clipboard_access": "allowed" }"#,
    ];

    let accesses = files.map(|json| parse(json).map(|settings| settings.clipboard_access).ok());

    assert_eq!(
        accesses,
        [Some(ClipboardAccess::Denied), Some(ClipboardAccess::Allowed)]
    );
}

#[test]
fn screen_text_reads_its_switch_and_its_apps() {
    let json = r#"{ "screen_text": { "access": "denied", "apps": [
            { "app_id": "slack.exe", "app_name": "Slack", "app_path": "C:\\Slack\\slack.exe", "is_on": false },
            { "app_id": "com.apple.mail", "app_name": "Mail", "is_on": true }
        ] } }"#;

    let screen_text = parse(json).map(|settings| settings.screen_text).ok();

    assert_eq!(
        screen_text,
        Some(ScreenTextSettings {
            access: ScreenTextAccess::Denied,
            apps: vec![
                ScreenTextApp {
                    app_id: "slack.exe".to_owned(),
                    app_name: "Slack".to_owned(),
                    app_path: Some("C:\\Slack\\slack.exe".to_owned()),
                    is_on: false,
                },
                ScreenTextApp {
                    app_id: "com.apple.mail".to_owned(),
                    app_name: "Mail".to_owned(),
                    app_path: None,
                    is_on: true,
                },
            ],
            ..ScreenTextSettings::default()
        })
    );
}

#[test]
fn settings_file_without_an_ocr_level_reads_as_fast() {
    let json = r#"{ "screen_text": { "access": "allowed", "apps": [] } }"#;

    let level = parse(json).map(|settings| settings.screen_text.ocr_level).ok();

    assert_eq!(level, Some(OcrLevel::Fast));
}

#[test]
fn ocr_level_survives_a_save_and_a_load() {
    let settings = Settings {
        screen_text: ScreenTextSettings {
            ocr_level: OcrLevel::Accurate,
            ..ScreenTextSettings::default()
        },
        ..Settings::default()
    };

    let loaded = parse(&to_json(&settings))
        .map(|loaded| loaded.screen_text.ocr_level)
        .ok();

    assert_eq!(loaded, Some(OcrLevel::Accurate));
}

#[test]
fn suggestion_places_read_their_apps_and_websites() {
    let json = r#"{ "suggestion_places": {
            "apps": [
                { "app_id": "slack.exe", "app_name": "Slack", "app_path": "C:\\Slack\\slack.exe", "is_on": false },
                { "app_id": "com.apple.mail", "app_name": "Mail", "is_on": true }
            ],
            "sites": [{ "host": "bank.example", "is_on": false }]
        } }"#;

    let places = parse(json).map(|settings| settings.suggestion_places).ok();

    assert_eq!(
        places,
        Some(SuggestionPlaces {
            apps: vec![
                SuggestionApp {
                    app_id: "slack.exe".to_owned(),
                    app_name: "Slack".to_owned(),
                    app_path: Some("C:\\Slack\\slack.exe".to_owned()),
                    is_on: false,
                },
                SuggestionApp {
                    app_id: "com.apple.mail".to_owned(),
                    app_name: "Mail".to_owned(),
                    app_path: None,
                    is_on: true,
                },
            ],
            sites: vec![SuggestionSite {
                host: "bank.example".to_owned(),
                is_on: false,
            }],
        })
    );
}

#[test]
fn settings_file_without_suggestion_places_shows_suggestions_everywhere() {
    let files = [
        r#"{ "screen_text": { "access": "allowed", "apps": [] } }"#,
        r#"{ "suggestion_places": { "apps": [] } }"#,
    ];

    let places = files.map(|json| parse(json).map(|settings| settings.suggestion_places).ok());

    assert_eq!(
        places,
        [
            Some(SuggestionPlaces::default()),
            Some(SuggestionPlaces::default())
        ]
    );
}

#[test]
fn suggestion_places_survive_a_save_and_a_load() {
    let places = SuggestionPlaces {
        apps: vec![SuggestionApp {
            app_id: "com.apple.mail".to_owned(),
            app_name: "Mail".to_owned(),
            app_path: None,
            is_on: false,
        }],
        sites: vec![SuggestionSite {
            host: "bank.example".to_owned(),
            is_on: false,
        }],
    };
    let settings = Settings {
        suggestion_places: places.clone(),
        ..Settings::default()
    };

    let loaded = parse(&to_json(&settings))
        .map(|loaded| loaded.suggestion_places)
        .ok();

    assert_eq!(loaded, Some(places));
}

#[test]
fn settings_file_without_personalization_reads_with_the_history_off() {
    let json = r#"{ "clipboard_access": "allowed" }"#;

    let personalization = parse(json).map(|settings| settings.personalization).ok();

    assert_eq!(personalization, Some(PersonalizationSettings::default()));
}

#[test]
fn personalization_reads_from_its_names() {
    let json = r#"{ "personalization": { "history": "on", "kept_texts": "accepted_only", "word_choice": "light" } }"#;

    let personalization = parse(json).map(|settings| settings.personalization).ok();

    assert_eq!(
        personalization,
        Some(PersonalizationSettings {
            history: HistoryAccess::On,
            kept_texts: KeptTexts::AcceptedOnly,
            apps: Vec::new(),
            word_choice: WordChoice::Light,
        })
    );
}

#[test]
fn history_app_reads_its_scope_from_its_name() {
    let json = r#"{ "personalization": { "apps": [
            { "app_id": "a.exe", "app_name": "A", "scope": "off" },
            { "app_id": "b.exe", "app_name": "B", "scope": "this_app" },
            { "app_id": "c.exe", "app_name": "C", "scope": "all_apps" }
        ] } }"#;

    let apps = history_apps_of(json);

    assert_eq!(
        apps,
        [
            ("a.exe".to_owned(), HistoryScope::Off),
            ("b.exe".to_owned(), HistoryScope::ThisApp),
            ("c.exe".to_owned(), HistoryScope::AllApps),
        ]
    );
}

#[test]
fn history_app_of_an_older_file_reads_its_switch_as_off_or_all_apps() {
    let json = r#"{ "personalization": { "apps": [
            { "app_id": "a.exe", "app_name": "A", "is_on": false },
            { "app_id": "b.exe", "app_name": "B", "is_on": true }
        ] } }"#;

    let apps = history_apps_of(json);

    assert_eq!(
        apps,
        [
            ("a.exe".to_owned(), HistoryScope::Off),
            ("b.exe".to_owned(), HistoryScope::AllApps),
        ]
    );
}

#[test]
fn saved_history_app_has_a_scope_and_no_switch() {
    let settings = Settings {
        personalization: PersonalizationSettings {
            apps: vec![HistoryApp {
                app_id: "a.exe".to_owned(),
                app_name: "A".to_owned(),
                app_path: None,
                scope: HistoryScope::ThisApp,
            }],
            ..PersonalizationSettings::default()
        },
        ..Settings::default()
    };

    let json = to_json(&settings);

    assert!(json.contains(r#""scope": "this_app""#) && !json.contains("is_on"));
}

#[test]
fn prompt_preparation_reads_from_its_name() {
    let files = [
        r#"{ "prompt_preparation": "on_first_key" }"#,
        r#"{ "prompt_preparation": "on_focus" }"#,
    ];

    let preparations = files.map(|json| parse(json).map(|settings| settings.prompt_preparation).ok());

    assert_eq!(
        preparations,
        [
            Some(PromptPreparation::OnFirstKey),
            Some(PromptPreparation::OnFocus)
        ]
    );
}

#[test]
fn suggestion_display_reads_from_its_name() {
    let files = [
        r#"{ "suggestion_display": "when_complete" }"#,
        r#"{ "suggestion_display": "word_by_word" }"#,
    ];

    let displays = files.map(|json| parse(json).map(|settings| settings.suggestion_display).ok());

    assert_eq!(
        displays,
        [
            Some(SuggestionDisplay::WhenComplete),
            Some(SuggestionDisplay::WordByWord)
        ]
    );
}

#[test]
fn typo_rule_reads_from_its_name() {
    let files = [
        r#"{ "typo_rule": "hide_suggestions" }"#,
        r#"{ "typo_rule": "suggest" }"#,
    ];

    let rules = files.map(|json| parse(json).map(|settings| settings.typo_rule).ok());

    assert_eq!(rules, [Some(TypoRule::HideSuggestions), Some(TypoRule::Suggest)]);
}

#[test]
fn typo_fix_reads_from_its_name() {
    let files = [r#"{ "typo_fix": "offer" }"#, r#"{ "typo_fix": "off" }"#];

    let typo_fixes = files.map(|json| parse(json).map(|settings| settings.typo_fix).ok());

    assert_eq!(typo_fixes, [Some(TypoFix::Offer), Some(TypoFix::Off)]);
}

#[test]
fn file_without_a_typo_fix_offers_fixes() {
    let fix = parse("{}").map(|settings| settings.typo_fix).ok();

    assert_eq!(fix, Some(TypoFix::Offer));
}

#[test]
fn file_without_a_typo_rule_hides_suggestions_on_typos() {
    let rule = parse("{}").map(|settings| settings.typo_rule).ok();

    assert_eq!(rule, Some(TypoRule::HideSuggestions));
}

#[test]
fn sentence_end_reads_from_its_name() {
    let files = [
        r#"{ "sentence_end": "continue" }"#,
        r#"{ "sentence_end": "stop" }"#,
    ];

    let ends = files.map(|json| parse(json).map(|settings| settings.sentence_end).ok());

    assert_eq!(ends, [Some(SentenceEnd::Continue), Some(SentenceEnd::Stop)]);
}

#[test]
fn file_without_a_sentence_end_stops_at_the_end_of_a_sentence() {
    let end = parse("{}").map(|settings| settings.sentence_end).ok();

    assert_eq!(end, Some(SentenceEnd::Stop));
}

#[test]
fn suggestion_length_reads_from_its_name() {
    let files = [
        r#"{ "suggestion_length": "very_short" }"#,
        r#"{ "suggestion_length": "short" }"#,
        r#"{ "suggestion_length": "medium" }"#,
        r#"{ "suggestion_length": "long" }"#,
        r#"{ "suggestion_length": "very_long" }"#,
    ];

    let lengths = files.map(|json| parse(json).map(|settings| settings.suggestion_length).ok());

    assert_eq!(
        lengths,
        [
            SuggestionLength::VeryShort,
            SuggestionLength::Short,
            SuggestionLength::Medium,
            SuggestionLength::Long,
            SuggestionLength::VeryLong,
        ]
        .map(Some)
    );
}

#[test]
fn idle_timeout_reads_from_its_name() {
    let files = [
        r#"{ "energy_saving": { "idle_timeout": "off" } }"#,
        r#"{ "energy_saving": { "idle_timeout": "fifteen_minutes" } }"#,
        r#"{ "energy_saving": { "idle_timeout": "thirty_minutes" } }"#,
        r#"{ "energy_saving": { "idle_timeout": "sixty_minutes" } }"#,
    ];

    let timeouts = files.map(|json| {
        parse(json)
            .map(|settings| settings.energy_saving.idle_timeout)
            .ok()
    });

    assert_eq!(
        timeouts,
        [
            IdleTimeout::Off,
            IdleTimeout::FifteenMinutes,
            IdleTimeout::ThirtyMinutes,
            IdleTimeout::SixtyMinutes,
        ]
        .map(Some)
    );
}

#[test]
fn energy_saver_rule_reads_from_its_name() {
    let files = [
        r#"{ "energy_saving": { "energy_saver": "stop_suggestions" } }"#,
        r#"{ "energy_saving": { "energy_saver": "keep_suggestions" } }"#,
    ];

    let rules = files.map(|json| {
        parse(json)
            .map(|settings| settings.energy_saving.energy_saver)
            .ok()
    });

    assert_eq!(
        rules,
        [
            Some(EnergySaverRule::StopSuggestions),
            Some(EnergySaverRule::KeepSuggestions)
        ]
    );
}

#[test]
fn game_rule_reads_from_its_name() {
    let files = [
        r#"{ "energy_saving": { "game": "stop_suggestions" } }"#,
        r#"{ "energy_saving": { "game": "keep_suggestions" } }"#,
    ];

    let rules = files.map(|json| parse(json).map(|settings| settings.energy_saving.game).ok());

    assert_eq!(
        rules,
        [Some(GameRule::StopSuggestions), Some(GameRule::KeepSuggestions)]
    );
}

#[test]
fn file_without_the_game_rule_stops_during_games() {
    let rule = parse(r#"{ "energy_saving": { "idle_timeout": "off" } }"#)
        .map(|settings| settings.energy_saving.game)
        .ok();

    assert_eq!(rule, Some(GameRule::StopSuggestions));
}

#[test]
fn file_without_energy_saving_stops_after_30_minutes_and_in_energy_saver() {
    let energy_saving = parse("{}").map(|settings| settings.energy_saving).ok();

    assert_eq!(energy_saving, Some(EnergySavingSettings::default()));
}

#[test]
fn energy_saving_survives_a_save_and_a_load() {
    let energy_saving = EnergySavingSettings {
        idle_timeout: IdleTimeout::SixtyMinutes,
        energy_saver: EnergySaverRule::KeepSuggestions,
        game: GameRule::KeepSuggestions,
    };
    let settings = Settings {
        energy_saving,
        ..Settings::default()
    };

    let loaded = parse(&to_json(&settings)).map(|loaded| loaded.energy_saving).ok();

    assert_eq!(loaded, Some(energy_saving));
}

#[test]
fn missing_field_takes_its_default() {
    let settings = parse("{}");

    assert_eq!(settings.ok(), Some(Settings::default()));
}

#[test]
fn unknown_backend_is_an_error() {
    let settings = parse(r#"{ "backend": "quantum" }"#);

    assert!(settings.is_err());
}
