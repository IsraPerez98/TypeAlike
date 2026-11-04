//! The tests of the Playground pane.

use std::time::Duration;

use super::*;
use crate::inference::{LogProbability, Prompt};
use crate::settings_window::view::Message;
use crate::test_support::{UiText, english, model_file, picked, writing_rule_breaks};

fn loading_note() -> Note {
    Note {
        text: "Starting the model…".to_owned(),
        tone: NoteTone::Busy,
    }
}

fn paused_note() -> Note {
    Note {
        text: "Suggestions are paused. To resume them, select “Resume suggestions” in the TypeAlike menu."
            .to_owned(),
        tone: NoteTone::Plain,
    }
}

fn energy_saver_note() -> Note {
    Note {
        text: "Suggestions are off while the computer saves energy. To use them now, turn off “Stop in energy saver”.".to_owned(),
        tone: NoteTone::Plain,
    }
}

fn idle_note() -> Note {
    Note {
        text: "Suggestions stopped to free memory, because you did not type or click for some time. They start again when you type in a text field.".to_owned(),
        tone: NoteTone::Plain,
    }
}

fn game_note() -> Note {
    Note {
        text:
            "Suggestions stopped to free memory for a game. They start again when you type in a text field."
                .to_owned(),
        tone: NoteTone::Plain,
    }
}

fn full_screen_note() -> Note {
    Note {
        text: "Suggestions stopped to free memory for an app in full screen. They start again when you type in a text field.".to_owned(),
        tone: NoteTone::Plain,
    }
}

fn prompt(text: &str) -> Prompt {
    prompt::framed_prompt(&[], text)
}

fn complete(text: &str) -> PlaygroundAction {
    PlaygroundAction::Complete(text.to_owned())
}

fn titles(titles: &[&str]) -> Vec<String> {
    titles.iter().map(|title| (*title).to_owned()).collect()
}

fn ready_pane(file: &ModelFile) -> PlaygroundPane {
    let mut pane = PlaygroundPane::new();
    pane.on_action(PlaygroundAction::SelectModel(file.clone()));
    pane.on_event(InferenceEvent::Loaded {
        file: file.clone(),
        result: Ok(()),
    });
    pane
}

fn completing_pane(file: &ModelFile) -> PlaygroundPane {
    let mut pane = ready_pane(file);
    pane.on_action(complete("Dear"));
    pane
}

fn completion(text: &str, tokens: u32, duration: Duration) -> Completion {
    Completion {
        text: text.to_owned(),
        tokens: TokenCount(tokens),
        duration,
        word_log_probabilities: Vec::new(),
        typed_log_probability: LogProbability::default(),
        correction: None,
    }
}

fn model_menu(page: &Page) -> Picker {
    match page.row(MODEL) {
        Some(Row::Picker(picker)) => picker.clone(),
        other => panic!("the page shows no model menu: {other:?}"),
    }
}

fn can_complete(page: &Page) -> bool {
    match page.row(PROMPT) {
        Some(Row::TextField(text_field)) => text_field.is_submit_enabled,
        other => panic!("the page shows no prompt field: {other:?}"),
    }
}

fn shown_completion(page: &Page) -> Option<Section> {
    page.sections.get(2).cloned()
}

fn completion_section(row: Row) -> Section {
    Section {
        heading: Some("Suggestion".to_owned()),
        ..Section::new(vec![row])
    }
}

fn completed_output(text: &str, tokens: u32, duration: Duration) -> Option<Section> {
    let file = model_file("a.gguf");
    let mut pane = completing_pane(&file);
    pane.on_event(InferenceEvent::Completed(Ok(completion(text, tokens, duration))));
    shown_completion(&pane.page(&[file], None, &english()))
}

fn submitted(text: &str) -> SettingsInput {
    SettingsInput::Submitted {
        id: ControlId::new(PROMPT),
        text: text.to_owned(),
    }
}

