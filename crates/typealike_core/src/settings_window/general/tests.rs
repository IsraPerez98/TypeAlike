//! The tests of the General pane.

use super::*;
use crate::inference::{InferenceError, InferenceEvent};
use crate::test_support::{english, model_file, picked, toggled};

#[derive(Debug)]
struct FakeAutostart {
    is_enabled: bool,
}

impl Autostart for FakeAutostart {
    fn is_enabled(&self) -> bool {
        self.is_enabled
    }

    fn enable(&mut self) {
        self.is_enabled = true;
    }

    fn disable(&mut self) {
        self.is_enabled = false;
    }
}

#[derive(Debug)]
struct RefusingAutostart {
    is_enabled: bool,
}

impl Autostart for RefusingAutostart {
    fn is_enabled(&self) -> bool {
        self.is_enabled
    }

    fn enable(&mut self) {}

    fn disable(&mut self) {}
}

fn settings(backend: Backend) -> Settings {
    Settings {
        backend,
        ..Settings::default()
    }
}

fn suggestion_settings(suggestion_model: Option<ModelFile>) -> Settings {
    Settings {
        suggestion_model,
        ..Settings::default()
    }
}

fn notes_settings(notes: &str) -> Settings {
    Settings {
        writer_notes: WriterNotes(notes.to_owned()),
        ..Settings::default()
    }
}

fn settings_outcome(settings: Settings) -> GeneralOutcome {
    GeneralOutcome {
        settings: Some(settings),
        request: None,
    }
}

fn clipboard_settings(clipboard_access: ClipboardAccess) -> Settings {
    Settings {
        clipboard_access,
        ..Settings::default()
    }
}

fn preparation_settings(prompt_preparation: PromptPreparation) -> Settings {
    Settings {
        prompt_preparation,
        ..Settings::default()
    }
}

fn display_settings(suggestion_display: SuggestionDisplay) -> Settings {
    Settings {
        suggestion_display,
        ..Settings::default()
    }
}

fn typo_settings(typo_rule: TypoRule) -> Settings {
    Settings {
        typo_rule,
        ..Settings::default()
    }
}

fn edit_writer_notes(pane: &mut GeneralPane, text: &str) -> GeneralOutcome {
    pane.on_action(
        GeneralAction::EditWriterNotes(WriterNotes(text.to_owned())),
        &mut PlaygroundPane::new(),
    )
}

fn autostart_pane(is_enabled: bool) -> GeneralPane {
    GeneralPane::new(Settings::default()).with_autostart(Box::new(FakeAutostart { is_enabled }))
}

fn loading_playground(backend: Backend) -> PlaygroundPane {
    let mut playground = PlaygroundPane::with_backend(backend);
    playground.on_action(PlaygroundAction::SelectModel(model_file("model.gguf")));
    playground
}

fn loaded_playground(backend: Backend, result: Result<(), InferenceError>) -> PlaygroundPane {
    let mut playground = loading_playground(backend);
    playground.on_event(InferenceEvent::Loaded {
        file: model_file("model.gguf"),
        result,
    });
    playground
}

/// A page with each section of `pane`, so that a test finds each control of the pane.
fn page_with(pane: &GeneralPane, playground: &PlaygroundPane, installed: &[ModelFile]) -> Page {
    let translations = english();
    let mut shown = pane.page(Vec::new(), &translations);
    shown.sections.extend([
        pane.copied_text_section(&translations),
        pane.writer_section(&translations),
        pane.preparation_section(&translations),
        pane.model_section(playground, installed, &translations),
    ]);
    shown
}

fn default_page(pane: &GeneralPane) -> Page {
    page_with(pane, &PlaygroundPane::new(), &[])
}

fn row(page: &Page, id: &str) -> Row {
    page.row(id).cloned().expect("the page shows the control")
}

fn switch(page: &Page, id: &str) -> Switch {
    match row(page, id) {
        Row::Switch(switch) => switch,
        other => panic!("the control is not a switch: {other:?}"),
    }
}

fn picker(page: &Page, id: &str) -> Picker {
    match row(page, id) {
        Row::Picker(picker) => picker,
        other => panic!("the control is not a menu: {other:?}"),
    }
}

fn text_area(page: &Page, id: &str) -> TextArea {
    match row(page, id) {
        Row::TextArea(text_area) => text_area,
        other => panic!("the control is not a text area: {other:?}"),
    }
}

fn text_line(page: &Page, id: &str) -> TextLine {
    match row(page, id) {
        Row::TextLine(text_line) => text_line,
        other => panic!("the control is not a text line: {other:?}"),
    }
}

fn action_for(pane: &GeneralPane, input: &SettingsInput) -> Option<GeneralAction> {
    pane.action_for(input, &PlaygroundPane::new(), &[])
}

fn typo_rule_switch() -> Row {
    Row::Switch(Switch {
        id: ControlId::new(TYPO_RULE),
        icon: RowIcon::WarningSign,
        label: "Hide suggestions on typos".to_owned(),
        description: "When a word looks like it has a typo, no suggestion finishes it, so that Tab does not type the typo. A rare name can also look like a typo, and then gets no suggestion.".to_owned(),
        is_on: true,
        note: None,
    })
}

fn typo_fix_switch() -> Row {
    Row::Switch(Switch {
        id: ControlId::new(TYPO_FIX),
        icon: RowIcon::SpellCheck,
        label: "Fix typos".to_owned(),
        description: "When a word looks like it has a typo, the suggestion shows the word with a line through it, then the fixed word. Tab types the fixed word.".to_owned(),
        is_on: true,
        note: None,
    })
}

fn sentence_end_switch(is_on: bool, note: Option<Note>) -> Row {
    Row::Switch(Switch {
        id: ControlId::new(SENTENCE_END),
        icon: RowIcon::StopButton,
        label: "Stop at the end of a sentence".to_owned(),
        description: "A suggestion ends after the first sentence that it finishes.".to_owned(),
        is_on,
        note,
    })
}

fn default_writer_section() -> Section {
    Section::new(vec![
        Row::TextLine(TextLine {
            id: ControlId::new(WRITER_NAME),
            icon: RowIcon::Person,
            label: "Your name".to_owned(),
            description: "Suggestions can use your name, for example to sign an email.".to_owned(),
            text: String::new(),
        }),
        Row::Picker(notes_example_picker(1)),
        Row::TextArea(TextArea {
            id: ControlId::new(WRITER_NOTES),
            icon: RowIcon::Note,
            label: "Notes about you".to_owned(),
            description: "Write a few short facts that help suggestions, such as your job and your languages. Start each fact with “I”, for example “I keep my emails short.”".to_owned(),
            text: "I write clear and friendly messages. I keep the tone of the conversation.".to_owned(),
            button: None,
        }),
    ])
}

