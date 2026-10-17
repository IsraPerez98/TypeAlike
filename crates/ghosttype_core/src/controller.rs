//! Runs GhostType from the events of the platform: keys, clicks, timers, the tray menu, the
//! settings window and the answers of the services. The platform reports its events to the
//! [`Controller`] and does what the controller asks through the [`Shell`].

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use crate::app::{App, BadgeView, INPUT_SETTLE_DELAY, TrayCommand, TrayOutcome, TrayView};
use crate::desktop::Desktop;
use crate::engine::Reaction;
use crate::general_pane::{GeneralAction, GeneralPane};
use crate::hub::{HubEvent, HubEventSink, ModelHub};
use crate::inference::{InferenceEvent, InferenceEventSink, InferenceRequest, ModelRunner};
use crate::key::{Disposition, Key};
use crate::models_pane::{ModelsAction, ModelsPane};
use crate::playground::{PlaygroundAction, PlaygroundPane};
use crate::settings::{Autostart, ClipboardAccess, Settings, SettingsStore};
use crate::settings_view::{SettingsInput, SettingsView};

/// The time that a host app takes to move its caret after a key. The ghost text and the badge
/// show after it, so that they show at the new caret.
const CARET_SETTLE_DELAY: Duration = Duration::from_millis(40);

/// A timer that the shell runs for the controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Timer {
    /// Waits until the host app moved its caret, so that the ghost text and the badge show at it.
    CaretSettle,
    /// Waits for [`INPUT_SETTLE_DELAY`] without keys and clicks.
    InputSettle,
}

/// Whether another window or app has the focus since the shell looked last.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusChange {
    /// The same window or app has the focus.
    Unchanged,
    /// Another window or app has the focus.
    Changed,
}

/// Who picks the backend that runs each model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendChoice {
    /// The user, in the General pane.
    User,
    /// The model runner, as its build allows.
    Runner,
}

/// Something that the user did in a pane of the settings window.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PaneAction {
    General(GeneralAction),
    Models(ModelsAction),
    Playground(PlaygroundAction),
}

/// The answer of a service. The platform gives it back to the controller on the thread that runs
/// the controller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delivery {
    /// For the Models pane.
    Hub(HubEvent),
    /// For the Playground pane, which also loads the suggestion model.
    Playground(InferenceEvent),
    /// For the ghost text at the caret.
    Suggestion(InferenceEvent),
}

/// Takes a delivery from any thread to the thread that runs the controller, and gives it to
/// [`Controller::on_delivery`] there.
pub type DeliverySink = Arc<dyn Fn(Delivery) + Send + Sync>;

/// The services that the controller uses.
pub struct Services {
    /// Finds, downloads and deletes models for the Models pane.
    pub hub: Box<dyn ModelHub>,
    /// Loads the suggestion model, and completes the text that the user types.
    pub runner: Box<dyn ModelRunner>,
    /// Keeps the settings that the user changes in the General pane.
    pub settings_store: Box<dyn SettingsStore>,
    /// Starts GhostType at login when the user turns it on in the General pane.
    pub autostart: Box<dyn Autostart>,
}

impl fmt::Debug for Services {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Services").finish_non_exhaustive()
    }
}

/// The platform as the controller drives it: the desktop that it reads, and what it shows and
/// does.
pub trait Shell: Desktop {
    /// Looks at the focused window again, and tells whether another window or app has the focus
    /// since the last look.
    fn refresh_focus(&mut self) -> FocusChange;

    /// Shows the `ghost` text after the caret of the focused field and the `badge` at the caret,
    /// from one look at the caret. Hides the ghost text for no `ghost`, the badge for no `badge`,
    /// and both when the caret is not known.
    fn render_at_caret(&mut self, ghost: Option<&str>, badge: Option<BadgeView>);

    /// Hides the ghost text at once.
    fn hide_ghost(&mut self);

    /// Hides the badge at once.
    fn hide_badge(&mut self);

    /// Types `text` at the caret of the focused app, after the event that the controller handles
    /// now.
    fn type_text(&mut self, text: String);

    /// Starts `timer`, or starts it again when it runs. After `delay`, the shell gives the timer
    /// one time to [`Controller::on_timer`].
    fn start_timer(&mut self, timer: Timer, delay: Duration);

    /// Stops `timer` when it runs.
    fn stop_timer(&mut self, timer: Timer);

    /// Shows `view` in the tray icon and its menu.
    fn render_tray(&mut self, view: &TrayView);