#[test]
fn pane_without_downloads_asks_for_one() {
    let pane = PlaygroundPane::new();

    let shown = pane.page(&[], None, &english());

    assert_eq!(
        shown,
        Page {
            title: "Playground".to_owned(),
            icon: PageIcon::TextCursor,
            summary: "Select a model, then type the start of a sentence to see what the model suggests.".to_owned(),
            search: None,
            sections: vec![
                Section::new(vec![Row::Picker(Picker {
                    id: ControlId::new(MODEL),
                    icon: RowIcon::Robot,
                    label: "Suggestion model".to_owned(),
                    description: String::new(),
                    choices: titles(&["No downloaded models"]),
                    choice_text: ChoiceText::Name,
                    selected: 0,
                    is_enabled: false,
                    note: Some(Note {
                        text: "TypeAlike needs a model to write suggestions. Download the recommended model below.".to_owned(),
                        tone: NoteTone::Plain,
                    }),
                })]),
                Section::new(vec![Row::TextField(TextField {
                    id: ControlId::new(PROMPT),
                    icon: RowIcon::Pencil,
                    label: "Text".to_owned(),
                    placeholder: "Type the start of a sentence".to_owned(),
                    submit_label: "Show suggestion".to_owned(),
                    is_submit_enabled: false,
                })]),
            ],
        }
    );
}

#[test]
fn downloaded_models_follow_a_placeholder_until_one_is_selected() {
    let pane = PlaygroundPane::new();

    let shown = pane.page(
        &[model_file("first.gguf"), model_file("second.gguf")],
        None,
        &english(),
    );

    let menu = model_menu(&shown);
    assert_eq!(
        (
            menu.choices,
            menu.selected,
            menu.is_enabled,
            menu.note,
            can_complete(&shown)
        ),
        (
            titles(&["Select a model", "first", "second"]),
            0,
            true,
            None,
            false
        )
    );
}

#[test]
fn picking_a_model_selects_it() {
    let file = model_file("a.gguf");
    let pane = PlaygroundPane::new();

    let action = pane.action_for(&picked(MODEL, 1), std::slice::from_ref(&file));

    assert_eq!(action, Some(PlaygroundAction::SelectModel(file)));
}

#[test]
fn picking_the_placeholder_model_does_nothing() {
    let pane = PlaygroundPane::new();

    let action = pane.action_for(&picked(MODEL, 0), &[model_file("a.gguf")]);

    assert_eq!(action, None);
}

#[test]
fn submitting_the_prompt_completes_its_text() {
    let file = model_file("a.gguf");
    let pane = ready_pane(&file);

    let action = pane.action_for(&submitted("Dear"), &[file]);

    assert_eq!(action, Some(complete("Dear")));
}

#[test]
fn submitting_the_prompt_before_the_model_is_ready_does_nothing() {
    let file = model_file("a.gguf");
    let mut pane = PlaygroundPane::new();
    pane.on_action(PlaygroundAction::SelectModel(file.clone()));

    let action = pane.action_for(&submitted("Dear"), &[file]);

    assert_eq!(action, None);
}

#[test]
fn input_for_a_control_of_another_pane_is_not_for_this_pane() {
    let pane = PlaygroundPane::new();
    let input = SettingsInput::Picked {
        id: ControlId::new("suggestion-length"),
        index: 0,
    };

    let action = pane.action_for(&input, &[model_file("a.gguf")]);

    assert_eq!(action, None);
}

#[test]
fn selecting_a_model_loads_it() {
    let file = model_file("a.gguf");
    let mut pane = PlaygroundPane::new();

    let request = pane.on_action(PlaygroundAction::SelectModel(file.clone()));

    assert_eq!(
        request,
        Some(InferenceRequest::Load {
            file,
            backend: Backend::Cpu
        })
    );
}

#[test]
fn pane_with_a_backend_loads_models_on_it() {
    let file = model_file("a.gguf");
    let mut pane = PlaygroundPane::with_backend(Backend::Vulkan);

    let request = pane.on_action(PlaygroundAction::SelectModel(file.clone()));

    assert_eq!(
        request,
        Some(InferenceRequest::Load {
            file,
            backend: Backend::Vulkan,
        })
    );
}

