//! The fake shell, the fake services and the test harness that the controller tests share.

use std::cell::RefCell;
use std::mem;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::app::{BadgeView, TrayCommand, TrayView};
use crate::controller::{
    BackendChoice, CaretLook, CaretRead, Controller, Delivery, DeliverySink, FocusChange, Ghost, GhostView,
    MAX_CARET_MOVE_CHECKS, PlaygroundPage, Services, Shell, Startup, Timer,
};
use crate::desktop::{
    CopiedText, Desktop, EnergySaver, Field, FieldText, FullScreenActivity, GameActivity, OcrAvailability,
    Permission, PermissionState, ScreenReadId, ScreenSource, Surroundings, Timestamp,
};
use crate::history::{HistoryEntry, WritingHistory};
use crate::hub::{ByteSize, HubEvent, HubEventSink, HubRequest, ModelFile, ModelHub, SizedFile};
use crate::inference::{
    Backend, Completion, Correction, InferenceEvent, InferenceEventSink, InferenceRequest, ModelRunner,
};
use crate::key::{Disposition, Key, Press};
use crate::localization::UiLanguage;
use crate::release::{
    DownloadSink, DownloadedRelease, ReleaseDownloadFailed, ReleaseFeed, ReleaseInstallFailed,
    ReleaseReadFailed, ReleaseSink, ReleaseTag, SelfUpdate,
};
use crate::settings::{
    Autostart, ClipboardAccess, HistoryAccess, HistoryScope, KeptTexts, OcrLevel, PersonalizationSettings,
    ReplyWords, Settings, SettingsStore, Setup, WordChoice, WriterName,
};
use crate::settings_window::view::{ControlId, SettingsInput, SettingsView};
use crate::setup::{SetupView, TrayPlace};
use crate::spelling::{
    DictionaryEventSink, DictionaryHub, DictionaryRequest, DownloadableDictionary, OsDictionaries,
    SpellChecker,
};
use crate::test_support::{self, FakeSpellChecker, completion, mail_history_app};

/// The width of each character of a ghost text after the caret in the fake shell.
const GHOST_CHAR_WIDTH: i32 = 10;
/// A word that the dictionary of the harness does not know.
pub(super) const MADE_UP_WORD: &str = "zorbly";
/// A typo that the dictionary of the harness does not know.
pub(super) const TYPO: &str = "teh";

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
    ShowSettings(Box<SettingsView>, Option<usize>),
    ShowSetup(Box<SetupView>),
    RenderSetup(Box<SetupView>),
    CloseSetup,
    UseClipboardAccess(ClipboardAccess),
    ReadScreen(ScreenReadId, ScreenSource),
    WarmUpOcr(OcrLevel),
    AskForPermission(Permission),
    OpenWebPage(String),
    Quit,
}

pub(super) struct FakeShell {
    pub(super) field: Field,
    pub(super) surroundings: Surroundings,
    pub(super) focus_change: FocusChange,
    pub(super) ocr: OcrAvailability,
    pub(super) permissions: Vec<(Permission, PermissionState)>,
    calls: Vec<Call>,
    pub(super) ghost_view: Option<GhostView>,
    pub(super) is_ghost_shown: bool,
    /// How the shell looks at the caret. A shell that looks later records each look as a call.
    pub(super) caret_look: CaretLook,
    pub(super) now: Option<Timestamp>,
    /// How many times the controller asked the shell to wake the text of the focused field.
    pub(super) field_text_wakes: usize,
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

    fn permissions(&self) -> Vec<(Permission, PermissionState)> {
        self.permissions.clone()
    }