fn notes_example_picker(selected: usize) -> Picker {
    Picker {
        id: ControlId::new(NOTES_EXAMPLE),
        icon: RowIcon::Pencil,
        label: "Start from an example".to_owned(),
        description: "Select the example that is closest to how you write. Then change the notes below."
            .to_owned(),
        choices: vec![
            "Your own notes".to_owned(),
            "Everyday writing".to_owned(),
            "Work emails".to_owned(),
            "Customer support".to_owned(),
            "Chats with friends and family".to_owned(),
            "School and study".to_owned(),
            "Software development".to_owned(),
        ],
        choice_text: ChoiceText::Label,
        selected,
        is_enabled: true,
        note: None,
    }
}

fn language_section(selected: usize) -> Section {
    Section::new(vec![Row::Picker(Picker {
        id: ControlId::new(UI_LANGUAGE),
        icon: RowIcon::Globe,
        label: "Language".to_owned(),
        description: "The windows and the menus of TypeAlike show in this language. Suggestions stay in the language of your text.".to_owned(),
        choices: vec![
            "System language".to_owned(),
            "Deutsch".to_owned(),
            "English".to_owned(),
            "Español".to_owned(),
            "Français".to_owned(),
            "Italiano".to_owned(),
            "Português (Brasil)".to_owned(),
        ],
        choice_text: ChoiceText::Label,
        selected,
        is_enabled: true,
        note: None,
    })])
}

fn language_settings(ui_language: Option<UiLanguage>) -> Settings {
    Settings {
        ui_language,
        ..Settings::default()
    }
}

fn select_language(pane: &mut GeneralPane, ui_language: Option<UiLanguage>) -> GeneralOutcome {
    pane.on_action(
        GeneralAction::SelectUiLanguage(ui_language),
        &mut PlaygroundPane::with_backend(Backend::Cpu),
    )
}

fn reset_section(label: &str) -> Section {
    Section::new(vec![Row::Message(Message {
        text: "All settings go back to their defaults, and the setup opens again. Your downloaded models and your earlier writing stay.".to_owned(),
        button: Some(Button {
            id: ControlId::new(RESET),
            label: label.to_owned(),
            is_enabled: true,
        }),
    })])
}

#[test]
fn page_shows_the_suggestion_settings() {
    let pane = GeneralPane::new(settings(Backend::Vulkan));

    let shown = pane.page(Vec::new(), &english());

    assert_eq!(
        shown,
        Page {
            title: "General".to_owned(),
            icon: PageIcon::Gear,
            summary: "Change when TypeAlike starts and how suggestions show.".to_owned(),
            search: None,
            sections: vec![language_section(0), Section::new(vec![
                Row::Picker(Picker {
                    id: ControlId::new(LENGTH),
                    icon: RowIcon::Ruler,
                    label: "Suggestion length".to_owned(),
                    description: "A suggestion stops at this length. A shorter suggestion shows sooner.".to_owned(),
                    choices: vec![
                        "Very short".to_owned(),
                        "Short".to_owned(),
                        "Medium".to_owned(),
                        "Long".to_owned(),
                        "Very long".to_owned(),
                    ],
                    choice_text: ChoiceText::Label,
                    selected: 2,
                    is_enabled: true,
                    note: None,
                }),
                sentence_end_switch(true, None),
                Row::Switch(Switch {
                    id: ControlId::new(DISPLAY),
                    icon: RowIcon::Stopwatch,
                    label: "Show words early".to_owned(),
                    description: "Each word of a suggestion shows as soon as the model writes it. The first word then shows sooner, but the suggestion grows while you read it.".to_owned(),
                    is_on: true,
                    note: None,
                }),
                typo_rule_switch(),
                typo_fix_switch(),
            ]), reset_section("Reset all settings")],
        }
    );
}

#[test]
fn language_menu_selects_the_saved_language() {
    let cases = [
        (None, 0),
        (Some(UiLanguage::English), 2),
        (Some(UiLanguage::Spanish), 3),
    ];

    let sections = cases.map(|(ui_language, _)| {
        GeneralPane::new(language_settings(ui_language)).language_section(&english())
    });

    assert_eq!(sections, cases.map(|(_, selected)| language_section(selected)));
}

#[test]
fn each_language_entry_selects_its_language() {
    let pane = GeneralPane::new(Settings::default());
    let cases = [
        (0, None),
        (1, Some(UiLanguage::German)),
        (2, Some(UiLanguage::English)),
        (3, Some(UiLanguage::Spanish)),
        (4, Some(UiLanguage::French)),
        (5, Some(UiLanguage::Italian)),
        (6, Some(UiLanguage::Portuguese)),
    ];

    let actions = cases.map(|(index, _)| action_for(&pane, &picked(UI_LANGUAGE, index)));

    assert_eq!(
        actions,
        cases.map(|(_, ui_language)| Some(GeneralAction::SelectUiLanguage(ui_language)))
    );
}

#[test]
fn picking_past_the_last_language_does_nothing() {
    let pane = GeneralPane::new(Settings::default());

    let action = action_for(&pane, &picked(UI_LANGUAGE, 7));

    assert_eq!(action, None);
}

#[test]
fn picked_language_replaces_the_language_of_the_system() {
    let mut pane = GeneralPane::new(Settings::default()).with_system_language(UiLanguage::English);

    let outcome = select_language(&mut pane, Some(UiLanguage::Spanish));

    assert_eq!(
        (
            outcome.settings.map(|settings| settings.ui_language),
            pane.language()
        ),
        (Some(Some(UiLanguage::Spanish)), UiLanguage::Spanish)
    );
}

#[test]
fn system_language_entry_follows_the_language_of_the_system() {
    let mut pane = GeneralPane::new(language_settings(Some(UiLanguage::English)))
        .with_system_language(UiLanguage::Spanish);

    select_language(&mut pane, None);

    assert_eq!(pane.language(), UiLanguage::Spanish);
}

#[test]
fn picked_language_writes_the_notes_of_an_example_in_it() {
    let mut pane = GeneralPane::new(notes_settings(
        "I write emails and messages at work. I use a polite and professional tone, and I keep my messages short and clear.",
    ));

    let outcome = select_language(&mut pane, Some(UiLanguage::Spanish));

    assert_eq!(
        outcome.settings.map(|settings| settings.writer_notes),
        Some(NotesExample::WorkEmails.notes(&Translations::new(UiLanguage::Spanish)))
    );
}