#[test]
fn changing_the_backend_loads_the_selected_model_again() {
    let file = model_file("a.gguf");
    let mut pane = ready_pane(&file);

    let request = pane.change_backend(Backend::Cuda);

    assert_eq!(
        (
            request,
            model_menu(&pane.page(std::slice::from_ref(&file), None, &english())).note
        ),
        (
            Some(InferenceRequest::Load {
                file,
                backend: Backend::Cuda,
            }),
            Some(loading_note()),
        )
    );
}

#[test]
fn changing_the_backend_loads_a_failed_model_again() {
    let file = model_file("a.gguf");
    let mut pane = PlaygroundPane::with_backend(Backend::Vulkan);
    pane.on_action(PlaygroundAction::SelectModel(file.clone()));
    pane.on_event(InferenceEvent::Loaded {
        file: file.clone(),
        result: Err(InferenceError::ModelTooLarge),
    });

    let request = pane.change_backend(Backend::Cpu);

    assert_eq!(
        request,
        Some(InferenceRequest::Load {
            file,
            backend: Backend::Cpu,
        })
    );
}

#[test]
fn changing_the_backend_without_a_model_loads_nothing() {
    let mut pane = PlaygroundPane::new();

    let request = pane.change_backend(Backend::Vulkan);

    assert_eq!(request, None);
}

#[test]
fn models_selected_after_a_backend_change_load_on_it() {
    let file = model_file("a.gguf");
    let mut pane = PlaygroundPane::new();
    pane.change_backend(Backend::Vulkan);

    let request = pane.on_action(PlaygroundAction::SelectModel(file.clone()));

    assert_eq!(
        request,
        Some(InferenceRequest::Load {
            file,
            backend: Backend::Vulkan,
        })
    );
}

#[test]
fn changing_the_backend_clears_the_output() {
    let file = model_file("a.gguf");
    let mut pane = completing_pane(&file);
    pane.on_event(InferenceEvent::Completed(Ok(completion(
        " Sir",
        1,
        Duration::ZERO,
    ))));

    pane.change_backend(Backend::Vulkan);

    assert_eq!(shown_completion(&pane.page(&[file], None, &english())), None);
}

#[test]
fn pane_is_busy_while_a_model_loads() {
    let mut pane = PlaygroundPane::new();
    pane.on_action(PlaygroundAction::SelectModel(model_file("a.gguf")));

    assert!(pane.is_busy());
}

#[test]
fn pane_is_busy_while_text_completes() {
    let pane = completing_pane(&model_file("a.gguf"));

    assert!(pane.is_busy());
}

#[test]
fn pane_with_a_loaded_model_is_not_busy() {
    let pane = ready_pane(&model_file("a.gguf"));

    assert!(!pane.is_busy());
}

#[test]
fn loading_model_shows_status_and_locks_the_menu() {
    let second = model_file("second.gguf");
    let mut pane = PlaygroundPane::new();
    pane.on_action(PlaygroundAction::SelectModel(second.clone()));

    let shown = pane.page(&[model_file("first.gguf"), second], None, &english());

    let menu = model_menu(&shown);
    assert_eq!(
        (
            menu.choices,
            menu.selected,
            menu.is_enabled,
            menu.note,
            can_complete(&shown)
        ),
        (
            titles(&["first", "second"]),
            1,
            false,
            Some(loading_note()),
            false
        )
    );
}

#[test]
fn loaded_model_can_complete_text() {
    let file = model_file("a.gguf");
    let pane = ready_pane(&file);

    let shown = pane.page(&[file], None, &english());

    let menu = model_menu(&shown);
    assert_eq!(
        (menu.is_enabled, menu.note, can_complete(&shown)),
        (true, None, true)
    );
}

