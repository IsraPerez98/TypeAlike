//! The fake shell, the fake services and the test harness that the controller tests share.

use std::cell::RefCell;
use std::mem;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::app::{BadgeView, TrayCommand, TrayView};
use crate::controller::{
    BackendChoice, CaretLook, CaretRead, Controller, Delivery, DeliverySink, FocusChange, Ghost, GhostView,
    MAX_CARET_MOVE_CHECKS, Services, Shell, Timer,
};
use crate::desktop::{
    CopiedText, Desktop, Field, FieldText, OcrAvailability, ScreenReadId, ScreenSource, Surroundings,
};
use crate::history::{HistoryEntry, WritingHistory};
use crate::hub::{ByteSize, HubEvent, HubEventSink, HubRequest, ModelFile, ModelHub, SizedFile};
use crate::inference::{
    Completion, Correction, InferenceEvent, InferenceEventSink, InferenceRequest, ModelRunner,
};
use crate::key::{Disposition, Key};
use crate::settings::{
    Autostart, ClipboardAccess, HistoryAccess, HistoryScope, KeptTexts, PersonalizationSettings, Settings,
    SettingsStore, WordChoice,
};
use crate::settings_window::view::{ControlId, SettingsInput, SettingsView};
use crate::test_support::{self, completion, mail_history_app};

/// The width of each character of a ghost text after the caret in the fake shell.
const GHOST_CHAR_WIDTH: i32 = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Call {
    ReadCaret,
    RenderAtCaret {
        ghost: Option<Ghost>,
        badge: Option<BadgeView>,
    },
    HideGhost,
    HideBadge,
    ReplaceText(usize, String),
    StartTimer(Timer, Duration),
    StopTimer(Timer),
    RenderTray(TrayView),
    RenderSettings(Box<SettingsView>),
    ShowSettings(Box<SettingsView>),
    UseClipboardAccess(ClipboardAccess),
    ReadScreen(ScreenReadId, ScreenSource),
    AskForOcrPermission,
    Quit,
}

pub(super) struct FakeShell {
    pub(super) field: Field,
    pub(super) surroundings: Surroundings,
    pub(super) focus_change: FocusChange,
    pub(super) ocr: OcrAvailability,
    calls: Vec<Call>,
    pub(super) ghost_view: Option<GhostView>,
    pub(super) is_ghost_shown: bool,
    /// How the shell looks at the caret. A shell that looks later records each look as a call.
    pub(super) caret_look: CaretLook,
}

impl Desktop for FakeShell {
    fn field(&self) -> Field {
        self.field
    }

    fn surroundings(&self) -> Surroundings {
        self.surroundings.clone()
    }

    fn copied_text(&self) -> Option<CopiedText> {
        None
    }

    fn ocr_availability(&self) -> OcrAvailability {
        self.ocr
    }

    fn is_ghost_shown(&self) -> bool {
        self.is_ghost_shown && self.ghost_view.is_some()
    }
}

impl Shell for FakeShell {
    fn refresh_focus(&mut self) -> FocusChange {
        mem::replace(&mut self.focus_change, FocusChange::Unchanged)
    }

    fn read_caret(&mut self) -> CaretLook {
        if self.caret_look == CaretLook::Later {
            self.calls.push(Call::ReadCaret);
        }
        self.caret_look
    }

    /// Measures the ghost text after the caret with the space that the platforms draw before the
    /// word of a fix.
    fn ghost_width(&self, ghost: &Ghost) -> i32 {
        let characters = match ghost {
            Ghost::Text(text) => text.chars().count(),
            Ghost::Fix(fix) => 1 + fix.word.chars().count() + fix.rest.chars().count(),
        };
        i32::try_from(characters).expect("a test ghost text is short") * GHOST_CHAR_WIDTH
    }

    fn render_at_caret(&mut self, ghost: Option<&GhostView>, badge: Option<BadgeView>) {
        self.ghost_view = ghost.cloned();
        self.calls.push(Call::RenderAtCaret {
            ghost: ghost.map(|view| view.ghost.clone()),
            badge,
        });
    }

    fn hide_ghost(&mut self) {
        self.ghost_view = None;
        self.calls.push(Call::HideGhost);
    }