#[test]
fn picked_language_keeps_the_own_notes_of_the_user() {
    let mut pane = GeneralPane::new(notes_settings("I am a nurse."));

    let outcome = select_language(&mut pane, Some(UiLanguage::Spanish));

    assert_eq!(
        outcome.settings.map(|settings| settings.writer_notes),
        Some(WriterNotes("I am a nurse.".to_owned()))
    );
}

#[test]
fn first_press_of_reset_asks_for_a_second_press() {
    let mut pane = GeneralPane::new(settings(Backend::Vulkan));

    let press = pane.press_reset();

    let last_section = pane.page(Vec::new(), &english()).sections.pop();
    assert_eq!(
        (press, last_section),
        (ResetPress::Armed, Some(reset_section("Press again to reset")))
    );
}

#[test]
fn second_press_of_reset_confirms_it() {
    let mut pane = GeneralPane::new(settings(Backend::Vulkan));
    pane.press_reset();

    let press = pane.press_reset();

    let last_section = pane.page(Vec::new(), &english()).sections.pop();
    assert_eq!(
        (press, last_section),
        (ResetPress::Confirmed, Some(reset_section("Reset all settings")))
    );
}

#[test]
fn canceled_reset_waits_for_its_first_press_again() {
    let mut pane = GeneralPane::new(settings(Backend::Vulkan));
    pane.press_reset();

    let was_pending = pane.cancel_reset();

    assert_eq!((was_pending, pane.press_reset()), (true, ResetPress::Armed));
}

#[test]
fn reset_button_is_the_only_reset_press() {
    let inputs = [
        SettingsInput::Pressed(ControlId::new(RESET)),
        SettingsInput::Pressed(ControlId::new("download")),
        toggled(RESET, true),
    ];

    let presses = inputs.map(|input| presses_reset(&input));

    assert_eq!(presses, [true, false, false]);
}

#[test]
fn reset_gives_the_first_start_settings_and_keeps_the_suggestion_model() {
    let mut pane = GeneralPane::new(Settings {
        suggestion_model: Some(model_file("model.gguf")),
        writer_notes: WriterNotes("I am a nurse.".to_owned()),
        setup: Setup::Finished,
        ..settings(Backend::Cpu)
    });
    let mut playground = PlaygroundPane::with_backend(Backend::Cpu);
    let first_start = Settings {
        backend: Backend::Vulkan,
        setup: Setup::Needed,
        ..Settings::default()
    };

    let outcome = pane.reset(first_start.clone(), &mut playground);

    let expected = Settings {
        suggestion_model: Some(model_file("model.gguf")),
        ..first_start
    };
    assert_eq!(
        (outcome.settings, pane.settings()),
        (Some(expected.clone()), &expected)
    );
}

#[test]
fn reset_keeps_the_backend_of_a_platform_with_one_backend() {
    let mut pane = GeneralPane::new(Settings::default()).with_platform_backend(Backend::Metal);
    let mut playground = PlaygroundPane::with_backend(Backend::Metal);

    pane.reset(settings(Backend::Cpu), &mut playground);

    assert_eq!(pane.settings().backend, Backend::Metal);
}

#[test]
fn finish_of_the_setup_keeps_it_finished() {
    let mut pane = GeneralPane::new(Settings {
        setup: Setup::Needed,
        ..Settings::default()
    });
    let mut playground = PlaygroundPane::with_backend(Backend::Cpu);

    let outcome = pane.on_action(GeneralAction::FinishSetup, &mut playground);

    assert_eq!(
        outcome.settings.map(|settings| settings.setup),
        Some(Setup::Finished)
    );
}

#[test]
fn notes_example_menu_selects_the_example_of_the_notes() {
    let cases = [
        (
            "I write clear and friendly messages. I keep the tone of the conversation.",
            1,
        ),
        (
            "I chat with friends and family. I write short and casual messages.",
            4,
        ),
        ("I am a nurse.", 0),
    ];

    let selected = cases.map(|(notes, _)| {
        GeneralPane::new(notes_settings(notes))
            .writer_section(&english())
            .rows
            .into_iter()
            .find_map(|row| match row {
                Row::Picker(picker) => Some(picker.selected),
                _ => None,
            })
    });

    assert_eq!(selected, cases.map(|(_, index)| Some(index)));
}

#[test]
fn pick_of_an_example_writes_its_notes_in_the_language_of_the_pane() {
    let mut pane = GeneralPane::new(Settings::default()).with_system_language(UiLanguage::Spanish);
    let mut playground = PlaygroundPane::with_backend(Backend::Cpu);
    let action = pane.action_for(&picked(NOTES_EXAMPLE, 4), &playground, &[]);

    let outcome = pane.on_action(action.expect("the menu picks an example"), &mut playground);

    assert_eq!(
        outcome.settings.map(|settings| settings.writer_notes),
        Some(WriterNotes(
            "Chateo con mi familia y mis amigos. Escribo mensajes breves e informales.".to_owned()
        ))
    );
}

#[test]
fn own_notes_entry_picks_nothing() {
    let pane = GeneralPane::new(notes_settings("I am a nurse."));
    let playground = PlaygroundPane::with_backend(Backend::Cpu);

    let action = pane.action_for(&picked(NOTES_EXAMPLE, 0), &playground, &[]);

    assert_eq!(action, None);
}

#[test]
fn copied_text_section_shows_the_clipboard_switch() {
    let pane = GeneralPane::new(settings(Backend::Vulkan));

    let shown = pane.copied_text_section(&english());

    assert_eq!(
        shown,
        Section::new(vec![Row::Switch(Switch {
            id: ControlId::new(CLIPBOARD),
            icon: RowIcon::Clipboard,
            label: "Use copied text".to_owned(),
            description: "Suggestions can use the text that you copy. TypeAlike reads it only while you use a text field, and does not save it.".to_owned(),
            is_on: true,
            note: None,
        })])
    );
}

#[test]
fn writer_section_shows_the_name_and_the_notes() {
    let pane = GeneralPane::new(settings(Backend::Vulkan));

    let shown = pane.writer_section(&english());

    assert_eq!(shown, default_writer_section());
}

#[test]
fn preparation_section_shows_the_preparation_switch() {
    let pane = GeneralPane::new(settings(Backend::Vulkan));

    let shown = pane.preparation_section(&english());

    assert_eq!(
        shown,
        Section::new(vec![Row::Switch(Switch {
            id: ControlId::new(PREPARATION),
            icon: RowIcon::LightningBolt,
            label: "Prepare suggestions early".to_owned(),
            description: "When you go to a text field, the model reads the text in it at once. The first suggestion then shows sooner, but this uses more power.".to_owned(),
            is_on: true,
            note: None,
        })])
    );
}

