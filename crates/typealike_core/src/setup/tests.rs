//! The tests of the setup.

use super::*;
use crate::hub::{ByteSize, HubEvent, SizedFile};
use crate::inference::{Backend, InferenceError};
use crate::settings::{Autostart, Settings};
use crate::settings_window::view::{Accessory, Item, PermissionRow};
use crate::test_support::{english, model_file, toggled, writing_rule_breaks};

struct NoAutostart;

impl Autostart for NoAutostart {
    fn is_enabled(&self) -> bool {
        false
    }

    fn enable(&mut self) {}

    fn disable(&mut self) {}
}

/// The panes and the platform that the setup reads.
struct Fixture {
    general: GeneralPane,
    models: ModelsPane,
    playground: PlaygroundPane,
    personalization: PersonalizationSettings,
    screen_text: ScreenTextSettings,
    permissions: Vec<(Permission, PermissionState)>,
    tray: TrayPlace,
    translations: Translations,
}

impl Fixture {
    /// Windows on the processor, in English, with no model.
    fn new() -> Self {
        Self::on(Backend::Cpu)
    }

    fn on(backend: Backend) -> Self {
        let settings = Settings {
            backend,
            ..Settings::default()
        };
        Self {
            general: GeneralPane::new(settings).with_autostart(Box::new(NoAutostart)),
            models: ModelsPane::new(),
            playground: PlaygroundPane::with_backend(backend),
            personalization: PersonalizationSettings::default(),
            screen_text: ScreenTextSettings::default(),
            permissions: Vec::new(),
            tray: TrayPlace::NotificationArea,
            translations: english(),
        }
    }

    /// macOS, where the user did not give the Accessibility permission yet.
    fn without_accessibility() -> Self {
        Self {
            permissions: vec![
                (Permission::Accessibility, PermissionState::Missing),
                (Permission::ScreenRecording, PermissionState::Missing),
            ],
            tray: TrayPlace::MenuBar,
            ..Self::new()
        }
    }

    fn with_model(mut self) -> Self {
        self.models.on_event(HubEvent::Installed(vec![SizedFile {
            file: model_file("model.gguf"),
            size: ByteSize(1),
        }]));
        self
    }

    /// Presses the download button of the recommended model.
    fn with_download(mut self) -> Self {
        let backend = self.general.settings().backend;
        let Some(Row::Item(Item {
            accessory: Accessory::Download(download),
            ..
        })) = self.models.recommended_row(backend, &self.translations)
        else {
            panic!("the recommended model shows with its download button");
        };
        let action =
            self.models
                .action_for(&SettingsInput::Pressed(download.id), backend, &self.translations);
        self.models
            .on_action(action.expect("the button downloads the model"));
        self
    }

    fn view(&self, flow: &SetupFlow) -> SetupView {
        let installed = self.models.installed_files();
        flow.view(&self.context(&installed))
    }

    fn act(&self, flow: &mut SetupFlow, action: SetupAction) -> SetupOutcome {
        let installed = self.models.installed_files();
        flow.on_action(action, &self.context(&installed))
    }

    fn context<'context>(&'context self, installed: &'context [ModelFile]) -> SetupContext<'context> {
        SetupContext {
            general: &self.general,
            models: &self.models,
            playground: &self.playground,
            installed,
            personalization: &self.personalization,
            screen_text: &self.screen_text,
            permissions: &self.permissions,
            tray: self.tray,
            translations: &self.translations,
        }
    }
}

fn flow_at(step: SetupStep) -> SetupFlow {
    SetupFlow {
        step,
        ..SetupFlow::new()
    }
}

fn button(id: &'static str, label: &str, is_enabled: bool) -> Button {
    Button {
        id: ControlId::new(id),
        label: label.to_owned(),
        is_enabled,
    }
}

fn rows(view: &SetupView) -> Vec<&Row> {
    view.sections.iter().flat_map(|section| &section.rows).collect()
}

/// What starts each section: the title of its first item, else the id of its first control.
fn section_starts(view: &SetupView) -> Vec<&str> {
    view.sections
        .iter()
        .filter_map(|section| match section.rows.first()? {
            Row::Item(item) => Some(item.title.as_str()),
            row => row.id().map(ControlId::as_str),
        })
        .collect()
}