    /// Shows `view` in the settings window, when the window exists.
    fn render_settings(&mut self, view: &SettingsView);

    /// Shows the settings window with `view` and brings it to the front. Makes the window the
    /// first time.
    fn show_settings(&mut self, view: &SettingsView);

    /// Keeps the text that the user copies while `access` allows it, and forgets it when not.
    fn use_clipboard_access(&mut self, access: ClipboardAccess);

    /// Ends GhostType after the event that the controller handles now.
    fn quit(&mut self);
}

/// GhostType while it runs: the suggestions at the caret, the panes of the settings window and
/// the services, driven by the events of the platform.
pub struct Controller {
    app: App,
    models: ModelsPane,
    playground: PlaygroundPane,
    general: GeneralPane,
    hub: Box<dyn ModelHub>,
    runner: Box<dyn ModelRunner>,
    settings_store: Box<dyn SettingsStore>,
    hub_sink: HubEventSink,
    playground_sink: InferenceEventSink,
    suggestion_sink: InferenceEventSink,
    pending_ghost: Option<String>,
    is_caret_timer_pending: bool,
}

impl fmt::Debug for Controller {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Controller")
            .field("app", &self.app)
            .field("has_pending_ghost", &self.pending_ghost.is_some())
            .field("is_caret_timer_pending", &self.is_caret_timer_pending)
            .finish_non_exhaustive()
    }
}

impl Controller {
    /// Starts GhostType with the saved `settings` and the `services`, and starts to load the
    /// suggestion model of `settings`. The services answer through `deliver`.
    #[must_use]
    pub fn new(
        settings: Settings,
        services: Services,
        backends: BackendChoice,
        deliver: &DeliverySink,
    ) -> Self {
        let Services {
            hub,
            mut runner,
            settings_store,
            autostart,
        } = services;
        let mut app = App::new();
        app.use_settings(&settings);
        let (mut playground, general) = match backends {
            BackendChoice::User => (
                PlaygroundPane::with_backend(settings.backend),
                GeneralPane::new(settings),
            ),
            BackendChoice::Runner => (
                PlaygroundPane::new(),
                GeneralPane::new(settings).without_backend_menu(),
            ),
        };
        playground.use_settings(general.settings());
        let general = general.with_autostart(autostart);
        let playground_sink = delivering_sink(deliver, Delivery::Playground);
        if let Some(request) = general.load_suggestion_model(&mut playground) {
            runner.submit(request, &playground_sink);
        }
        Self {
            app,
            models: ModelsPane::new(),
            playground,
            general,
            hub,
            runner,
            settings_store,
            hub_sink: delivering_sink(deliver, Delivery::Hub),
            playground_sink,
            suggestion_sink: delivering_sink(deliver, Delivery::Suggestion),
            pending_ghost: None,
            is_caret_timer_pending: false,
        }
    }

    /// Returns what the tray icon and its menu must show now.
    #[must_use]
    pub fn tray_view(&self) -> TrayView {
        self.app.tray_view()
    }

    /// Handles a key or a click of the user, or another app that comes to the front: GhostType
    /// waits for the input to settle again.
    pub fn on_input(&mut self, shell: &mut dyn Shell) {
        shell.start_timer(Timer::InputSettle, INPUT_SETTLE_DELAY);
    }

    /// Handles a key that the user pressed, before the key reaches the focused app. `key` is
    /// `None` for a press that the platform does not translate, such as a modifier key alone.
    pub fn on_key_down(&mut self, key: Option<Key>, shell: &mut dyn Shell) -> Disposition {
        self.refresh_focus(shell);
        let Some(key) = key else {
            return Disposition::Pass;
        };
        let reaction = self.app.on_key(key, &*shell);
        let disposition = self.apply(reaction, shell);
        self.render_badge_after_caret_settles(shell);
        disposition
    }

    /// Handles a caret that moved somewhere unknown: another window has the focus, the user
    /// clicked, or the screen changed.
    pub fn on_context_lost(&mut self, shell: &mut dyn Shell) {
        let reaction = self.app.on_context_lost();
        self.apply(reaction, shell);
        shell.hide_badge();
    }

    /// Handles a `timer` that ran out.
    pub fn on_timer(&mut self, timer: Timer, shell: &mut dyn Shell) {
        match timer {
            Timer::CaretSettle => self.on_caret_settled(shell),
            Timer::InputSettle => self.on_input_settled(shell),
        }
    }