#[test]
fn model_section_shows_the_model_menu_and_then_the_backend_menu() {
    let pane = GeneralPane::new(settings(Backend::Vulkan));

    let shown = pane.model_section(&PlaygroundPane::with_backend(Backend::Vulkan), &[], &english());

    assert_eq!(
        shown,
        Section::new(vec![
            Row::Picker(Picker {
                id: ControlId::new(SUGGESTION_MODEL),
                icon: RowIcon::Robot,
                label: "Suggestion model".to_owned(),
                description: "A smaller model is faster and needs less memory, but its suggestions are wrong more often.".to_owned(),
                choices: vec!["No downloaded models".to_owned()],
                choice_text: ChoiceText::Name,
                selected: 0,
                is_enabled: false,
                note: Some(Note {
                    text: "TypeAlike needs a model to write suggestions. Download the recommended model below.".to_owned(),
                    tone: NoteTone::Plain,
                }),
            }),
            Row::Picker(Picker {
                id: ControlId::new(BACKEND),
                icon: RowIcon::Chip,
                label: "Run the model on".to_owned(),
                description: "A graphics card runs the model faster than the processor, but the whole model must fit in its memory.".to_owned(),
                choices: vec![
                    "Processor (CPU)".to_owned(),
                    "Graphics card (Vulkan)".to_owned(),
                    "NVIDIA graphics card (CUDA)".to_owned(),
                ],
                choice_text: ChoiceText::Label,
                selected: 1,
                is_enabled: true,
                note: None,
            }),
        ])
    );
}

fn recommended_row() -> Row {
    Row::Message(Message {
        text: "The recommended model".to_owned(),
        button: None,
    })
}

#[test]
fn recommended_model_shows_first_below_the_reason_for_a_model() {
    let pane = GeneralPane::new(settings(Backend::Vulkan));

    let shown = pane.page(vec![recommended_row()], &english());

    assert_eq!(
        shown.sections.first(),
        Some(&Section::new(vec![
            Row::Note(Note {
                text: "TypeAlike needs a model to write suggestions. Download the recommended model below."
                    .to_owned(),
                tone: NoteTone::Plain,
            }),
            recommended_row(),
        ]))
    );
}

#[test]
fn menu_shows_each_saved_backend_as_its_own_choice() {
    let backends = [Backend::Cpu, Backend::Vulkan, Backend::Cuda];

    let shown = backends.map(|backend| {
        let pane = GeneralPane::new(settings(backend));
        let selected = picker(&default_page(&pane), BACKEND).selected;
        action_for(&pane, &picked(BACKEND, selected))
    });

    assert_eq!(
        shown,
        backends.map(|backend| Some(GeneralAction::SelectBackend(backend)))
    );
}

#[test]
fn pane_with_a_platform_backend_hides_the_menu() {
    let pane = GeneralPane::new(settings(Backend::Cpu)).with_platform_backend(Backend::Metal);

    let shown = page_with(&pane, &loading_playground(Backend::Metal), &[]);

    assert_eq!(shown.row(BACKEND), None);
}

#[test]
fn platform_backend_replaces_the_saved_backend() {
    let pane = GeneralPane::new(settings(Backend::Cpu)).with_platform_backend(Backend::Metal);

    assert_eq!(pane.settings().backend, Backend::Metal);
}

#[test]
fn saved_backend_that_the_menu_does_not_offer_becomes_the_default() {
    let pane = GeneralPane::new(settings(Backend::Metal));

    assert_eq!(pane.settings().backend, Backend::default());
}

#[test]
fn each_switch_gives_its_on_and_off_action() {
    let pane = GeneralPane::new(Settings::default());
    let inputs = [
        toggled(AUTOSTART, true),
        toggled(AUTOSTART, false),
        toggled(CLIPBOARD, true),
        toggled(CLIPBOARD, false),
        toggled(PREPARATION, true),
        toggled(PREPARATION, false),
        toggled(DISPLAY, true),
        toggled(DISPLAY, false),
        toggled(TYPO_RULE, true),
        toggled(TYPO_RULE, false),
        toggled(TYPO_FIX, true),
        toggled(TYPO_FIX, false),
        toggled(SENTENCE_END, true),
        toggled(SENTENCE_END, false),
    ];

    let actions = inputs.map(|input| action_for(&pane, &input));

    assert_eq!(
        actions,
        [
            GeneralAction::EnableAutostart,
            GeneralAction::DisableAutostart,
            GeneralAction::SwitchClipboard(ClipboardAccess::Allowed),
            GeneralAction::SwitchClipboard(ClipboardAccess::Denied),
            GeneralAction::SwitchPreparation(PromptPreparation::OnFocus),
            GeneralAction::SwitchPreparation(PromptPreparation::OnFirstKey),
            GeneralAction::SwitchDisplay(SuggestionDisplay::WordByWord),
            GeneralAction::SwitchDisplay(SuggestionDisplay::WhenComplete),
            GeneralAction::SwitchTypoRule(TypoRule::HideSuggestions),
            GeneralAction::SwitchTypoRule(TypoRule::Suggest),
            GeneralAction::SwitchTypoFix(TypoFix::Offer),
            GeneralAction::SwitchTypoFix(TypoFix::Off),
            GeneralAction::SwitchSentenceEnd(SentenceEnd::Stop),
            GeneralAction::SwitchSentenceEnd(SentenceEnd::Continue),
        ]
        .map(Some)
    );
}

#[test]
fn picking_a_suggestion_length_selects_it() {
    let pane = GeneralPane::new(settings(Backend::Vulkan));

    let action = action_for(&pane, &picked(LENGTH, 0));

    assert_eq!(
        action,
        Some(GeneralAction::SelectSuggestionLength(SuggestionLength::VeryShort))
    );
}

#[test]
fn picking_a_short_length_on_the_cpu_selects_it() {
    let pane = GeneralPane::new(settings(Backend::Cpu));

    let actions = [0, 1].map(|index| action_for(&pane, &picked(LENGTH, index)));

    assert_eq!(
        actions,
        [SuggestionLength::VeryShort, SuggestionLength::Short]
            .map(|length| Some(GeneralAction::SelectSuggestionLength(length)))
    );
}

#[test]
fn picking_past_the_short_lengths_on_the_cpu_does_nothing() {
    let pane = GeneralPane::new(settings(Backend::Cpu));

    let action = action_for(&pane, &picked(LENGTH, 2));

    assert_eq!(action, None);
}