#[test]
fn failed_load_tells_why() {
    let file = model_file("a.gguf");
    let mut pane = PlaygroundPane::new();
    pane.on_action(PlaygroundAction::SelectModel(file.clone()));
    pane.on_event(InferenceEvent::Loaded {
        file: file.clone(),
        result: Err(InferenceError::ModelInvalid),
    });

    let shown = pane.page(&[file], None, &english());

    let menu = model_menu(&shown);
    assert_eq!(
        (menu.is_enabled, menu.note, can_complete(&shown)),
        (
            true,
            Some(Note {
                text: "TypeAlike cannot use this model. The file can be damaged, or of a type that TypeAlike does not support. Download it again, or select another model.".to_owned(),
                tone: NoteTone::Error,
            }),
            false,
        )
    );
}

#[test]
fn failed_model_can_be_selected_again() {
    let file = model_file("a.gguf");
    let mut pane = PlaygroundPane::new();
    pane.on_action(PlaygroundAction::SelectModel(file.clone()));
    pane.on_event(InferenceEvent::Loaded {
        file: file.clone(),
        result: Err(InferenceError::ModelMissing),
    });

    let request = pane.on_action(PlaygroundAction::SelectModel(file.clone()));

    assert_eq!(
        request,
        Some(InferenceRequest::Load {
            file,
            backend: Backend::Cpu
        })
    );
}

#[test]
fn load_result_of_another_file_is_ignored() {
    let file = model_file("a.gguf");
    let mut pane = PlaygroundPane::new();
    pane.on_action(PlaygroundAction::SelectModel(file.clone()));

    pane.on_event(InferenceEvent::Loaded {
        file: model_file("b.gguf"),
        result: Ok(()),
    });

    assert_eq!(
        model_menu(&pane.page(&[file], None, &english())).note,
        Some(loading_note())
    );
}

#[test]
fn selecting_the_loaded_model_again_does_nothing() {
    let file = model_file("a.gguf");
    let mut pane = ready_pane(&file);

    let request = pane.on_action(PlaygroundAction::SelectModel(file));

    assert_eq!(request, None);
}

#[test]
fn selecting_while_a_model_loads_does_nothing() {
    let mut pane = PlaygroundPane::new();
    pane.on_action(PlaygroundAction::SelectModel(model_file("a.gguf")));

    let request = pane.on_action(PlaygroundAction::SelectModel(model_file("b.gguf")));

    assert_eq!(request, None);
}

#[test]
fn selecting_another_loaded_model_loads_it() {
    let other = model_file("b.gguf");
    let mut pane = ready_pane(&model_file("a.gguf"));

    let request = pane.on_action(PlaygroundAction::SelectModel(other.clone()));

    assert_eq!(
        request,
        Some(InferenceRequest::Load {
            file: other,
            backend: Backend::Cpu,
        })
    );
}

#[test]
fn unloading_the_loaded_model_asks_the_runner_to_unload_it() {
    let mut pane = ready_pane(&model_file("a.gguf"));

    let request = pane.unload_model(StopReason::Paused);

    assert_eq!(request, Some(InferenceRequest::Unload));
}

#[test]
fn unloading_without_a_model_does_nothing() {
    let mut pane = PlaygroundPane::new();

    let request = pane.unload_model(StopReason::Paused);

    assert_eq!(request, None);
}

#[test]
fn unloaded_model_stays_selected_and_tells_that_suggestions_are_paused() {
    let file = model_file("a.gguf");
    let mut pane = ready_pane(&file);

    pane.unload_model(StopReason::Paused);

    let shown = pane.page(std::slice::from_ref(&file), None, &english());
    let menu = model_menu(&shown);
    assert_eq!(
        (
            pane.loaded_model(),
            menu.selected,
            menu.note,
            can_complete(&shown)
        ),
        (None, 0, Some(paused_note()), false)
    );
}

#[test]
fn unloading_an_unloaded_model_sends_no_request() {
    let mut pane = ready_pane(&model_file("a.gguf"));
    pane.unload_model(StopReason::Paused);

    let request = pane.unload_model(StopReason::EnergySaver);

    assert_eq!(request, None);
}