    fn now(&self) -> Option<Timestamp> {
        self.now
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

    fn show_settings(&mut self, view: &SettingsView, page: Option<usize>) {
        self.calls.push(Call::ShowSettings(Box::new(view.clone()), page));
    }

    fn show_setup(&mut self, view: &SetupView) {
        self.calls.push(Call::ShowSetup(Box::new(view.clone())));
    }

    fn render_setup(&mut self, view: &SetupView) {
        self.calls.push(Call::RenderSetup(Box::new(view.clone())));
    }

    fn close_setup(&mut self) {
        self.calls.push(Call::CloseSetup);
    }

    fn use_clipboard_access(&mut self, access: ClipboardAccess) {
        self.calls.push(Call::UseClipboardAccess(access));
    }

    fn read_screen(&mut self, read: ScreenReadId, source: ScreenSource) {
        self.calls.push(Call::ReadScreen(read, source));
    }

    fn warm_up_ocr(&mut self, level: OcrLevel) {
        self.calls.push(Call::WarmUpOcr(level));
    }

    fn wake_field_text(&mut self) {
        self.field_text_wakes += 1;
    }

    fn ask_for_permission(&mut self, permission: Permission) {
        self.calls.push(Call::AskForPermission(permission));
    }

    fn open_web_page(&mut self, url: &str) {
        self.calls.push(Call::OpenWebPage(url.to_owned()));
    }

    fn quit(&mut self) {
        self.calls.push(Call::Quit);
    }
}

#[derive(Default)]
struct Recorder {
    inference: Vec<(InferenceRequest, InferenceEventSink)>,
    hub: Vec<HubRequest>,
    dictionaries: Vec<DictionaryRequest>,
    opened_dictionaries: Vec<Vec<String>>,
    saved: Vec<Settings>,
    kept_history: Vec<HistoryEntry>,
    history_loads: usize,
    history_saves: Vec<Vec<HistoryEntry>>,
    history_deletes: usize,
    release_reads: Vec<ReleaseSink>,
    release_downloads: Vec<(ReleaseTag, DownloadSink)>,
    installs: Vec<DownloadedRelease>,
    install_result: Option<Result<(), ReleaseInstallFailed>>,
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

/// The dictionaries of a Windows computer with only English, which do not know the made-up word
/// and the typo of the harness.
struct FakeOsDictionaries(Rc<RefCell<Recorder>>);

impl OsDictionaries for FakeOsDictionaries {
    fn preferred_languages(&self) -> Vec<String> {
        vec!["en-US".to_owned()]
    }

    fn available_languages(&self) -> Vec<String> {
        vec!["en-US".to_owned()]
    }

    fn open(&self, languages: &[String]) -> Box<dyn SpellChecker> {
        self.0.borrow_mut().opened_dictionaries.push(languages.to_vec());
        Box::new(FakeSpellChecker::without(&[MADE_UP_WORD, TYPO]))
    }
}

/// A hub that can download the dictionary of Spanish of Chile, which knows each word.
struct FakeDictionaryHub(Rc<RefCell<Recorder>>);

impl DictionaryHub for FakeDictionaryHub {
    fn catalog(&self) -> Vec<DownloadableDictionary> {
        vec![DownloadableDictionary {
            language: "es-CL".to_owned(),
            size: ByteSize(835_000),
        }]
    }

    fn submit(&mut self, request: DictionaryRequest, _sink: &DictionaryEventSink) {
        self.0.borrow_mut().dictionaries.push(request);
    }

    fn open(&self, languages: &[String]) -> Box<dyn SpellChecker> {
        self.0.borrow_mut().opened_dictionaries.push(languages.to_vec());
        Box::new(FakeSpellChecker::default())
    }
}

struct FakeReleases {
    recorder: Rc<RefCell<Recorder>>,
    self_update: SelfUpdate,
}

impl ReleaseFeed for FakeReleases {
    fn read_newest(&mut self, sink: &ReleaseSink) {
        self.recorder.borrow_mut().release_reads.push(Arc::clone(sink));
    }

    fn self_update(&self) -> SelfUpdate {
        self.self_update
    }

    fn download(&mut self, tag: &ReleaseTag, sink: &DownloadSink) {
        self.recorder
            .borrow_mut()
            .release_downloads
            .push((tag.clone(), Arc::clone(sink)));
    }

    fn install(&mut self, release: &DownloadedRelease) -> Result<(), ReleaseInstallFailed> {
        let mut recorder = self.recorder.borrow_mut();
        recorder.installs.push(release.clone());
        recorder.install_result.unwrap_or(Ok(()))
    }
}

struct FakeStore(Rc<RefCell<Recorder>>);

impl SettingsStore for FakeStore {
    fn save(&mut self, settings: &Settings) {
        self.0.borrow_mut().saved.push(settings.clone());
    }