#[test]
fn cpu_backend_offers_only_the_short_lengths() {
    let pane = GeneralPane::new(Settings {
        backend: Backend::Cpu,
        suggestion_length: SuggestionLength::VeryLong,
        ..Settings::default()
    });

    let shown = picker(&default_page(&pane), LENGTH);

    assert_eq!(
        (shown.choices, shown.selected, shown.is_enabled, shown.note),
        (
            vec!["Very short".to_owned(), "Short".to_owned()],
            1,
            true,
            Some(Note {
                text: "When the model runs on the processor, suggestions are short or very short, so that they show sooner."
                    .to_owned(),
                tone: NoteTone::Plain,
            })
        )
    );
}

#[test]
fn gpu_backend_shows_the_selected_length() {
    let pane = GeneralPane::new(Settings {
        backend: Backend::Cuda,
        suggestion_length: SuggestionLength::VeryLong,
        ..Settings::default()
    });

    let shown = picker(&default_page(&pane), LENGTH);

    assert_eq!((shown.selected, shown.is_enabled, shown.note), (4, true, None));
}

#[test]
fn picking_past_the_last_entry_does_nothing() {
    let pane = GeneralPane::new(settings(Backend::Vulkan));

    let action = action_for(&pane, &picked(LENGTH, LENGTH_CHOICES.len()));

    assert_eq!(action, None);
}

#[test]
fn picking_a_backend_without_backend_menu_does_nothing() {
    let pane = GeneralPane::new(settings(Backend::Cpu)).with_platform_backend(Backend::Metal);

    let action = action_for(&pane, &picked(BACKEND, 1));

    assert_eq!(action, None);
}

#[test]
fn picking_a_suggestion_model_selects_it() {
    let pane = GeneralPane::new(Settings::default());
    let installed = [model_file("other.gguf"), model_file("model.gguf")];

    let action = pane.action_for(&picked(SUGGESTION_MODEL, 2), &PlaygroundPane::new(), &installed);

    assert_eq!(
        action,
        Some(GeneralAction::SelectSuggestionModel(model_file("model.gguf")))
    );
}

#[test]
fn picking_the_placeholder_model_does_nothing() {
    let pane = GeneralPane::new(Settings::default());

    let action = pane.action_for(
        &picked(SUGGESTION_MODEL, 0),
        &PlaygroundPane::new(),
        &[model_file("model.gguf")],
    );

    assert_eq!(action, None);
}

#[test]
fn editing_the_name_field_edits_the_writer_name() {
    let pane = GeneralPane::new(Settings::default());
    let input = SettingsInput::Edited {
        id: ControlId::new(WRITER_NAME),
        text: "Sara Novak".to_owned(),
    };

    let action = action_for(&pane, &input);

    assert_eq!(
        action,
        Some(GeneralAction::EditWriterName(WriterName("Sara Novak".to_owned())))
    );
}

#[test]
fn editing_the_writer_name_keeps_it() {
    let mut pane = GeneralPane::new(Settings::default());
    let name_settings = Settings {
        writer_name: WriterName("Sara Novak".to_owned()),
        ..Settings::default()
    };

    let outcome = pane.on_action(
        GeneralAction::EditWriterName(WriterName("Sara Novak".to_owned())),
        &mut PlaygroundPane::new(),
    );

    assert_eq!(outcome, settings_outcome(name_settings));
}

#[test]
fn name_field_shows_the_edited_writer_name() {
    let mut pane = GeneralPane::new(Settings::default());
    pane.on_action(
        GeneralAction::EditWriterName(WriterName("Sara Novak".to_owned())),
        &mut PlaygroundPane::new(),
    );

    let shown = text_line(&default_page(&pane), WRITER_NAME).text;

    assert_eq!(shown, "Sara Novak");
}

#[test]
fn editing_the_notes_field_edits_the_writer_notes() {
    let pane = GeneralPane::new(Settings::default());
    let input = SettingsInput::Edited {
        id: ControlId::new(WRITER_NOTES),
        text: "I am a nurse.".to_owned(),
    };

    let action = action_for(&pane, &input);

    assert_eq!(
        action,
        Some(GeneralAction::EditWriterNotes(WriterNotes(
            "I am a nurse.".to_owned()
        )))
    );
}

#[test]
fn input_for_a_control_of_another_pane_is_not_for_this_pane() {
    let pane = GeneralPane::new(Settings::default());

    let action = action_for(&pane, &SettingsInput::Pressed(ControlId::new("download")));

    assert_eq!(action, None);
}

#[test]
fn selecting_a_suggestion_model_loads_it_in_the_playground() {
    let mut pane = GeneralPane::new(Settings::default());
    let mut playground = PlaygroundPane::new();

    let outcome = pane.on_action(
        GeneralAction::SelectSuggestionModel(model_file("model.gguf")),
        &mut playground,
    );

    assert_eq!(
        outcome,
        GeneralOutcome {
            settings: None,
            request: Some(InferenceRequest::Load {
                file: model_file("model.gguf"),
                backend: Backend::Cpu,
            }),
        }
    );
}

#[test]
fn selecting_a_backend_keeps_it() {
    let mut pane = GeneralPane::new(settings(Backend::Cpu));
    let mut playground = PlaygroundPane::with_backend(Backend::Cpu);

    let outcome = pane.on_action(GeneralAction::SelectBackend(Backend::Cuda), &mut playground);

    assert_eq!(
        (outcome, pane.settings()),
        (
            GeneralOutcome {
                settings: Some(settings(Backend::Cuda)),
                request: None,
            },
            &settings(Backend::Cuda),
        )
    );
}

#[test]
fn selecting_a_backend_loads_the_selected_model_on_it() {
    let mut pane = GeneralPane::new(settings(Backend::Cpu));
    let mut playground = loaded_playground(Backend::Cpu, Ok(()));

    let outcome = pane.on_action(GeneralAction::SelectBackend(Backend::Vulkan), &mut playground);

    assert_eq!(
        outcome.request,
        Some(InferenceRequest::Load {
            file: model_file("model.gguf"),
            backend: Backend::Vulkan,
        })
    );
}

#[test]
fn selecting_the_current_backend_does_nothing() {
    let mut pane = GeneralPane::new(settings(Backend::Vulkan));
    let mut playground = loaded_playground(Backend::Vulkan, Ok(()));

    let outcome = pane.on_action(GeneralAction::SelectBackend(Backend::Vulkan), &mut playground);

    assert_eq!(outcome, GeneralOutcome::default());
}