#[test]
fn unloaded_model_tells_why_it_is_unloaded() {
    let file = model_file("a.gguf");
    let reasons = [
        StopReason::Paused,
        StopReason::EnergySaver,
        StopReason::Game,
        StopReason::FullScreen,
        StopReason::Idle,
    ];

    let notes = reasons.map(|reason| {
        let mut pane = ready_pane(&file);
        pane.unload_model(reason);
        model_menu(&pane.page(std::slice::from_ref(&file), None, &english())).note
    });

    assert_eq!(
        notes,
        [
            paused_note(),
            energy_saver_note(),
            game_note(),
            full_screen_note(),
            idle_note()
        ]
        .map(Some)
    );
}

#[test]
fn second_unload_tells_its_own_reason() {
    let file = model_file("a.gguf");
    let mut pane = ready_pane(&file);
    pane.unload_model(StopReason::Idle);

    pane.unload_model(StopReason::Paused);

    let shown = pane.page(std::slice::from_ref(&file), None, &english());
    assert_eq!(model_menu(&shown).note, Some(paused_note()));
}

#[test]
fn model_selected_without_a_load_shows_as_unloaded() {
    let file = model_file("a.gguf");
    let mut pane = PlaygroundPane::new();

    pane.select_unloaded_model(file.clone(), StopReason::EnergySaver);

    let shown = pane.page(std::slice::from_ref(&file), None, &english());
    let menu = model_menu(&shown);
    assert_eq!(
        (pane.loaded_model(), menu.selected, menu.note),
        (None, 0, Some(energy_saver_note()))
    );
}

#[test]
fn selecting_without_a_load_keeps_the_selected_model() {
    let file = model_file("a.gguf");
    let mut pane = ready_pane(&file);

    pane.select_unloaded_model(model_file("b.gguf"), StopReason::EnergySaver);

    assert_eq!(pane.loaded_model(), Some(&file));
}

#[test]
fn selecting_the_unloaded_model_loads_it_again() {
    let file = model_file("a.gguf");
    let mut pane = ready_pane(&file);
    pane.unload_model(StopReason::Paused);

    let request = pane.on_action(PlaygroundAction::SelectModel(file.clone()));

    assert_eq!(
        request,
        Some(InferenceRequest::Load {
            file,
            backend: Backend::Cpu
        })
    );
}

#[test]
fn load_that_ends_after_an_unload_is_ignored() {
    let file = model_file("a.gguf");
    let mut pane = PlaygroundPane::new();
    pane.on_action(PlaygroundAction::SelectModel(file.clone()));
    pane.unload_model(StopReason::Paused);

    pane.on_event(InferenceEvent::Loaded { file, result: Ok(()) });

    assert_eq!(pane.loaded_model(), None);
}

#[test]
fn changing_the_backend_does_not_load_an_unloaded_model() {
    let mut pane = ready_pane(&model_file("a.gguf"));
    pane.unload_model(StopReason::Paused);

    let request = pane.change_backend(Backend::Vulkan);

    assert_eq!(request, None);
}

#[test]
fn completing_sends_the_prompt_with_a_token_limit() {
    let mut pane = ready_pane(&model_file("a.gguf"));

    let request = pane.on_action(complete("Dear "));

    assert_eq!(
        request,
        Some(InferenceRequest::Complete {
            prompt: prompt("Dear "),
            max_tokens: TokenCount(32),
            word_bias: None,
            next_word_bias: NextWordBias::default(),
            min_log_probability: None,
            typo_check: TypoCheck::Off,
            sentence_end: SentenceEnd::Stop,
            line_after: String::new(),
        })
    );
}