fn item_titles(view: &SetupView) -> Vec<&str> {
    rows(view)
        .into_iter()
        .filter_map(|row| match row {
            Row::Item(Item { title, .. }) => Some(title.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn welcome_shows_what_typealike_does_with_a_demo() {
    let fixture = Fixture::new();

    let view = fixture.view(&SetupFlow::new());

    assert_eq!(
        view,
        SetupView {
            window_title: "TypeAlike setup".to_owned(),
            step: SetupStep::Welcome,
            progress: StepProgress {
                number: 1,
                count: 4,
                text: "Step 1 of 4".to_owned(),
            },
            title: "Welcome to TypeAlike".to_owned(),
            body: "TypeAlike suggests the next words while you type, in any app. Press Tab to type the next word, or keep typing to ignore the suggestion.".to_owned(),
            demo: Some(Demo {
                typed: "Thanks for the quick reply. I will send you the".to_owned(),
                next_part: " report".to_owned(),
                rest: " by Friday.".to_owned(),
                key: "Tab".to_owned(),
            }),
            sections: vec![fixture.general.language_section(&english())],
            footnote: None,
            back: None,
            next: button(NEXT, "Start setup", true),
            language: UiLanguage::English,
        }
    );
}

#[test]
fn demo_key_types_only_the_next_word_as_tab_does() {
    let parts = UiLanguage::ALL.map(|language| {
        let fixture = Fixture {
            translations: Translations::new(language),
            ..Fixture::new()
        };
        fixture
            .view(&SetupFlow::new())
            .demo
            .map(|demo| (demo.next_part, demo.rest))
    });

    assert_eq!(
        parts,
        [
            Some((" Bericht".to_owned(), " bis Freitag.".to_owned())),
            Some((" report".to_owned(), " by Friday.".to_owned())),
            Some((" informe".to_owned(), " el viernes.".to_owned())),
            Some((" rapport".to_owned(), " d’ici vendredi.".to_owned())),
            Some((" resoconto".to_owned(), " entro venerdì.".to_owned())),
            Some((" relatório".to_owned(), " até sexta.".to_owned())),
        ]
    );
}

#[test]
fn os_that_asks_for_accessibility_adds_the_permission_step() {
    let cases = [Fixture::new(), Fixture::without_accessibility()];

    let counts = cases.map(|fixture| fixture.view(&SetupFlow::new()).progress.count);

    assert_eq!(counts, [4, 5]);
}

#[test]
fn next_goes_through_each_step_in_order() {
    let fixture = Fixture {
        permissions: vec![(Permission::Accessibility, PermissionState::Granted)],
        ..Fixture::new().with_model()
    };
    let mut flow = SetupFlow::new();

    let steps = [(); 4].map(|()| {
        fixture.act(&mut flow, SetupAction::Next);
        fixture.view(&flow).step
    });

    assert_eq!(
        steps,
        [
            SetupStep::Model,
            SetupStep::Permission,
            SetupStep::About,
            SetupStep::Ready
        ]
    );
}

#[test]
fn back_goes_to_the_step_before() {
    let fixture = Fixture::new();
    let mut flow = flow_at(SetupStep::About);

    fixture.act(&mut flow, SetupAction::Back);

    assert_eq!(fixture.view(&flow).step, SetupStep::Model);
}

#[test]
fn each_step_after_the_first_has_a_back_button() {
    let fixture = Fixture::new();

    let back = fixture.view(&flow_at(SetupStep::Model)).back;

    assert_eq!(back, Some(button(BACK, "Back", true)));
}

#[test]
fn model_step_shows_the_backend_then_the_recommended_model_then_the_other_models() {
    let fixture = Fixture::new();

    let view = fixture.view(&flow_at(SetupStep::Model));

    assert_eq!(
        (
            view.title.as_str(),
            section_starts(&view),
            view.sections[1].footer.as_deref()
        ),
        (
            "Download the model",
            vec!["backend", "Qwen3.5 0.8B", OTHER_MODELS],
            Some("You download the model one time. The setup continues when the download finishes.")
        )
    );
}

#[test]
fn model_step_with_a_model_shows_the_model_menu_above_the_link() {
    let fixture = Fixture::new().with_model();

    let view = fixture.view(&flow_at(SetupStep::Model));

    assert_eq!(
        (view.title.as_str(), section_starts(&view)),
        (
            "Select the model",
            vec!["backend", "suggestion-model", OTHER_MODELS]
        )
    );
}

#[test]
fn other_models_are_a_quiet_link() {
    let fixture = Fixture::new();

    let view = fixture.view(&flow_at(SetupStep::Model));

    assert_eq!(
        view.sections.last().map(|section| section.rows.clone()),
        Some(vec![Row::Link(button(
            OTHER_MODELS,
            "Choose another model",
            true
        ))])
    );
}

#[test]
fn other_models_show_the_search_and_the_models_of_the_models_page() {
    let fixture = Fixture::new();
    let mut flow = flow_at(SetupStep::Model);

    fixture.act(&mut flow, SetupAction::ShowOtherModels);

    let view = fixture.view(&flow);
    let headings: Vec<&str> = view
        .sections
        .iter()
        .filter_map(|section| section.heading.as_deref())
        .collect();
    let starts = section_starts(&view);
    assert_eq!(
        (starts[..2].to_vec(), starts.contains(&OTHER_MODELS), headings),
        (
            vec!["backend", "models-search"],
            false,
            vec!["Recommended models"]
        )
    );
}

#[test]
fn new_step_shows_the_recommended_model_again() {
    let fixture = Fixture::new();
    let mut flow = flow_at(SetupStep::Model);
    fixture.act(&mut flow, SetupAction::ShowOtherModels);

    fixture.act(&mut flow, SetupAction::Back);
    fixture.act(&mut flow, SetupAction::Next);

    assert_eq!(
        section_starts(&fixture.view(&flow)),
        vec!["backend", "Qwen3.5 0.8B", OTHER_MODELS]
    );
}

#[test]
fn model_step_waits_until_the_download_finishes() {
    let cases = [
        Fixture::new(),
        Fixture::new().with_download(),
        Fixture::new().with_model(),
    ];

    let enabled = cases.map(|fixture| fixture.view(&flow_at(SetupStep::Model)).next.is_enabled);

    assert_eq!(enabled, [false, false, true]);
}

#[test]
fn next_does_nothing_while_the_model_downloads() {
    let fixture = Fixture::new().with_download();
    let mut flow = flow_at(SetupStep::Model);

    fixture.act(&mut flow, SetupAction::Next);

    assert_eq!(fixture.view(&flow).step, SetupStep::Model);
}

#[test]
fn next_does_nothing_while_the_step_does_not_allow_it() {
    let fixture = Fixture::new();
    let mut flow = flow_at(SetupStep::Model);

    fixture.act(&mut flow, SetupAction::Next);

    assert_eq!(fixture.view(&flow).step, SetupStep::Model);
}

#[test]
fn model_step_checks_the_graphics_card_before_its_recommendation() {
    let mut fixture = Fixture::on(Backend::Vulkan);
    fixture.models.start_gpu_memory_read(Backend::Vulkan);

    let view = fixture.view(&flow_at(SetupStep::Model));

    assert_eq!(
        view.sections[1].rows,
        vec![Row::Loading("Checking the graphics card…".to_owned())]
    );
}

#[test]
fn model_step_warns_when_it_cannot_read_the_graphics_card() {
    let mut fixture = Fixture::on(Backend::Vulkan);
    fixture
        .models
        .on_gpu_memory(Backend::Vulkan, Err(InferenceError::BackendUnavailable));

    let view = fixture.view(&flow_at(SetupStep::Model));

    assert_eq!(
        (view.sections[1].rows.first(), item_titles(&view)),
        (
            fixture
                .models
                .recommendation_warning(Backend::Vulkan, &english())
                .as_ref(),
            vec!["Gemma 4 E2B"]
        )
    );
}

#[test]
fn permission_step_waits_for_accessibility() {
    let mut fixture = Fixture::without_accessibility();
    let missing = fixture.view(&flow_at(SetupStep::Permission));

    fixture.permissions[0].1 = PermissionState::Granted;

    let granted = fixture.view(&flow_at(SetupStep::Permission));
    assert_eq!((missing.next.is_enabled, granted.next.is_enabled), (false, true));
}

#[test]
fn permission_step_shows_only_accessibility() {
    let fixture = Fixture::without_accessibility();

    let view = fixture.view(&flow_at(SetupStep::Permission));

    let labels: Vec<&str> = rows(&view)
        .into_iter()
        .filter_map(|row| match row {
            Row::Permission(PermissionRow { label, .. }) => Some(label.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        (view.title.as_str(), labels),
        ("Give TypeAlike access to text fields", vec!["Accessibility"])
    );
}

#[test]
fn about_step_shows_the_writer_and_what_suggestions_can_use() {
    let mut fixture = Fixture::new();
    fixture.personalization.history = HistoryAccess::Off;

    let view = fixture.view(&flow_at(SetupStep::About));

    let switches: Vec<(&str, bool)> = rows(&view)
        .into_iter()
        .filter_map(|row| match row {
            Row::Switch(switch) => Some((switch.id.as_str(), switch.is_on)),
            _ => None,
        })
        .collect();
    assert_eq!(
        (view.sections[0].clone(), switches),
        (
            fixture.general.writer_section(&english()),
            vec![(HISTORY, false), (CLIPBOARD, true), (SCREEN_TEXT, true)]
        )
    );
}

#[test]
fn ready_step_tells_where_to_try_typealike_and_where_its_icon_is() {
    let fixture = Fixture::new().with_model();

    let view = fixture.view(&flow_at(SetupStep::Ready));

    assert_eq!(
        (view.title.as_str(), view.body.as_str(), view.footnote, view.next),
        (
            "TypeAlike is ready",
            "Try it now in any app, such as your email or a chat.",
            Some("TypeAlike stays in the notification area of the taskbar. Use its icon to pause suggestions or to open the settings.".to_owned()),
            button(NEXT, "Finish setup", true),
        )
    );
}

#[test]
fn ready_step_names_the_menu_bar_on_macos() {
    let fixture = Fixture::without_accessibility();

    let footnote = fixture.view(&flow_at(SetupStep::Ready)).footnote;

    assert_eq!(
        footnote.as_deref(),
        Some("TypeAlike stays in the menu bar. Use its icon to pause suggestions or to open the settings.")
    );
}

#[test]
fn autostart_switch_is_on_by_default() {
    let fixture = Fixture::new().with_model();

    let view = fixture.view(&flow_at(SetupStep::Ready));

    let autostart = rows(&view).into_iter().find_map(|row| match row {
        Row::Switch(switch) if switch.id.as_str() == AUTOSTART => Some(switch.is_on),
        _ => None,
    });
    assert_eq!(autostart, Some(true));
}

#[test]
fn finish_applies_the_choice_of_the_autostart_switch() {
    let fixture = Fixture::new().with_model();
    let mut kept = flow_at(SetupStep::Ready);
    let mut turned_off = flow_at(SetupStep::Ready);
    fixture.act(&mut turned_off, SetupAction::SwitchAutostart(false));

    let outcomes = [&mut kept, &mut turned_off].map(|flow| fixture.act(flow, SetupAction::Next));

    assert_eq!(
        outcomes,
        [
            SetupOutcome::Finished(GeneralAction::EnableAutostart),
            SetupOutcome::Finished(GeneralAction::DisableAutostart),
        ]
    );
}

#[test]
fn setup_controls_give_their_actions() {
    let inputs = [
        SettingsInput::Pressed(ControlId::new(BACK)),
        SettingsInput::Pressed(ControlId::new(NEXT)),
        toggled(AUTOSTART, false),
        SettingsInput::Pressed(ControlId::new(OTHER_MODELS)),
        toggled(HISTORY, false),
    ];

    let actions = inputs.map(|input| SetupFlow::action_for(&input));

    assert_eq!(
        actions,
        [
            Some(SetupAction::Back),
            Some(SetupAction::Next),
            Some(SetupAction::SwitchAutostart(false)),
            Some(SetupAction::ShowOtherModels),
            None
        ]
    );
}

#[test]
fn each_step_follows_the_writing_rules() {
    let fixtures = [
        Fixture::without_accessibility(),
        Fixture::new().with_download(),
        Fixture::new().with_model(),
    ];
    let steps = [
        SetupStep::Welcome,
        SetupStep::Model,
        SetupStep::Permission,
        SetupStep::About,
        SetupStep::Ready,
    ];

    let other_models = SetupFlow {
        model_choice: ModelChoice::Other,
        ..flow_at(SetupStep::Model)
    };

    let texts: Vec<_> = fixtures
        .iter()
        .flat_map(|fixture| {
            steps
                .map(|step| fixture.view(&flow_at(step)))
                .into_iter()
                .chain([fixture.view(&other_models)])
        })
        .flat_map(|view| view.texts())
        .collect();

    assert_eq!(writing_rule_breaks(&texts), Vec::<String>::new());
}

#[test]
fn setup_shows_in_spanish() {
    let fixture = Fixture {
        translations: Translations::new(UiLanguage::Spanish),
        ..Fixture::new()
    };

    let view = fixture.view(&SetupFlow::new());

    assert_eq!(
        (view.title.as_str(), view.progress.text.as_str(), view.language),
        (
            "Te damos la bienvenida a TypeAlike",
            "Paso 1 de 4",
            UiLanguage::Spanish
        )
    );
}

#[test]
fn about_step_tells_what_uses_the_internet() {
    let fixture = Fixture::new();

    let view = fixture.view(&flow_at(SetupStep::About));

    assert_eq!(
        view.sections.last().and_then(|section| section.footer.as_deref()),
        Some(
            "TypeAlike uses the internet only to find and download models. It does not save the text of password fields."
        )
    );
}