#[test]
fn selecting_a_backend_while_the_model_loads_does_nothing() {
    let mut pane = GeneralPane::new(settings(Backend::Cpu));
    let mut playground = loading_playground(Backend::Cpu);

    let outcome = pane.on_action(GeneralAction::SelectBackend(Backend::Vulkan), &mut playground);

    assert_eq!(
        (outcome, pane.settings()),
        (GeneralOutcome::default(), &settings(Backend::Cpu))
    );
}

#[test]
fn loading_model_locks_the_backend_menu() {
    let pane = GeneralPane::new(settings(Backend::Cuda));

    let shown = page_with(&pane, &loading_playground(Backend::Cuda), &[]);

    assert!(!picker(&shown, BACKEND).is_enabled);
}

#[test]
fn model_too_large_for_the_gpu_tells_what_to_do() {
    let pane = GeneralPane::new(settings(Backend::Vulkan));

    let shown = page_with(
        &pane,
        &loaded_playground(Backend::Vulkan, Err(InferenceError::ModelTooLarge)),
        &[model_file("model.gguf")],
    );

    assert_eq!(
        (
            picker(&shown, BACKEND).is_enabled,
            picker(&shown, SUGGESTION_MODEL).note
        ),
        (
            true,
            Some(Note {
                text:
                    "The model does not fit in the free memory of the graphics card. Select a smaller model."
                        .to_owned(),
                tone: NoteTone::Error,
            }),
        )
    );
}

#[test]
fn autostart_switch_shows_the_state_that_the_os_keeps() {
    let pane = autostart_pane(true);

    let shown = default_page(&pane);

    assert_eq!(
        switch(&shown, AUTOSTART),
        Switch {
            id: ControlId::new(AUTOSTART),
            icon: RowIcon::Power,
            label: "Open at login".to_owned(),
            description: "TypeAlike starts each time you log in, so you do not need to start it yourself."
                .to_owned(),
            is_on: true,
            note: None,
        }
    );
}

#[test]
fn autostart_switch_comes_first() {
    let pane = autostart_pane(false);

    let shown = default_page(&pane);

    assert_eq!(shown.sections[0].rows[0].id(), Some(&ControlId::new(AUTOSTART)));
}

#[test]
fn enabling_autostart_turns_the_switch_on() {
    let mut pane = autostart_pane(false);

    let outcome = pane.on_action(GeneralAction::EnableAutostart, &mut PlaygroundPane::new());

    let is_on = switch(&default_page(&pane), AUTOSTART).is_on;
    assert_eq!((outcome, is_on), (GeneralOutcome::default(), true));
}

#[test]
fn disabling_autostart_turns_the_switch_off() {
    let mut pane = autostart_pane(true);

    pane.on_action(GeneralAction::DisableAutostart, &mut PlaygroundPane::new());

    assert!(!switch(&default_page(&pane), AUTOSTART).is_on);
}

#[test]
fn switch_shows_the_kept_state_when_the_os_refuses_a_change() {
    let mut pane = GeneralPane::new(Settings::default())
        .with_autostart(Box::new(RefusingAutostart { is_enabled: false }));

    pane.on_action(GeneralAction::EnableAutostart, &mut PlaygroundPane::new());

    assert!(!switch(&default_page(&pane), AUTOSTART).is_on);
}

#[test]
fn suggestion_model_menu_shows_the_model_that_the_playground_selected() {
    let other = model_file("other.gguf");
    let pane = GeneralPane::new(suggestion_settings(None));

    let shown = page_with(
        &pane,
        &loaded_playground(Backend::Cpu, Ok(())),
        &[other, model_file("model.gguf")],
    );

    let menu = picker(&shown, SUGGESTION_MODEL);
    assert_eq!(
        (menu.choices, menu.selected, menu.is_enabled, menu.note),
        (vec!["other".to_owned(), "model".to_owned()], 1, true, None)
    );
}

#[test]
fn suggestion_model_menu_locks_and_shows_status_while_the_model_loads() {
    let pane = GeneralPane::new(suggestion_settings(None));

    let shown = page_with(
        &pane,
        &loading_playground(Backend::Cpu),
        &[model_file("model.gguf")],
    );

    let menu = picker(&shown, SUGGESTION_MODEL);
    assert_eq!(
        (menu.is_enabled, menu.note),
        (
            false,
            Some(Note {
                text: "Starting the model…".to_owned(),
                tone: NoteTone::Busy,
            }),
        )
    );
}

#[test]
fn loaded_model_becomes_the_suggestion_model() {
    let mut pane = GeneralPane::new(suggestion_settings(None));

    let settings = pane.keep_suggestion_model(&loaded_playground(Backend::Cpu, Ok(())));

    assert_eq!(
        (settings, pane.settings()),
        (
            Some(suggestion_settings(Some(model_file("model.gguf")))),
            &suggestion_settings(Some(model_file("model.gguf"))),
        )
    );
}

#[test]
fn loading_the_saved_suggestion_model_again_keeps_nothing() {
    let mut pane = GeneralPane::new(suggestion_settings(Some(model_file("model.gguf"))));

    let settings = pane.keep_suggestion_model(&loaded_playground(Backend::Cpu, Ok(())));

    assert_eq!(settings, None);
}

#[test]
fn model_that_did_not_load_is_not_kept() {
    let playgrounds = [
        loading_playground(Backend::Cpu),
        loaded_playground(Backend::Cpu, Err(InferenceError::ModelInvalid)),
    ];

    let kept = playgrounds
        .map(|playground| GeneralPane::new(suggestion_settings(None)).keep_suggestion_model(&playground));

    assert_eq!(kept, [None, None]);
}

#[test]
fn suggestion_model_loads_in_the_playground() {
    let pane = GeneralPane::new(suggestion_settings(Some(model_file("model.gguf"))));
    let mut playground = PlaygroundPane::new();

    let request = pane.load_suggestion_model(&mut playground);

    assert_eq!(
        (request, playground.is_busy()),
        (
            Some(InferenceRequest::Load {
                file: model_file("model.gguf"),
                backend: Backend::Cpu,
            }),
            true,
        )
    );
}

#[test]
fn unloaded_model_stays_the_suggestion_model() {
    let mut pane = GeneralPane::new(suggestion_settings(Some(model_file("model.gguf"))));
    let mut playground = loaded_playground(Backend::Cpu, Ok(()));
    playground.unload_model(StopReason::Paused);

    let settings = pane.keep_suggestion_model(&playground);

    assert_eq!(
        (settings, pane.settings()),
        (None, &suggestion_settings(Some(model_file("model.gguf"))))
    );
}