#[test]
fn completing_puts_the_writer_name_after_the_notes() {
    let mut pane = ready_pane(&model_file("a.gguf"));
    pane.use_settings(&Settings {
        writer_name: WriterName("Sara Novak".to_owned()),
        ..Settings::default()
    });

    let request = pane.on_action(complete("Best,\n"));

    assert_eq!(
        request,
        Some(InferenceRequest::Complete {
            prompt: prompt::framed_prompt(&["My name is Sara Novak."], "Best,\n"),
            max_tokens: TokenCount(32),
            word_bias: None,
            next_word_bias: NextWordBias::default(),
            min_log_probability: None,
            typo_check: TypoCheck::Off,
            sentence_end: SentenceEnd::Stop,
            line_after: String::new(),
        })
    );
}

#[test]
fn completing_puts_the_writer_notes_before_the_text() {
    let mut pane = ready_pane(&model_file("a.gguf"));
    pane.use_settings(&Settings {
        writer_notes: WriterNotes("I am a nurse.".to_owned()),
        ..Settings::default()
    });

    let request = pane.on_action(complete("Dear "));

    assert_eq!(
        request,
        Some(InferenceRequest::Complete {
            prompt: prompt::prompt_with_notes("I am a nurse.", &[], "Dear "),
            max_tokens: TokenCount(32),
            word_bias: None,
            next_word_bias: NextWordBias::default(),
            min_log_probability: None,
            typo_check: TypoCheck::Off,
            sentence_end: SentenceEnd::Stop,
            line_after: String::new(),
        })
    );
}

#[test]
fn blank_prompt_is_not_sent() {
    let mut pane = ready_pane(&model_file("a.gguf"));

    let request = pane.on_action(complete(" \n"));

    assert_eq!(request, None);
}

#[test]
fn completing_before_a_model_is_ready_does_nothing() {
    let mut pane = PlaygroundPane::new();
    pane.on_action(PlaygroundAction::SelectModel(model_file("a.gguf")));

    let request = pane.on_action(complete("Dear"));

    assert_eq!(request, None);
}

#[test]
fn running_completion_shows_status_and_locks_the_controls() {
    let file = model_file("a.gguf");
    let pane = completing_pane(&file);

    let shown = pane.page(&[file], None, &english());

    assert_eq!(
        (
            model_menu(&shown).is_enabled,
            can_complete(&shown),
            shown_completion(&shown)
        ),
        (
            false,
            false,
            Some(completion_section(Row::Note(Note {
                text: "Writing a suggestion…".to_owned(),
                tone: NoteTone::Busy,
            }))),
        )
    );
}

#[test]
fn completing_while_a_completion_runs_does_nothing() {
    let mut pane = completing_pane(&model_file("a.gguf"));

    let request = pane.on_action(complete("Dear"));

    assert_eq!(request, None);
}

#[test]
fn finished_completion_shows_text_and_timing() {
    let shown = completed_output(" Sir,", 12, Duration::from_millis(420));

    assert_eq!(
        shown,
        Some(completion_section(Row::Output(Output {
            text: " Sir,".to_owned(),
            detail: "Written in 0.42 seconds".to_owned(),
        })))
    );
}

#[test]
fn completion_time_keeps_two_digits_after_the_decimal_separator() {
    let shown = completed_output(" Sir", 1, Duration::from_millis(1_500));

    assert_eq!(
        shown,
        Some(completion_section(Row::Output(Output {
            text: " Sir".to_owned(),
            detail: "Written in 1.50 seconds".to_owned(),
        })))
    );
}

#[test]
fn empty_completion_says_that_no_text_was_added() {
    let shown = completed_output("", 0, Duration::from_millis(30));

    assert_eq!(
        shown,
        Some(completion_section(Row::Note(Note {
            text: "The model has no suggestion for this text. Type more words, then try again.".to_owned(),
            tone: NoteTone::Plain,
        })))
    );
}

#[test]
fn failed_completion_tells_why_and_allows_another_try() {
    let file = model_file("a.gguf");
    let mut pane = completing_pane(&file);
    pane.on_event(InferenceEvent::Completed(Err(InferenceError::PromptTooLong)));

    let shown = pane.page(&[file], None, &english());

    assert_eq!(
        (can_complete(&shown), shown_completion(&shown)),
        (
            true,
            Some(completion_section(Row::Note(Note {
                text: "The text is too long for this model. Use shorter text.".to_owned(),
                tone: NoteTone::Error,
            }))),
        )
    );
}