    fn hide_badge(&mut self) {
        self.calls.push(Call::HideBadge);
    }

    fn replace_before_caret(&mut self, deleted: usize, text: String) {
        self.calls.push(Call::ReplaceText(deleted, text));
    }

    fn start_timer(&mut self, timer: Timer, delay: Duration) {
        self.calls.push(Call::StartTimer(timer, delay));
    }

    fn stop_timer(&mut self, timer: Timer) {
        self.calls.push(Call::StopTimer(timer));
    }

    fn render_tray(&mut self, view: &TrayView) {
        self.calls.push(Call::RenderTray(view.clone()));
    }

    fn render_settings(&mut self, view: &SettingsView) {
        self.calls.push(Call::RenderSettings(Box::new(view.clone())));
    }

    fn show_settings(&mut self, view: &SettingsView) {
        self.calls.push(Call::ShowSettings(Box::new(view.clone())));
    }

    fn use_clipboard_access(&mut self, access: ClipboardAccess) {
        self.calls.push(Call::UseClipboardAccess(access));
    }

    fn read_screen(&mut self, read: ScreenReadId, source: ScreenSource) {
        self.calls.push(Call::ReadScreen(read, source));
    }

    fn ask_for_ocr_permission(&mut self) {
        self.calls.push(Call::AskForOcrPermission);
    }

    fn quit(&mut self) {
        self.calls.push(Call::Quit);
    }
}

#[derive(Default)]
struct Recorder {
    inference: Vec<(InferenceRequest, InferenceEventSink)>,
    hub: Vec<HubRequest>,
    saved: Vec<Settings>,
    kept_history: Vec<HistoryEntry>,
    history_loads: usize,
    history_saves: Vec<Vec<HistoryEntry>>,
}

struct FakeRunner(Rc<RefCell<Recorder>>);

impl ModelRunner for FakeRunner {
    fn submit(&mut self, request: InferenceRequest, sink: &InferenceEventSink) {
        self.0.borrow_mut().inference.push((request, Arc::clone(sink)));
    }
}

struct FakeHub(Rc<RefCell<Recorder>>);

impl ModelHub for FakeHub {
    fn submit(&mut self, request: HubRequest, _sink: &HubEventSink) {
        self.0.borrow_mut().hub.push(request);
    }
}

struct FakeStore(Rc<RefCell<Recorder>>);

impl SettingsStore for FakeStore {
    fn save(&mut self, settings: &Settings) {
        self.0.borrow_mut().saved.push(settings.clone());
    }
}

struct FakeHistory(Rc<RefCell<Recorder>>);

impl WritingHistory for FakeHistory {
    fn load(&mut self) -> Vec<HistoryEntry> {
        let mut recorder = self.0.borrow_mut();
        recorder.history_loads += 1;
        recorder.kept_history.clone()
    }

    fn save(&mut self, entries: &[HistoryEntry]) {
        self.0.borrow_mut().history_saves.push(entries.to_vec());
    }
}

struct FakeAutostart;

impl Autostart for FakeAutostart {
    fn is_enabled(&self) -> bool {
        false
    }

    fn enable(&mut self) {}

    fn disable(&mut self) {}
}

pub(super) struct Harness {
    pub(super) controller: Controller,
    pub(super) shell: FakeShell,
    recorder: Rc<RefCell<Recorder>>,
    deliveries: Arc<Mutex<Vec<Delivery>>>,
}

impl Harness {
    pub(super) fn new(settings: Settings) -> Self {
        Self::with_backend_choice(settings, BackendChoice::User)
    }

    pub(super) fn with_backend_choice(settings: Settings, backend_choice: BackendChoice) -> Self {
        Self::start(settings, backend_choice, Vec::new())
    }

    pub(super) fn with_kept_history(settings: Settings, kept_history: Vec<HistoryEntry>) -> Self {
        Self::start(settings, BackendChoice::User, kept_history)
    }