    /// Handles a click on the badge at the caret.
    pub fn on_badge_click(&mut self, shell: &mut dyn Shell) {
        if let Some(badge) = self.app.badge_view() {
            self.on_tray_command(badge.command, shell);
        }
    }

    /// Runs a command that the user picked in the tray menu.
    pub fn on_tray_command(&mut self, command: TrayCommand, shell: &mut dyn Shell) {
        match self.app.on_tray_command(command) {
            TrayOutcome::Paused => {
                self.hide_ghost(shell);
                let request = self.playground.unload_model();
                self.on_status_change(request, shell);
            }
            TrayOutcome::Resumed => {
                let request = self.general.load_suggestion_model(&mut self.playground);
                self.on_status_change(request, shell);
            }
            TrayOutcome::OpenSettings => self.open_settings(shell),
            TrayOutcome::Quit => shell.quit(),
        }
    }

    /// Handles something that the user did with a control of the settings window, then shows
    /// the panes as they are now. A menu then shows its selected entry again, also when the
    /// picked entry does nothing.
    pub fn on_settings_input(&mut self, input: &SettingsInput, shell: &mut dyn Shell) {
        match self.pane_action(input) {
            Some(PaneAction::General(action)) => self.on_general_action(action, shell),
            Some(PaneAction::Models(action)) => {
                if let Some(request) = self.models.on_action(action) {
                    self.hub.submit(request, &self.hub_sink);
                }
            }
            Some(PaneAction::Playground(action)) => {
                if let Some(request) = self.playground.on_action(action) {
                    self.runner.submit(request, &self.playground_sink);
                }
            }
            None => {}
        }
        self.render_settings(shell);
    }

    /// The action of the pane that owns the control of `input`, if the input does something.
    fn pane_action(&self, input: &SettingsInput) -> Option<PaneAction> {
        let installed = self.models.installed_files();
        self.general
            .action_for(input, &self.playground, &installed)
            .map(PaneAction::General)
            .or_else(|| {
                self.playground
                    .action_for(input, &installed)
                    .map(PaneAction::Playground)
            })
            .or_else(|| self.models.action_for(input).map(PaneAction::Models))
    }

    fn render_settings(&self, shell: &mut dyn Shell) {
        shell.render_settings(&self.settings_view());
    }

    /// Handles the answer of a service.
    pub fn on_delivery(&mut self, delivery: Delivery, shell: &mut dyn Shell) {
        match delivery {
            Delivery::Hub(event) => {
                self.models.on_event(event);
                self.render_settings(shell);
            }
            Delivery::Playground(event) => self.on_playground_event(event, shell),
            Delivery::Suggestion(event) => self.on_suggestion_event(event, shell),
        }
    }

    fn refresh_focus(&mut self, shell: &mut dyn Shell) {
        if shell.refresh_focus() == FocusChange::Changed {
            self.on_context_lost(shell);
        }
    }

    fn on_caret_settled(&mut self, shell: &mut dyn Shell) {
        self.is_caret_timer_pending = false;
        shell.render_at_caret(self.pending_ghost.as_deref(), self.app.badge_view());
    }

    fn on_input_settled(&mut self, shell: &mut dyn Shell) {
        self.refresh_focus(shell);
        if let Some(request) = self.app.on_input_settled(&*shell) {
            self.runner.submit(request, &self.suggestion_sink);
        }
    }

    fn apply(&mut self, reaction: Reaction, shell: &mut dyn Shell) -> Disposition {
        match reaction {
            Reaction::Show(text) => {
                self.show_ghost_after_caret_settles(text, shell);
                Disposition::Pass
            }
            Reaction::Hide => {
                self.hide_ghost(shell);
                Disposition::Pass
            }
            Reaction::Accept { text, rest } => {
                self.hide_ghost(shell);
                shell.type_text(text);
                if let Some(rest) = rest {
                    self.show_ghost_after_caret_settles(rest, shell);
                }
                Disposition::Swallow
            }
            Reaction::Request(request) => {
                self.hide_ghost(shell);
                self.runner.submit(request, &self.suggestion_sink);
                Disposition::Pass
            }
        }
    }

    fn on_suggestion_event(&mut self, event: InferenceEvent, shell: &mut dyn Shell) {
        let reaction = match event {
            InferenceEvent::Partial(text) => self.app.on_partial_completion(&text),
            InferenceEvent::Completed(result) => Some(self.app.on_completion(result)),
            InferenceEvent::Loaded { .. } => None,
        };
        match reaction {
            // A completion does not move the caret, so it must not make the wait for the caret longer.
            Some(Reaction::Show(text)) if self.is_caret_timer_pending => self.pending_ghost = Some(text),
            Some(reaction) => {
                self.apply(reaction, shell);
            }
            None => {}
        }
    }