#[test]
fn completion_without_a_request_is_ignored() {
    let file = model_file("a.gguf");
    let mut pane = ready_pane(&file);

    pane.on_event(InferenceEvent::Completed(Ok(completion(
        " Sir",
        1,
        Duration::ZERO,
    ))));

    assert_eq!(shown_completion(&pane.page(&[file], None, &english())), None);
}

#[test]
fn selecting_another_model_clears_the_output() {
    let file = model_file("a.gguf");
    let mut pane = completing_pane(&file);
    pane.on_event(InferenceEvent::Completed(Ok(completion(
        " Sir",
        1,
        Duration::ZERO,
    ))));

    pane.on_action(PlaygroundAction::SelectModel(model_file("b.gguf")));

    assert_eq!(shown_completion(&pane.page(&[file], None, &english())), None);
}

#[test]
fn deleted_model_is_no_longer_selected() {
    let pane = ready_pane(&model_file("a.gguf"));

    let shown = pane.page(&[model_file("b.gguf")], None, &english());

    let menu = model_menu(&shown);
    assert_eq!(
        (menu.choices, menu.selected, can_complete(&shown)),
        (titles(&["Select a model", "b"]), 0, false)
    );
}

fn load_failure_text(error: InferenceError) -> Option<String> {
    let file = model_file("a.gguf");
    let mut pane = PlaygroundPane::new();
    pane.on_action(PlaygroundAction::SelectModel(file.clone()));
    pane.on_event(InferenceEvent::Loaded {
        file: file.clone(),
        result: Err(error),
    });
    model_menu(&pane.page(&[file], None, &english()))
        .note
        .map(|note| note.text)
}

#[test]
fn recommended_model_shows_below_the_model_menu() {
    let pane = PlaygroundPane::new();
    let recommended = Row::Message(Message {
        text: "The recommended model".to_owned(),
        button: None,
    });

    let shown = pane.page(&[], Some(recommended.clone()), &english());

    assert_eq!(shown.sections[0].rows.get(1), Some(&recommended));
}

#[test]
fn every_error_has_its_own_message() {
    let errors = [
        InferenceError::ModelMissing,
        InferenceError::ModelInvalid,
        InferenceError::ModelTooLarge,
        InferenceError::BackendUnavailable,
        InferenceError::NoModel,
        InferenceError::PromptTooLong,
        InferenceError::Failed,
        InferenceError::Superseded,
    ];

    let messages = errors.map(load_failure_text);

    assert_eq!(
        messages,
        [
            Some("TypeAlike cannot find the model file. Download the model again.".to_owned()),
            Some("TypeAlike cannot use this model. The file can be damaged, or of a type that TypeAlike does not support. Download it again, or select another model.".to_owned()),
            Some("The model does not fit in the free memory of the graphics card. Select a smaller model.".to_owned()),
            Some("TypeAlike cannot run the model on this graphics card. In “Run the model on”, select another choice.".to_owned()),
            Some("Select a model first.".to_owned()),
            Some("The text is too long for this model. Use shorter text.".to_owned()),
            Some("TypeAlike could not write a suggestion. Try again.".to_owned()),
            Some("TypeAlike stopped this suggestion to write a newer one.".to_owned()),
        ]
    );
}

#[test]
fn every_error_message_follows_the_writing_rules() {
    let errors = [
        InferenceError::ModelMissing,
        InferenceError::ModelInvalid,
        InferenceError::ModelTooLarge,
        InferenceError::BackendUnavailable,
        InferenceError::NoModel,
        InferenceError::PromptTooLong,
        InferenceError::Failed,
        InferenceError::Superseded,
    ];

    let messages: Vec<UiText> = errors
        .into_iter()
        .filter_map(load_failure_text)
        .map(UiText::prose)
        .collect();

    assert_eq!(writing_rule_breaks(&messages), Vec::<String>::new());
}