#[test]
fn unloading_the_suggestion_model_unloads_the_model_of_the_playground() {
    let pane = GeneralPane::new(suggestion_settings(Some(model_file("model.gguf"))));
    let mut playground = loaded_playground(Backend::Cpu, Ok(()));

    let request = pane.unload_suggestion_model(&mut playground, StopReason::EnergySaver);

    assert_eq!(
        (request, playground.loaded_model()),
        (Some(InferenceRequest::Unload), None)
    );
}

#[test]
fn unloading_before_any_load_selects_the_saved_model_without_a_load() {
    let pane = GeneralPane::new(suggestion_settings(Some(model_file("model.gguf"))));
    let mut playground = PlaygroundPane::with_backend(Backend::Cpu);

    let request = pane.unload_suggestion_model(&mut playground, StopReason::EnergySaver);

    let shown = page_with(&pane, &playground, &[model_file("model.gguf")]);
    let menu = picker(&shown, SUGGESTION_MODEL);
    assert_eq!(
        (request, menu.choices, menu.selected),
        (None, vec!["model".to_owned()], 0)
    );
}

#[test]
fn unloaded_suggestion_model_loads_again() {
    let pane = GeneralPane::new(suggestion_settings(Some(model_file("model.gguf"))));
    let mut playground = loaded_playground(Backend::Cpu, Ok(()));
    playground.unload_model(StopReason::Paused);

    let request = pane.load_suggestion_model(&mut playground);

    assert_eq!(
        request,
        Some(InferenceRequest::Load {
            file: model_file("model.gguf"),
            backend: Backend::Cpu,
        })
    );
}

#[test]
fn no_suggestion_model_loads_nothing() {
    let pane = GeneralPane::new(suggestion_settings(None));
    let mut playground = PlaygroundPane::new();

    let request = pane.load_suggestion_model(&mut playground);

    assert_eq!((request, playground.is_busy()), (None, false));
}

#[test]
fn notes_field_shows_the_writer_notes() {
    let pane = GeneralPane::new(notes_settings("I am a nurse."));

    let field = text_area(&default_page(&pane), WRITER_NOTES);

    assert_eq!(field.text, "I am a nurse.");
}

#[test]
fn editing_the_writer_notes_keeps_them() {
    let mut pane = GeneralPane::new(Settings::default());

    let outcome = edit_writer_notes(&mut pane, "I am a nurse.");

    assert_eq!(
        (outcome, pane.settings()),
        (
            settings_outcome(notes_settings("I am a nurse.")),
            &notes_settings("I am a nurse."),
        )
    );
}

#[test]
fn emptied_writer_notes_stay_empty() {
    let mut pane = GeneralPane::new(notes_settings("I am a nurse."));

    let outcome = edit_writer_notes(&mut pane, "");

    assert_eq!(outcome, settings_outcome(notes_settings("")));
}

#[test]
fn editing_the_writer_notes_to_the_same_text_does_nothing() {
    let mut pane = GeneralPane::new(notes_settings("I am a nurse."));

    let outcome = edit_writer_notes(&mut pane, "I am a nurse.");

    assert_eq!(outcome, GeneralOutcome::default());
}

#[test]
fn allowing_the_clipboard_keeps_it_and_turns_the_switch_on() {
    let mut pane = GeneralPane::new(clipboard_settings(ClipboardAccess::Denied));

    let outcome = pane.on_action(
        GeneralAction::SwitchClipboard(ClipboardAccess::Allowed),
        &mut PlaygroundPane::new(),
    );

    let is_on = switch(&default_page(&pane), CLIPBOARD).is_on;
    assert_eq!(
        (outcome, is_on),
        (
            settings_outcome(clipboard_settings(ClipboardAccess::Allowed)),
            true
        )
    );
}

#[test]
fn denying_the_clipboard_keeps_it_and_turns_the_switch_off() {
    let mut pane = GeneralPane::new(clipboard_settings(ClipboardAccess::Allowed));

    let outcome = pane.on_action(
        GeneralAction::SwitchClipboard(ClipboardAccess::Denied),
        &mut PlaygroundPane::new(),
    );

    let is_on = switch(&default_page(&pane), CLIPBOARD).is_on;
    assert_eq!(
        (outcome, is_on),
        (
            settings_outcome(clipboard_settings(ClipboardAccess::Denied)),
            false
        )
    );
}

#[test]
fn choosing_the_current_clipboard_access_does_nothing() {
    let mut pane = GeneralPane::new(clipboard_settings(ClipboardAccess::Allowed));

    let outcome = pane.on_action(
        GeneralAction::SwitchClipboard(ClipboardAccess::Allowed),
        &mut PlaygroundPane::new(),
    );

    assert_eq!(outcome, GeneralOutcome::default());
}

#[test]
fn preparing_on_focus_keeps_it_and_turns_the_switch_on() {
    let mut pane = GeneralPane::new(preparation_settings(PromptPreparation::OnFirstKey));

    let outcome = pane.on_action(
        GeneralAction::SwitchPreparation(PromptPreparation::OnFocus),
        &mut PlaygroundPane::new(),
    );

    let is_on = switch(&default_page(&pane), PREPARATION).is_on;
    assert_eq!(
        (outcome, is_on),
        (
            settings_outcome(preparation_settings(PromptPreparation::OnFocus)),
            true
        )
    );
}

#[test]
fn preparing_on_the_first_key_keeps_it_and_turns_the_switch_off() {
    let mut pane = GeneralPane::new(preparation_settings(PromptPreparation::OnFocus));

    let outcome = pane.on_action(
        GeneralAction::SwitchPreparation(PromptPreparation::OnFirstKey),
        &mut PlaygroundPane::new(),
    );

    let is_on = switch(&default_page(&pane), PREPARATION).is_on;
    assert_eq!(
        (outcome, is_on),
        (
            settings_outcome(preparation_settings(PromptPreparation::OnFirstKey)),
            false
        )
    );
}

#[test]
fn choosing_the_current_preparation_does_nothing() {
    let mut pane = GeneralPane::new(preparation_settings(PromptPreparation::OnFocus));

    let outcome = pane.on_action(
        GeneralAction::SwitchPreparation(PromptPreparation::OnFocus),
        &mut PlaygroundPane::new(),
    );

    assert_eq!(outcome, GeneralOutcome::default());
}

#[test]
fn showing_word_by_word_keeps_it_and_turns_the_switch_on() {
    let mut pane = GeneralPane::new(display_settings(SuggestionDisplay::WhenComplete));

    let outcome = pane.on_action(
        GeneralAction::SwitchDisplay(SuggestionDisplay::WordByWord),
        &mut PlaygroundPane::new(),
    );

    let is_on = switch(&default_page(&pane), DISPLAY).is_on;
    assert_eq!(
        (outcome, is_on),
        (
            settings_outcome(display_settings(SuggestionDisplay::WordByWord)),
            true
        )
    );
}