    fn on_playground_event(&mut self, event: InferenceEvent, shell: &mut dyn Shell) {
        self.playground.on_event(event);
        if let Some(settings) = self.general.keep_suggestion_model(&self.playground) {
            self.settings_store.save(&settings);
        }
        self.render_settings(shell);
    }

    fn on_general_action(&mut self, action: GeneralAction, shell: &mut dyn Shell) {
        let outcome = self.general.on_action(action, &mut self.playground);
        if let Some(settings) = outcome.settings {
            self.settings_store.save(&settings);
            self.app.use_settings(&settings);
            self.playground.use_settings(&settings);
            shell.use_clipboard_access(settings.clipboard_access);
        }
        if let Some(request) = outcome.request {
            self.runner.submit(request, &self.playground_sink);
        }
    }

    fn on_status_change(&mut self, model_request: Option<InferenceRequest>, shell: &mut dyn Shell) {
        if let Some(request) = model_request {
            self.runner.submit(request, &self.playground_sink);
        }
        shell.render_tray(&self.app.tray_view());
        shell.render_at_caret(self.pending_ghost.as_deref(), self.app.badge_view());
        self.render_settings(shell);
    }

    fn open_settings(&mut self, shell: &mut dyn Shell) {
        let request = self.models.on_open();
        self.hub.submit(request, &self.hub_sink);
        shell.show_settings(&self.settings_view());
    }

    fn settings_view(&self) -> SettingsView {
        let installed = self.models.installed_files();
        SettingsView {
            pages: vec![
                self.general.page(&self.playground, &installed),
                self.models.page(),
                self.playground.page(&installed),
            ],
        }
    }

    fn show_ghost_after_caret_settles(&mut self, text: String, shell: &mut dyn Shell) {
        self.pending_ghost = Some(text);
        self.restart_caret_timer(shell);
    }

    fn render_badge_after_caret_settles(&mut self, shell: &mut dyn Shell) {
        if self.app.badge_view().is_some() {
            self.restart_caret_timer(shell);
        } else {
            shell.hide_badge();
        }
    }

    fn restart_caret_timer(&mut self, shell: &mut dyn Shell) {
        shell.start_timer(Timer::CaretSettle, CARET_SETTLE_DELAY);
        self.is_caret_timer_pending = true;
    }

    fn hide_ghost(&mut self, shell: &mut dyn Shell) {
        self.pending_ghost = None;
        shell.stop_timer(Timer::CaretSettle);
        self.is_caret_timer_pending = false;
        shell.hide_ghost();
    }
}