    /// Starts the controller with the saved `settings`, and a history store that holds
    /// `kept_history`.
    fn start(settings: Settings, backend_choice: BackendChoice, kept_history: Vec<HistoryEntry>) -> Self {
        let recorder = Rc::new(RefCell::new(Recorder {
            kept_history,
            ..Recorder::default()
        }));
        let deliveries = Arc::new(Mutex::new(Vec::new()));
        let received = Arc::clone(&deliveries);
        let deliver: DeliverySink = Arc::new(move |delivery| {
            received
                .lock()
                .expect("no test panics while it holds the lock")
                .push(delivery);
        });
        let services = Services {
            hub: Box::new(FakeHub(Rc::clone(&recorder))),
            runner: Box::new(FakeRunner(Rc::clone(&recorder))),
            settings_store: Box::new(FakeStore(Rc::clone(&recorder))),
            autostart: Box::new(FakeAutostart),
            history: Box::new(FakeHistory(Rc::clone(&recorder))),
        };
        Self {
            controller: Controller::new(settings, services, backend_choice, &deliver),
            shell: FakeShell {
                field: Field::Open,
                surroundings: Surroundings::default(),
                focus_change: FocusChange::Unchanged,
                ocr: OcrAvailability::Unsupported,
                calls: Vec::new(),
                ghost_view: None,
                is_ghost_shown: true,
                caret_look: CaretLook::Now(CaretRead::default()),
            },
            recorder,
            deliveries,
        }
    }

    pub(super) fn press(&mut self, key: Key) -> Disposition {
        self.controller.on_key_down(Some(key), &mut self.shell)
    }

    /// Presses the key of each character of `text`.
    pub(super) fn type_text(&mut self, text: &str) {
        for character in text.chars() {
            self.press(Key::Char(character));
        }
    }

    pub(super) fn settle_caret(&mut self) {
        self.controller.on_timer(Timer::CaretSettle, &mut self.shell);
    }

    pub(super) fn scroll(&mut self) {
        self.controller.on_scroll(&mut self.shell);
    }

    /// Settles the caret once for each check that waits for the caret to move.
    pub(super) fn run_out_caret_checks(&mut self) {
        for _ in 0..MAX_CARET_MOVE_CHECKS {
            self.settle_caret();
        }
    }

    /// Gives `read` to the controller as the answer of the shell to its last look at the caret.
    pub(super) fn answer_caret(&mut self, read: CaretRead) {
        self.controller.on_caret_read(read, &mut self.shell);
    }

    /// Settles the caret and answers the look with `read`, once for each check that waits for the
    /// caret to move.
    pub(super) fn run_out_caret_checks_at(&mut self, read: CaretRead) {
        for _ in 0..MAX_CARET_MOVE_CHECKS {
            self.settle_caret();
            self.answer_caret(read);
        }
    }

    /// Answers the last inference request with `event`, through the sink of the request.
    pub(super) fn answer(&mut self, event: InferenceEvent) {
        let sink = Arc::clone(&self.recorder.borrow().inference.last().expect("a request").1);
        sink(event);
        let deliveries = mem::take(
            &mut *self
                .deliveries
                .lock()
                .expect("no test panics while it holds the lock"),
        );
        for delivery in deliveries {
            self.controller.on_delivery(delivery, &mut self.shell);
        }
    }

    pub(super) fn show(&mut self, suggestion: &str) {
        self.press(Key::Char('B'));
        self.answer(completed(suggestion));
        self.settle_caret();
        self.take_calls();
    }

    pub(super) fn take_calls(&mut self) -> Vec<Call> {
        mem::take(&mut self.shell.calls)
    }

    pub(super) fn requests(&self) -> Vec<InferenceRequest> {
        self.recorder
            .borrow()
            .inference
            .iter()
            .map(|(request, _)| request.clone())
            .collect()
    }

    pub(super) fn last_request(&self) -> Option<InferenceRequest> {
        self.requests().pop()
    }

    pub(super) fn hub_requests(&self) -> Vec<HubRequest> {
        self.recorder.borrow().hub.clone()
    }

    pub(super) fn saved(&self) -> Vec<Settings> {
        self.recorder.borrow().saved.clone()
    }

    pub(super) fn history_loads(&self) -> usize {
        self.recorder.borrow().history_loads
    }

    pub(super) fn history_saves(&self) -> Vec<Vec<HistoryEntry>> {
        self.recorder.borrow().history_saves.clone()
    }

    pub(super) fn settle_input(&mut self) {
        self.controller.on_timer(Timer::InputSettle, &mut self.shell);
    }