#[test]
fn showing_when_complete_keeps_it_and_turns_the_switch_off() {
    let mut pane = GeneralPane::new(display_settings(SuggestionDisplay::WordByWord));

    let outcome = pane.on_action(
        GeneralAction::SwitchDisplay(SuggestionDisplay::WhenComplete),
        &mut PlaygroundPane::new(),
    );

    let is_on = switch(&default_page(&pane), DISPLAY).is_on;
    assert_eq!(
        (outcome, is_on),
        (
            settings_outcome(display_settings(SuggestionDisplay::WhenComplete)),
            false
        )
    );
}

#[test]
fn choosing_the_current_suggestion_display_does_nothing() {
    let mut pane = GeneralPane::new(display_settings(SuggestionDisplay::WordByWord));

    let outcome = pane.on_action(
        GeneralAction::SwitchDisplay(SuggestionDisplay::WordByWord),
        &mut PlaygroundPane::new(),
    );

    assert_eq!(outcome, GeneralOutcome::default());
}

#[test]
fn suggesting_on_typos_keeps_it_and_turns_the_switch_off() {
    let mut pane = GeneralPane::new(Settings::default());

    let outcome = pane.on_action(
        GeneralAction::SwitchTypoRule(TypoRule::Suggest),
        &mut PlaygroundPane::new(),
    );

    let is_on = switch(&default_page(&pane), TYPO_RULE).is_on;
    assert_eq!(
        (outcome, is_on),
        (settings_outcome(typo_settings(TypoRule::Suggest)), false)
    );
}

#[test]
fn hiding_suggestions_on_typos_keeps_it_and_turns_the_switch_on() {
    let mut pane = GeneralPane::new(typo_settings(TypoRule::Suggest));

    let outcome = pane.on_action(
        GeneralAction::SwitchTypoRule(TypoRule::HideSuggestions),
        &mut PlaygroundPane::new(),
    );

    let is_on = switch(&default_page(&pane), TYPO_RULE).is_on;
    assert_eq!((outcome, is_on), (settings_outcome(Settings::default()), true));
}

#[test]
fn leaving_typos_keeps_it_and_turns_the_fix_switch_off() {
    let mut pane = GeneralPane::new(Settings::default());

    let outcome = pane.on_action(
        GeneralAction::SwitchTypoFix(TypoFix::Off),
        &mut PlaygroundPane::new(),
    );

    let is_on = switch(&default_page(&pane), TYPO_FIX).is_on;
    let saved = Settings {
        typo_fix: TypoFix::Off,
        ..Settings::default()
    };
    assert_eq!((outcome, is_on), (settings_outcome(saved), false));
}

#[test]
fn fixing_typos_keeps_it_and_turns_the_fix_switch_on() {
    let mut pane = GeneralPane::new(Settings {
        typo_fix: TypoFix::Off,
        ..Settings::default()
    });

    let outcome = pane.on_action(
        GeneralAction::SwitchTypoFix(TypoFix::Offer),
        &mut PlaygroundPane::new(),
    );

    let is_on = switch(&default_page(&pane), TYPO_FIX).is_on;
    assert_eq!((outcome, is_on), (settings_outcome(Settings::default()), true));
}

#[test]
fn suggesting_on_typos_hides_the_fix_switch() {
    let pane = GeneralPane::new(typo_settings(TypoRule::Suggest));

    let shown = default_page(&pane);

    assert_eq!(shown.row(TYPO_FIX), None);
}

#[test]
fn choosing_the_current_typo_rule_does_nothing() {
    let mut pane = GeneralPane::new(Settings::default());

    let outcome = pane.on_action(
        GeneralAction::SwitchTypoRule(TypoRule::HideSuggestions),
        &mut PlaygroundPane::new(),
    );

    assert_eq!(outcome, GeneralOutcome::default());
}

#[test]
fn stopping_at_the_end_of_a_sentence_keeps_it_and_turns_the_switch_on() {
    let mut pane = GeneralPane::new(Settings {
        sentence_end: SentenceEnd::Continue,
        ..Settings::default()
    });

    let outcome = pane.on_action(
        GeneralAction::SwitchSentenceEnd(SentenceEnd::Stop),
        &mut PlaygroundPane::new(),
    );

    let shown = row(&default_page(&pane), SENTENCE_END);
    assert_eq!(
        (outcome, shown),
        (
            settings_outcome(Settings::default()),
            sentence_end_switch(true, None)
        )
    );
}

#[test]
fn continuing_after_a_sentence_keeps_it_and_turns_the_switch_off() {
    let mut pane = GeneralPane::new(Settings::default());

    let outcome = pane.on_action(
        GeneralAction::SwitchSentenceEnd(SentenceEnd::Continue),
        &mut PlaygroundPane::new(),
    );

    let is_on = switch(&default_page(&pane), SENTENCE_END).is_on;
    let saved = Settings {
        sentence_end: SentenceEnd::Continue,
        ..Settings::default()
    };
    assert_eq!((outcome, is_on), (settings_outcome(saved), false));
}

#[test]
fn sentence_end_switch_that_is_off_tells_that_suggestions_get_worse() {
    let pane = GeneralPane::new(Settings {
        sentence_end: SentenceEnd::Continue,
        ..Settings::default()
    });

    let shown = row(&default_page(&pane), SENTENCE_END);

    let note = Note {
        text: "Suggestions are often wrong after the first sentence. Turn on this switch to stop them there."
            .to_owned(),
        tone: NoteTone::Warning,
    };
    assert_eq!(shown, sentence_end_switch(false, Some(note)));
}

#[test]
fn selecting_a_suggestion_length_keeps_it_and_selects_its_entry() {
    let mut pane = GeneralPane::new(settings(Backend::Vulkan));

    let outcome = pane.on_action(
        GeneralAction::SelectSuggestionLength(SuggestionLength::VeryShort),
        &mut PlaygroundPane::new(),
    );

    let selected = picker(&default_page(&pane), LENGTH).selected;
    let short_settings = Settings {
        suggestion_length: SuggestionLength::VeryShort,
        ..settings(Backend::Vulkan)
    };
    assert_eq!((outcome, selected), (settings_outcome(short_settings), 0));
}

#[test]
fn selecting_the_current_suggestion_length_does_nothing() {
    let mut pane = GeneralPane::new(Settings::default());

    let outcome = pane.on_action(
        GeneralAction::SelectSuggestionLength(SuggestionLength::Medium),
        &mut PlaygroundPane::new(),
    );

    assert_eq!(outcome, GeneralOutcome::default());
}