/// Returns a sink that gives each event to `deliver`, wrapped by `wrap`.
fn delivering_sink<Event: 'static>(
    deliver: &DeliverySink,
    wrap: fn(Event) -> Delivery,
) -> Arc<dyn Fn(Event) + Send + Sync> {
    let deliver = Arc::clone(deliver);
    Arc::new(move |event| deliver(wrap(event)))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::mem;
    use std::rc::Rc;
    use std::sync::Mutex;

    use super::*;
    use crate::app::{Field, Status};
    use crate::desktop::{CopiedText, Surroundings};
    use crate::general_pane::{BACKEND, CLIPBOARD, PREPARATION, SUGGESTION_MODEL};
    use crate::hub::{ByteSize, HubRequest, ModelFile, SizedFile};
    use crate::inference::{Backend, Completion, TokenCount};
    use crate::models_pane::SEARCH;
    use crate::playground::MODEL;
    use crate::settings::{PromptPreparation, SuggestionDisplay};
    use crate::settings_view::{ControlId, Page};

    const PAUSE_BADGE: BadgeView = BadgeView {
        status: Status::Active,
        label: "Pause suggestions",
        command: TrayCommand::TogglePause,
    };

    const RESUME_BADGE: BadgeView = BadgeView {
        status: Status::Paused,
        label: "Resume suggestions",
        command: TrayCommand::TogglePause,
    };

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Call {
        RenderAtCaret {
            ghost: Option<String>,
            badge: Option<BadgeView>,
        },
        HideGhost,
        HideBadge,
        TypeText(String),
        StartTimer(Timer, Duration),
        StopTimer(Timer),
        RenderTray(TrayView),
        RenderSettings(Box<SettingsView>),
        ShowSettings(Box<SettingsView>),
        UseClipboardAccess(ClipboardAccess),
        Quit,
    }

    struct FakeShell {
        field: Field,
        focus_change: FocusChange,
        calls: Vec<Call>,
    }

    impl Desktop for FakeShell {
        fn field(&self) -> Field {
            self.field
        }

        fn surroundings(&self) -> Surroundings {
            Surroundings::default()
        }

        fn copied_text(&self) -> Option<CopiedText> {
            None
        }
    }

    impl Shell for FakeShell {
        fn refresh_focus(&mut self) -> FocusChange {
            mem::replace(&mut self.focus_change, FocusChange::Unchanged)
        }

        fn render_at_caret(&mut self, ghost: Option<&str>, badge: Option<BadgeView>) {
            self.calls.push(Call::RenderAtCaret {
                ghost: ghost.map(str::to_owned),
                badge,
            });
        }

        fn hide_ghost(&mut self) {
            self.calls.push(Call::HideGhost);
        }

        fn hide_badge(&mut self) {
            self.calls.push(Call::HideBadge);
        }

        fn type_text(&mut self, text: String) {
            self.calls.push(Call::TypeText(text));
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

        fn quit(&mut self) {
            self.calls.push(Call::Quit);
        }
    }

    #[derive(Default)]
    struct Recorder {
        inference: Vec<(InferenceRequest, InferenceEventSink)>,
        hub: Vec<HubRequest>,
        saved: Vec<Settings>,
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

    struct FakeAutostart;

    impl Autostart for FakeAutostart {
        fn is_enabled(&self) -> bool {
            false
        }

        fn enable(&mut self) {}

        fn disable(&mut self) {}
    }

    struct Harness {
        controller: Controller,
        shell: FakeShell,
        recorder: Rc<RefCell<Recorder>>,
        deliveries: Arc<Mutex<Vec<Delivery>>>,
    }

    impl Harness {
        fn new(settings: Settings) -> Self {
            Self::with_backends(settings, BackendChoice::Runner)
        }

        fn with_backends(settings: Settings, backends: BackendChoice) -> Self {
            let recorder = Rc::new(RefCell::new(Recorder::default()));
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
            };
            Self {
                controller: Controller::new(settings, services, backends, &deliver),
                shell: FakeShell {
                    field: Field::Open,
                    focus_change: FocusChange::Unchanged,
                    calls: Vec::new(),
                },
                recorder,
                deliveries,
            }
        }

        fn press(&mut self, key: Key) -> Disposition {
            self.controller.on_key_down(Some(key), &mut self.shell)
        }

        fn settle_caret(&mut self) {
            self.controller.on_timer(Timer::CaretSettle, &mut self.shell);
        }

        /// Answers the last inference request with `event`, through the sink of the request.
        fn answer(&mut self, event: InferenceEvent) {
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

        fn show(&mut self, suggestion: &str) {
            self.press(Key::Char('B'));
            self.answer(completed(suggestion));
            self.settle_caret();
            self.take_calls();
        }

        fn take_calls(&mut self) -> Vec<Call> {
            mem::take(&mut self.shell.calls)
        }

        fn requests(&self) -> Vec<InferenceRequest> {
            self.recorder
                .borrow()
                .inference
                .iter()
                .map(|(request, _)| request.clone())
                .collect()
        }

        fn last_request(&self) -> Option<InferenceRequest> {
            self.requests().pop()
        }

        fn hub_requests(&self) -> Vec<HubRequest> {
            self.recorder.borrow().hub.clone()
        }

        fn saved(&self) -> Vec<Settings> {
            self.recorder.borrow().saved.clone()
        }

        fn toggle(&mut self, id: &'static str, is_on: bool) {
            let input = SettingsInput::Toggled {
                id: ControlId::new(id),
                is_on,
            };
            self.controller.on_settings_input(&input, &mut self.shell);
        }

        fn pick(&mut self, id: &'static str, index: usize) {
            let input = SettingsInput::Picked {
                id: ControlId::new(id),
                index,
            };
            self.controller.on_settings_input(&input, &mut self.shell);
        }

        fn install(&mut self, file: ModelFile) {
            let installed = HubEvent::Installed(vec![SizedFile {
                file,
                size: ByteSize(1),
            }]);
            self.controller
                .on_delivery(Delivery::Hub(installed), &mut self.shell);
        }

        fn shown_settings(&mut self) -> SettingsView {
            self.controller
                .on_tray_command(TrayCommand::OpenSettings, &mut self.shell);
            match self.take_calls().pop() {
                Some(Call::ShowSettings(view)) => *view,
                other => panic!("the settings window did not show: {other:?}"),
            }
        }
    }

    fn general_page(view: &SettingsView) -> &Page {
        view.pages
            .iter()
            .find(|page| page.title == "General")
            .expect("the settings window shows the General pane")
    }

    fn model_file() -> ModelFile {
        ModelFile {
            repo: "owner/repo".to_owned(),
            name: "model.gguf".to_owned(),
        }
    }

    fn suggestion_settings() -> Settings {
        Settings {
            suggestion_model: Some(model_file()),
            ..Settings::default()
        }
    }

    fn completed(text: &str) -> InferenceEvent {
        InferenceEvent::Completed(Ok(Completion {
            text: text.to_owned(),
            tokens: TokenCount(1),
            duration: Duration::ZERO,
        }))
    }

    fn loaded() -> InferenceEvent {
        InferenceEvent::Loaded {
            file: model_file(),
            result: Ok(()),
        }
    }

    fn ghost_at_caret(ghost: Option<&str>, badge: Option<BadgeView>) -> Call {
        Call::RenderAtCaret {
            ghost: ghost.map(str::to_owned),
            badge,
        }
    }

    fn hidden_ghost_and_badge() -> Vec<Call> {
        vec![
            Call::StopTimer(Timer::CaretSettle),
            Call::HideGhost,
            Call::HideBadge,
        ]
    }

    #[test]
    fn new_controller_loads_saved_suggestion_model() {
        let harness = Harness::new(suggestion_settings());

        assert_eq!(
            harness.requests(),
            vec![InferenceRequest::Load {
                file: model_file(),
                backend: None,
            }]
        );
    }

    #[test]
    fn user_backend_choice_loads_suggestion_model_on_saved_backend() {
        let settings = Settings {
            backend: Backend::Vulkan,
            ..suggestion_settings()
        };

        let harness = Harness::with_backends(settings, BackendChoice::User);

        assert_eq!(
            harness.requests(),
            vec![InferenceRequest::Load {
                file: model_file(),
                backend: Some(Backend::Vulkan),
            }]
        );
    }

    #[test]
    fn user_backend_choice_shows_backend_menu() {
        let mut harness = Harness::with_backends(Settings::default(), BackendChoice::User);

        let view = harness.shown_settings();

        assert!(general_page(&view).row(BACKEND).is_some());
    }

    #[test]
    fn runner_backend_choice_shows_no_backend_menu() {
        let mut harness = Harness::new(Settings::default());

        let view = harness.shown_settings();

        assert_eq!(general_page(&view).row(BACKEND), None);
    }

    #[test]
    fn key_in_open_field_requests_suggestion() {
        let mut harness = Harness::new(Settings::default());

        harness.press(Key::Char('B'));

        assert!(matches!(
            harness.last_request(),
            Some(InferenceRequest::Complete { .. })
        ));
    }

    #[test]
    fn typed_key_passes() {
        let mut harness = Harness::new(Settings::default());

        let disposition = harness.press(Key::Char('B'));

        assert_eq!(disposition, Disposition::Pass);
    }

    #[test]
    fn key_in_open_field_hides_ghost_and_waits_for_caret() {
        let mut harness = Harness::new(Settings::default());

        harness.press(Key::Char('B'));

        assert_eq!(
            harness.take_calls(),
            vec![
                Call::StopTimer(Timer::CaretSettle),
                Call::HideGhost,
                Call::StartTimer(Timer::CaretSettle, CARET_SETTLE_DELAY),
            ]
        );
    }

    #[test]
    fn settled_caret_shows_badge() {
        let mut harness = Harness::new(Settings::default());
        harness.press(Key::Char('B'));
        harness.take_calls();

        harness.settle_caret();

        assert_eq!(
            harness.take_calls(),
            vec![ghost_at_caret(None, Some(PAUSE_BADGE))]
        );
    }

    #[test]
    fn completion_during_caret_wait_keeps_timer() {
        let mut harness = Harness::new(Settings::default());
        harness.press(Key::Char('B'));
        harness.take_calls();

        harness.answer(completed("est"));

        assert_eq!(harness.take_calls(), Vec::new());
    }

    #[test]
    fn completion_shows_when_caret_settles() {
        let mut harness = Harness::new(Settings::default());
        harness.press(Key::Char('B'));
        harness.answer(completed("est"));
        harness.take_calls();

        harness.settle_caret();

        assert_eq!(
            harness.take_calls(),
            vec![ghost_at_caret(Some("est"), Some(PAUSE_BADGE))]
        );
    }

    #[test]
    fn completion_after_caret_settled_waits_for_caret_again() {
        let mut harness = Harness::new(Settings::default());
        harness.press(Key::Char('B'));
        harness.settle_caret();
        harness.take_calls();

        harness.answer(completed("est"));

        assert_eq!(
            harness.take_calls(),
            vec![Call::StartTimer(Timer::CaretSettle, CARET_SETTLE_DELAY)]
        );
    }

    #[test]
    fn partial_completion_waits_for_caret_when_words_show_early() {
        let settings = Settings {
            suggestion_display: SuggestionDisplay::WordByWord,
            ..Settings::default()
        };
        let mut harness = Harness::new(settings);
        harness.press(Key::Char('B'));
        harness.settle_caret();
        harness.take_calls();

        harness.answer(InferenceEvent::Partial("est regards".to_owned()));

        assert_eq!(
            harness.take_calls(),
            vec![Call::StartTimer(Timer::CaretSettle, CARET_SETTLE_DELAY)]
        );
    }

    #[test]
    fn load_answer_for_suggestion_changes_nothing() {
        let mut harness = Harness::new(Settings::default());

        harness
            .controller
            .on_delivery(Delivery::Suggestion(loaded()), &mut harness.shell);

        assert_eq!(harness.take_calls(), Vec::new());
    }

    #[test]
    fn accept_swallows_key() {
        let mut harness = Harness::new(Settings::default());
        harness.show("est regards");

        let disposition = harness.press(Key::Tab);

        assert_eq!(disposition, Disposition::Swallow);
    }

    #[test]
    fn accept_types_text() {
        let mut harness = Harness::new(Settings::default());
        harness.show("est regards");

        harness.press(Key::Tab);

        assert!(harness.take_calls().contains(&Call::TypeText("est".to_owned())));
    }

    #[test]
    fn accept_shows_rest_when_caret_settles() {
        let mut harness = Harness::new(Settings::default());
        harness.show("est regards");
        harness.press(Key::Tab);
        harness.take_calls();

        harness.settle_caret();

        assert_eq!(
            harness.take_calls(),
            vec![ghost_at_caret(Some(" regards"), Some(PAUSE_BADGE))]
        );
    }

    #[test]
    fn untranslated_key_passes() {
        let mut harness = Harness::new(Settings::default());

        let disposition = harness.controller.on_key_down(None, &mut harness.shell);

        assert_eq!(disposition, Disposition::Pass);
    }

    #[test]
    fn untranslated_key_requests_nothing() {
        let mut harness = Harness::new(Settings::default());

        harness.controller.on_key_down(None, &mut harness.shell);

        assert_eq!(harness.requests(), Vec::new());
    }

    #[test]
    fn focus_change_before_key_hides_ghost_and_badge() {
        let mut harness = Harness::new(Settings::default());
        harness.shell.focus_change = FocusChange::Changed;

        harness.controller.on_key_down(None, &mut harness.shell);

        assert_eq!(harness.take_calls(), hidden_ghost_and_badge());
    }

    #[test]
    fn key_in_excluded_field_hides_ghost_and_badge() {
        let mut harness = Harness::new(Settings::default());
        harness.shell.field = Field::Excluded;

        harness.press(Key::Char('B'));

        assert_eq!(harness.take_calls(), hidden_ghost_and_badge());
    }

    #[test]
    fn lost_context_hides_ghost_and_badge() {
        let mut harness = Harness::new(Settings::default());
        harness.show("est");

        harness.controller.on_context_lost(&mut harness.shell);

        assert_eq!(harness.take_calls(), hidden_ghost_and_badge());
    }

    #[test]
    fn input_waits_for_input_to_settle() {
        let mut harness = Harness::new(Settings::default());

        harness.controller.on_input(&mut harness.shell);

        assert_eq!(
            harness.take_calls(),
            vec![Call::StartTimer(Timer::InputSettle, INPUT_SETTLE_DELAY)]
        );
    }

    #[test]
    fn settled_input_prepares_prompt_when_preparing_on_focus() {
        let settings = Settings {
            prompt_preparation: PromptPreparation::OnFocus,
            ..Settings::default()
        };
        let mut harness = Harness::new(settings);

        harness
            .controller
            .on_timer(Timer::InputSettle, &mut harness.shell);

        assert!(matches!(
            harness.last_request(),
            Some(InferenceRequest::Complete {
                max_tokens: TokenCount(0),
                ..
            })
        ));
    }

    #[test]
    fn settled_input_after_focus_change_hides_ghost_and_badge() {
        let mut harness = Harness::new(Settings::default());
        harness.shell.focus_change = FocusChange::Changed;

        harness
            .controller
            .on_timer(Timer::InputSettle, &mut harness.shell);

        assert_eq!(harness.take_calls(), hidden_ghost_and_badge());
    }

    #[test]
    fn badge_click_pauses_suggestions() {
        let mut harness = Harness::new(Settings::default());
        harness.press(Key::Char('B'));

        harness.controller.on_badge_click(&mut harness.shell);

        assert_eq!(harness.controller.tray_view().status, Status::Paused);
    }

    #[test]
    fn click_without_badge_changes_nothing() {
        let mut harness = Harness::new(Settings::default());

        harness.controller.on_badge_click(&mut harness.shell);

        assert_eq!(harness.take_calls(), Vec::new());
    }

    #[test]
    fn pause_hides_ghost() {
        let mut harness = Harness::new(Settings::default());
        harness.show("est");

        harness
            .controller
            .on_tray_command(TrayCommand::TogglePause, &mut harness.shell);

        assert!(harness.take_calls().contains(&Call::HideGhost));
    }

    #[test]
    fn pause_unloads_suggestion_model() {
        let mut harness = Harness::new(suggestion_settings());

        harness
            .controller
            .on_tray_command(TrayCommand::TogglePause, &mut harness.shell);

        assert_eq!(harness.last_request(), Some(InferenceRequest::Unload));
    }

    #[test]
    fn pause_renders_paused_tray() {
        let mut harness = Harness::new(Settings::default());

        harness
            .controller
            .on_tray_command(TrayCommand::TogglePause, &mut harness.shell);

        let paused_tray = harness.controller.tray_view();
        assert!(harness.take_calls().contains(&Call::RenderTray(paused_tray)));
    }

    #[test]
    fn pause_shows_resume_badge_at_caret() {
        let mut harness = Harness::new(Settings::default());
        harness.press(Key::Char('B'));

        harness
            .controller
            .on_tray_command(TrayCommand::TogglePause, &mut harness.shell);

        assert!(
            harness
                .take_calls()
                .contains(&ghost_at_caret(None, Some(RESUME_BADGE)))
        );
    }

    #[test]
    fn resume_loads_suggestion_model() {
        let mut harness = Harness::new(suggestion_settings());
        harness
            .controller
            .on_tray_command(TrayCommand::TogglePause, &mut harness.shell);

        harness
            .controller
            .on_tray_command(TrayCommand::TogglePause, &mut harness.shell);

        assert_eq!(
            harness.last_request(),
            Some(InferenceRequest::Load {
                file: model_file(),
                backend: None,
            })
        );
    }

    #[test]
    fn open_settings_lists_installed_models() {
        let mut harness = Harness::new(Settings::default());

        harness
            .controller
            .on_tray_command(TrayCommand::OpenSettings, &mut harness.shell);

        assert_eq!(harness.hub_requests(), vec![HubRequest::ListInstalled]);
    }

    #[test]
    fn open_settings_shows_settings_window() {
        let mut harness = Harness::new(Settings::default());

        harness
            .controller
            .on_tray_command(TrayCommand::OpenSettings, &mut harness.shell);

        assert!(matches!(harness.take_calls().as_slice(), [Call::ShowSettings(_)]));
    }

    #[test]
    fn quit_command_quits() {
        let mut harness = Harness::new(Settings::default());

        harness
            .controller
            .on_tray_command(TrayCommand::Quit, &mut harness.shell);

        assert_eq!(harness.take_calls(), vec![Call::Quit]);
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

        harness
            .controller
            .on_timer(Timer::InputSettle, &mut harness.shell);

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
        let mut harness = Harness::with_backends(suggestion_settings(), BackendChoice::User);
        harness.answer(loaded());

        harness.pick(BACKEND, 1);

        assert_eq!(
            harness.last_request(),
            Some(InferenceRequest::Load {
                file: model_file(),
                backend: Some(Backend::Vulkan),
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
                backend: None,
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
        let mut harness = Harness::new(Settings::default());
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
}