    fn first_start_settings(&self) -> Settings {
        first_start_settings()
    }
}

/// The settings of the first start in the fake store: the name of the account, the Vulkan
/// backend, and the setup to do.
pub(super) fn first_start_settings() -> Settings {
    Settings {
        backend: Backend::Vulkan,
        writer_name: WriterName("Sara Novak".to_owned()),
        setup: Setup::Needed,
        ..Settings::default()
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

    fn delete(&mut self) {
        self.0.borrow_mut().history_deletes += 1;
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

/// The services of a harness that differ between the tests.
struct FakeServices {
    kept_history: Vec<HistoryEntry>,
    self_update: SelfUpdate,
}

impl Default for FakeServices {
    fn default() -> Self {
        Self {
            kept_history: Vec::new(),
            self_update: SelfUpdate::Unsupported,
        }
    }
}

/// The start of a release build in English on a computer that does not save energy.
fn default_startup() -> Startup {
    Startup {
        backend_choice: BackendChoice::User,
        energy_saver: EnergySaver::Off,
        ui_language: UiLanguage::English,
        playground: PlaygroundPage::Hidden,
        tray: TrayPlace::NotificationArea,
    }
}

impl Harness {
    pub(super) fn new(settings: Settings) -> Self {
        Self::with_backend_choice(settings, BackendChoice::User)
    }

    pub(super) fn with_backend_choice(settings: Settings, backend_choice: BackendChoice) -> Self {
        let startup = Startup {
            backend_choice,
            ..default_startup()
        };
        Self::start(settings, startup, FakeServices::default())
    }

    pub(super) fn with_energy_saver(settings: Settings, energy_saver: EnergySaver) -> Self {
        let startup = Startup {
            energy_saver,
            ..default_startup()
        };
        Self::start(settings, startup, FakeServices::default())
    }

    pub(super) fn with_ui_language(settings: Settings, ui_language: UiLanguage) -> Self {
        let startup = Startup {
            ui_language,
            ..default_startup()
        };
        Self::start(settings, startup, FakeServices::default())
    }

    pub(super) fn with_playground_page(settings: Settings) -> Self {
        let startup = Startup {
            playground: PlaygroundPage::Shown,
            ..default_startup()
        };
        Self::start(settings, startup, FakeServices::default())
    }

    pub(super) fn with_kept_history(settings: Settings, kept_history: Vec<HistoryEntry>) -> Self {
        let services = FakeServices {
            kept_history,
            ..FakeServices::default()
        };
        Self::start(settings, default_startup(), services)
    }

    /// Starts an installed copy of TypeAlike, which can install a new release by itself.
    pub(super) fn installed(settings: Settings) -> Self {
        let services = FakeServices {
            self_update: SelfUpdate::Supported,
            ..FakeServices::default()
        };
        Self::start(settings, default_startup(), services)
    }

    /// Starts the controller with the saved `settings`, the state of the platform at `startup`,
    /// and the fake `services`.
    fn start(settings: Settings, startup: Startup, services: FakeServices) -> Self {
        let recorder = Rc::new(RefCell::new(Recorder {
            kept_history: services.kept_history,
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
            os_dictionaries: Box::new(FakeOsDictionaries(Rc::clone(&recorder))),
            dictionary_hub: Some(Box::new(FakeDictionaryHub(Rc::clone(&recorder)))),
            releases: Box::new(FakeReleases {
                recorder: Rc::clone(&recorder),
                self_update: services.self_update,
            }),
            libraries: &[],
        };
        Self {
            controller: Controller::new(settings, services, startup, &deliver),
            shell: FakeShell {
                field: Field::Open,
                surroundings: Surroundings::default(),
                focus_change: FocusChange::Unchanged,
                ocr: OcrAvailability::Unsupported,
                permissions: Vec::new(),
                calls: Vec::new(),
                ghost_view: None,
                is_ghost_shown: true,
                caret_look: CaretLook::Now(CaretRead::default()),
                now: None,
                field_text_wakes: 0,
            },
            recorder,
            deliveries,
        }
    }

    pub(super) fn press(&mut self, key: Key) -> Disposition {
        self.controller
            .on_key_down(Some(key), Press::First, &mut self.shell)
    }

    /// Presses `key` again, as the OS does while the user holds it.
    pub(super) fn repeat(&mut self, key: Key) -> Disposition {
        self.controller
            .on_key_down(Some(key), Press::Repeat, &mut self.shell)
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
        self.deliver_answers();
    }

    /// Answers the last read of the newest release with `release`, through the sink of the read.
    pub(super) fn answer_release_read(&mut self, release: Result<ReleaseTag, ReleaseReadFailed>) {
        let sink = Arc::clone(
            self.recorder
                .borrow()
                .release_reads
                .last()
                .expect("a release read"),
        );
        sink(release);
        self.deliver_answers();
    }

    /// Answers the last download of a release with `release`, through the sink of the download.
    pub(super) fn answer_release_download(
        &mut self,
        release: Result<DownloadedRelease, ReleaseDownloadFailed>,
    ) {
        let sink = Arc::clone(
            &self
                .recorder
                .borrow()
                .release_downloads
                .last()
                .expect("a release download")
                .1,
        );
        sink(release);
        self.deliver_answers();
    }

    /// Makes each later install of a release fail.
    pub(super) fn fail_installs(&mut self) {
        self.recorder.borrow_mut().install_result = Some(Err(ReleaseInstallFailed));
    }

    /// The tags of the releases that the controller downloaded.
    pub(super) fn release_downloads(&self) -> Vec<ReleaseTag> {
        self.recorder
            .borrow()
            .release_downloads
            .iter()
            .map(|(tag, _)| tag.clone())
            .collect()
    }

    /// The releases that the controller installed.
    pub(super) fn installs(&self) -> Vec<DownloadedRelease> {
        self.recorder.borrow().installs.clone()
    }

    /// Gives the controller each delivery that the services sent since the last call.
    fn deliver_answers(&mut self) {
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

    pub(super) fn release_reads(&self) -> usize {
        self.recorder.borrow().release_reads.len()
    }

    pub(super) fn hub_requests(&self) -> Vec<HubRequest> {
        self.recorder.borrow().hub.clone()
    }

    pub(super) fn dictionary_requests(&self) -> Vec<DictionaryRequest> {
        self.recorder.borrow().dictionaries.clone()
    }

    /// The languages of each spell checker that the dictionaries of the OS and the hub opened.
    pub(super) fn opened_dictionaries(&self) -> Vec<Vec<String>> {
        self.recorder.borrow().opened_dictionaries.clone()
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

    pub(super) fn history_deletes(&self) -> usize {
        self.recorder.borrow().history_deletes
    }

    pub(super) fn settle_input(&mut self) {
        self.controller.on_timer(Timer::InputSettle, &mut self.shell);
    }

    pub(super) fn run_out_idle_timeout(&mut self) {
        self.controller.on_timer(Timer::Idle, &mut self.shell);
    }

    pub(super) fn change_energy_saver(&mut self, energy_saver: EnergySaver) {
        self.controller.on_energy_saver(energy_saver, &mut self.shell);
    }

    pub(super) fn change_game(&mut self, game: GameActivity) {
        self.controller.on_game(game, &mut self.shell);
    }

    pub(super) fn change_full_screen(&mut self, full_screen: FullScreenActivity) {
        self.controller.on_full_screen(full_screen, &mut self.shell);
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

    /// The setup that the shell showed or rendered last since the last take of the calls.
    pub(super) fn last_setup(&mut self) -> Option<SetupView> {
        self.take_calls().into_iter().rev().find_map(|call| match call {
            Call::ShowSetup(view) | Call::RenderSetup(view) => Some(*view),
            _ => None,
        })
    }

    pub(super) fn shown_settings(&mut self) -> SettingsView {
        self.controller
            .on_tray_command(TrayCommand::OpenSettings, &mut self.shell);
        match self.take_calls().pop() {
            Some(Call::ShowSettings(view, _)) => *view,
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

pub(super) fn history_off() -> Settings {
    Settings {
        personalization: PersonalizationSettings {
            history: HistoryAccess::Off,
            ..PersonalizationSettings::default()
        },
        ..Settings::default()
    }
}

pub(super) fn history_on() -> Settings {
    Settings {
        personalization: PersonalizationSettings {
            history: HistoryAccess::On,
            kept_texts: KeptTexts::All,
            apps: vec![mail_history_app(HistoryScope::AllApps)],
            word_choice: WordChoice::Off,
            reply_words: ReplyWords::On,
        },
        ..Settings::default()
    }
}