    pub(super) fn toggle_pause(&mut self) {
        self.controller
            .on_tray_command(TrayCommand::TogglePause, &mut self.shell);
    }

    pub(super) fn press_button(&mut self, id: &'static str) {
        let input = SettingsInput::Pressed(ControlId::new(id));
        self.controller.on_settings_input(&input, &mut self.shell);
    }

    pub(super) fn toggle(&mut self, id: &'static str, is_on: bool) {
        let input = SettingsInput::Toggled {
            id: ControlId::new(id),
            is_on,
        };
        self.controller.on_settings_input(&input, &mut self.shell);
    }

    pub(super) fn pick(&mut self, id: &'static str, index: usize) {
        let input = SettingsInput::Picked {
            id: ControlId::new(id),
            index,
        };
        self.controller.on_settings_input(&input, &mut self.shell);
    }

    pub(super) fn install(&mut self, file: ModelFile) {
        let installed = HubEvent::Installed(vec![SizedFile {
            file,
            size: ByteSize(1),
        }]);
        self.controller
            .on_delivery(Delivery::Hub(installed), &mut self.shell);
    }

    /// Puts the focus in the text field of an app.
    pub(super) fn focus_text_field_of(&mut self, app_id: &str, app_name: &str) {
        self.shell.surroundings = Surroundings {
            app_id: Some(app_id.to_owned()),
            app_name: Some(app_name.to_owned()),
            field_text: Some(FieldText::default()),
            ..Surroundings::default()
        };
    }

    /// Settles the input, and returns the read of the screen that the controller started.
    pub(super) fn settle_input_to_read_screen(&mut self) -> ScreenReadId {
        self.settle_input();
        match self.take_calls().pop() {
            Some(Call::ReadScreen(read, ScreenSource::Accessibility)) => read,
            other => panic!("the controller did not read the screen: {other:?}"),
        }
    }

    pub(super) fn shown_settings(&mut self) -> SettingsView {
        self.controller
            .on_tray_command(TrayCommand::OpenSettings, &mut self.shell);
        match self.take_calls().pop() {
            Some(Call::ShowSettings(view)) => *view,
            other => panic!("the settings window did not show: {other:?}"),
        }
    }
}

pub(super) fn model_file() -> ModelFile {
    test_support::model_file("model.gguf")
}

pub(super) fn suggestion_settings() -> Settings {
    Settings {
        suggestion_model: Some(model_file()),
        ..Settings::default()
    }
}

pub(super) fn completed(text: &str) -> InferenceEvent {
    InferenceEvent::Completed(completion(text))
}

/// The completion `text` with the fix of the `typed` letters to `fixed`.
pub(super) fn fixed(typed: &str, fixed: &str, text: &str) -> InferenceEvent {
    InferenceEvent::Completed(completion(text).map(|completion| Completion {
        correction: Some(Correction {
            typed: typed.to_owned(),
            fixed: fixed.to_owned(),
        }),
        ..completion
    }))
}

pub(super) fn loaded() -> InferenceEvent {
    InferenceEvent::Loaded {
        file: model_file(),
        result: Ok(()),
    }
}

pub(super) fn ghost_at_caret(ghost: Option<&str>, badge: Option<BadgeView>) -> Call {
    Call::RenderAtCaret {
        ghost: ghost.map(|text| Ghost::Text(text.to_owned())),
        badge,
    }
}

pub(super) fn hidden_ghost_and_badge() -> Vec<Call> {
    vec![
        Call::StopTimer(Timer::CaretSettle),
        Call::HideGhost,
        Call::HideBadge,
    ]
}

pub(super) fn field_around(before_caret: &str, after_caret: &str) -> Surroundings {
    Surroundings {
        field_text: Some(FieldText {
            before_caret: before_caret.to_owned(),
            after_caret: after_caret.to_owned(),
        }),
        ..Surroundings::default()
    }
}

pub(super) fn history_on() -> Settings {
    Settings {
        personalization: PersonalizationSettings {
            history: HistoryAccess::On,
            kept_texts: KeptTexts::All,
            apps: vec![mail_history_app(HistoryScope::AllApps)],
            word_choice: WordChoice::Off,
        },
        ..Settings::default()
    }
}
