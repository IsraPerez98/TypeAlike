//! Runs TypeAlike from the events of the platform: keys, clicks, timers, the tray menu, the
//! settings window and the answers of the services. The platform reports its events to the
//! [`Controller`] and does what the controller asks through the [`Shell`].

mod caret_wait;
mod ghost_view;
mod services;
mod shell;
mod tab_hold;

pub use ghost_view::{Ghost, GhostPlacement, GhostView};
pub use services::{BackendChoice, Delivery, DeliverySink, PlaygroundPage, Services, Startup};
pub use shell::{CaretLook, CaretRead, FocusChange, Shell, Timer};

use std::time::Duration;
use std::{fmt, mem};

use caret_wait::{CaretWait, GhostTiming, LookOutcome};
use services::delivering_sink;
use tab_hold::TabHold;

use crate::app::{
    App, BadgeCommand, BadgeMenu, BadgeSettings, INPUT_SETTLE_DELAY, SettledWork, TrayCommand, TrayOutcome,
    TrayView,
};
use crate::desktop::{
    CopiedText, Desktop, EnergySaver, Field, GameActivity, OcrAvailability, OcrLevels, Permission,
    PermissionState, ScreenRect, Surroundings, Timestamp,
};
use crate::engine::{AfterAccept, Reaction};
use crate::history::WritingHistory;
use crate::hub::ModelFile;
use crate::hub::{HubEvent, HubEventSink, ModelHub};
use crate::inference::{Backend, InferenceEvent, InferenceEventSink, InferenceRequest, ModelRunner};
use crate::key::{Disposition, Key};
use crate::localization::Translations;
use crate::settings::{HistoryAccess, HistoryApp, Settings, SettingsStore, Setup, SuggestionApp};
use crate::settings_window::energy_saving::{EnergySavingAction, EnergySavingPane};
use crate::settings_window::general::{GeneralAction, GeneralPane, ResetPress, presses_reset};
use crate::settings_window::models::{ModelsAction, ModelsPane};
use crate::settings_window::permissions;
use crate::settings_window::personalization::{
    PersonalizationAction, PersonalizationChange, PersonalizationPane,
};
use crate::settings_window::playground::{PlaygroundAction, PlaygroundPane};
use crate::settings_window::screen_text::{ScreenTextAction, ScreenTextPane};
use crate::settings_window::suggestion_places::{SuggestionPlacesAction, SuggestionPlacesPane};
use crate::settings_window::view::{PageIcon, Row, SettingsInput, SettingsView};
use crate::setup::{SetupAction, SetupContext, SetupFlow, SetupOutcome, SetupView, TrayPlace};

/// The time that a host app takes to move its caret after a key. The ghost text and the badge
/// show after it, so that they show at the new caret.
const CARET_SETTLE_DELAY: Duration = Duration::from_millis(40);
/// The time between two looks at a caret that did not move yet after a key. A terminal moves its
/// caret only after the program in it writes the typed text back.
const CARET_MOVE_CHECK_DELAY: Duration = Duration::from_millis(20);
/// The most looks at the caret after a key, before the ghost text shows at the caret where it is.
const MAX_CARET_MOVE_CHECKS: u8 = 20;
/// The room that the ghost text after the caret keeps free before the right edge of its field.
const FIELD_EDGE_GAP: i32 = 4;

/// Something that the user did in a pane of the settings window.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PaneAction {
    General(GeneralAction),
    SuggestionPlaces(SuggestionPlacesAction),
    ScreenText(ScreenTextAction),
    Personalization(PersonalizationAction),
    EnergySaving(EnergySavingAction),
    Models(ModelsAction),
    Playground(PlaygroundAction),
    Permissions(Permission),
    Setup(SetupAction),
    ResetSettings,
}

/// TypeAlike while it runs: the suggestions at the caret, the panes of the settings window and
/// the services, driven by the events of the platform.
pub struct Controller {
    app: App,
    translations: Translations,
    models: ModelsPane,
    playground: PlaygroundPane,
    playground_page: PlaygroundPage,
    general: GeneralPane,
    suggestion_places: SuggestionPlacesPane,
    screen_text: ScreenTextPane,
    personalization: PersonalizationPane,
    energy_saving: EnergySavingPane,
    hub: Box<dyn ModelHub>,
    runner: Box<dyn ModelRunner>,
    settings_store: Box<dyn SettingsStore>,
    history: Box<dyn WritingHistory>,
    /// Whether the app has the entries of `history`, which it reads only while the history is on.
    is_history_loaded: bool,
    hub_sink: HubEventSink,
    playground_sink: InferenceEventSink,
    suggestion_sink: InferenceEventSink,
    recommendation_sink: InferenceEventSink,
    caret_wait: CaretWait,
    tab_hold: TabHold,
    /// Whether the Apps and websites, Screen text or Personalization pane has an app or a website
    /// that the settings store does not have yet.
    has_unsaved_app: bool,
    /// The state of the setup window while it shows.
    setup: Option<SetupFlow>,
    tray: TrayPlace,
}

impl fmt::Debug for Controller {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Controller")
            .field("app", &self.app)
            .field("caret_wait", &self.caret_wait)
            .field("tab_hold", &self.tab_hold)
            .finish_non_exhaustive()
    }
}

impl Controller {
    /// Starts TypeAlike with the saved `settings` and the `services`, and starts to load the
    /// suggestion model of `settings`, unless the energy saver of `startup` stops suggestions. The
    /// services answer through `deliver`. When the backend choice of `startup` changes the saved
    /// backend, it saves the settings with the new backend.
    #[must_use]
    pub fn new(settings: Settings, services: Services, startup: Startup, deliver: &DeliverySink) -> Self {
        let Services {
            hub,
            runner,
            mut settings_store,
            autostart,
            history,
            spell_checker,
        } = services;
        let mut app = App::new();
        app.use_settings(&settings);
        app.use_spell_checker(spell_checker);
        app.use_energy_saver(startup.energy_saver);
        let energy_saving = EnergySavingPane::new(settings.energy_saving);
        let suggestion_places = SuggestionPlacesPane::new(settings.suggestion_places.clone());
        let screen_text = ScreenTextPane::new(settings.screen_text.clone());
        let personalization = PersonalizationPane::new(settings.personalization.clone());
        let saved_backend = settings.backend;
        let general = match startup.backend_choice {
            BackendChoice::User => GeneralPane::new(settings),
            BackendChoice::Platform(backend) => GeneralPane::new(settings).with_platform_backend(backend),
        };
        if general.settings().backend != saved_backend {
            settings_store.save(general.settings());
        }
        let mut playground = PlaygroundPane::with_backend(general.settings().backend);
        playground.use_settings(general.settings());
        let general = general
            .with_autostart(autostart)
            .with_system_language(startup.ui_language);
        let mut controller = Self {
            app,
            translations: Translations::new(general.language()),
            models: ModelsPane::new(),
            playground,
            playground_page: startup.playground,
            general,
            suggestion_places,
            screen_text,
            personalization,
            energy_saving,
            hub,
            runner,
            settings_store,
            history,
            is_history_loaded: false,
            hub_sink: delivering_sink(deliver, Delivery::Hub),
            playground_sink: delivering_sink(deliver, Delivery::Playground),
            suggestion_sink: delivering_sink(deliver, Delivery::Suggestion),
            recommendation_sink: delivering_sink(deliver, Delivery::Recommendation),
            caret_wait: CaretWait::default(),
            tab_hold: TabHold::default(),
            has_unsaved_app: false,
            setup: None,
            tray: startup.tray,
        };
        controller.submit_model_request();
        controller.read_gpu_memory_for_recommendation();
        controller.load_history_when_on();
        controller
    }

    /// Returns what the tray icon and its menu must show now.
    #[must_use]
    pub fn tray_view(&self) -> TrayView {
        self.app.tray_view(&self.translations)
    }

    /// Shows the setup when TypeAlike starts, until the user finishes it. The platform calls this
    /// one time, after it can show windows.
    pub fn on_launch(&mut self, shell: &mut dyn Shell) {
        if self.needs_setup() {
            self.open_setup(shell);
        }
    }

    /// Handles a close of the setup window by the user. The setup starts again at its first step
    /// the next time that it shows.
    pub fn on_setup_closed(&mut self) {
        self.setup = None;
    }

    /// Handles a key or a click of the user, or another app that comes to the front: TypeAlike
    /// waits for the input to settle again, and the idle timeout of the Energy saving pane starts
    /// again.
    pub fn on_input(&mut self, shell: &mut dyn Shell) {
        shell.start_timer(Timer::InputSettle, INPUT_SETTLE_DELAY);
        if let Some(timeout) = self.energy_saving.settings().idle_timeout.duration() {
            shell.start_timer(Timer::Idle, timeout);
        }
    }

    /// Handles a key that the user pressed, before the key reaches the focused app. `key` is
    /// `None` for a press that the platform does not translate, such as a modifier key alone. The
    /// first key of a session also reads the screen, so that the prompt gets the newest messages.
    /// After the last part of a suggestion, a quick Tab does not reach the app while the next
    /// suggestion can still come and does not show yet.
    pub fn on_key_down(&mut self, key: Option<Key>, shell: &mut dyn Shell) -> Disposition {
        self.refresh_focus(shell);
        let Some(key) = key else {
            return Disposition::Pass;
        };
        if self.tab_hold.holds(key, shell) {
            return Disposition::Swallow;
        }
        self.caret_wait.on_key();
        let reaction = if self.caret_wait.is_rest() {
            self.app.on_key(key, &DesktopWithPendingRest(&*shell))
        } else {
            self.app.on_key(key, &*shell)
        };
        let disposition = self.apply(reaction, shell);
        self.render_badge_after_caret_settles(shell);
        let read = self.app.start_session_screen_read();
        self.do_settled_work(read, shell);
        disposition
    }

    /// Handles a caret that moved somewhere unknown: another window has the focus, the user
    /// clicked, or the screen changed.
    pub fn on_context_lost(&mut self, shell: &mut dyn Shell) {
        self.tab_hold.end(shell);
        let reaction = self.app.on_context_lost();
        self.apply(reaction, shell);
        shell.hide_badge();
    }

    /// Handles a scroll of the mouse wheel or the touchpad, which moves the caret on the screen but
    /// does not change the text. While the badge shows or waits for the caret, the ghost text and
    /// the badge hide, and show again with the same suggestion at the caret when the scroll stops.
    /// A scroll does not end the session and does not count as input, so it reads no screen.
    pub fn on_scroll(&mut self, shell: &mut dyn Shell) {
        if !self.app.has_badge() {
            return;
        }
        shell.hide_ghost();
        shell.hide_badge();
        self.caret_wait.on_scroll(shell);
    }

    /// Handles a `timer` that ran out.
    pub fn on_timer(&mut self, timer: Timer, shell: &mut dyn Shell) {
        match timer {
            Timer::CaretSettle => self.on_caret_settled(shell),
            Timer::InputSettle => self.on_input_settled(shell),
            Timer::TabHold => self.tab_hold.on_timer(),
            Timer::Idle => self.on_idle(shell),
        }
    }

    /// Handles a change of the energy saver of the computer. While the energy saver is on and the
    /// Energy saving pane stops suggestions then, the session ends, the suggestion model unloads,
    /// and the tray and the badge show the stop. When it goes off, the model loads again, unless
    /// the user paused suggestions or the idle timeout ran out.
    pub fn on_energy_saver(&mut self, energy_saver: EnergySaver, shell: &mut dyn Shell) {
        self.app.use_energy_saver(energy_saver);
        self.on_stop_change(shell);
    }

    /// Handles a change of the app in front: whether it is a game. While a game is in front and the
    /// Energy saving pane stops suggestions then, the session ends, the suggestion model unloads,
    /// and the tray and the badge show the stop. After the game, the next request for a suggestion
    /// loads the model again. The same state again changes nothing.
    pub fn on_game(&mut self, game: GameActivity, shell: &mut dyn Shell) {
        if self.app.game() == game {
            return;
        }
        self.app.use_game(game);
        self.on_stop_change(shell);
    }

    /// Handles the end of the idle timeout: the session ends and the suggestion model unloads. The
    /// next request for a suggestion loads it again.
    fn on_idle(&mut self, shell: &mut dyn Shell) {
        self.tab_hold.end(shell);
        let reaction = self.app.on_idle();
        self.apply(reaction, shell);
        shell.hide_badge();
        self.on_stop_change(shell);
    }

    /// Handles `read`, the look at the caret that the shell gives after it answered
    /// [`CaretLook::Later`]. Shows the waiting ghost text and the badge at the caret of `read`.
    /// After a key, a caret that did not move yet gets only the badge, and the controller looks
    /// again later. A caret far from the frame of its field counts as no caret. Drops `read` when
    /// a key came after the look started, because the key can move the caret.
    pub fn on_caret_read(&mut self, read: CaretRead, shell: &mut dyn Shell) {
        let Some(outcome) = self.caret_wait.on_caret_read(read) else {
            return;
        };
        match outcome {
            LookOutcome::GhostAndBadge => self.render_at_caret(read, shell),
            LookOutcome::BadgeOnly => shell.render_at_caret(None, self.app.badge_view(&self.translations)),
            LookOutcome::Nothing => {
                shell.hide_ghost();
                shell.hide_badge();
            }
        }
        self.caret_wait.after_render(shell);
    }

    /// Returns the menu that a click on the badge at the caret shows, or `None` while no badge
    /// shows.
    #[must_use]
    pub fn badge_menu(&self) -> Option<BadgeMenu> {
        let settings = BadgeSettings {
            suggestion_places: self.suggestion_places.settings(),
            screen_text: self.screen_text.settings(),
            personalization: self.personalization.settings(),
        };
        self.app.badge_menu(&settings, &self.translations)
    }

    /// Runs a command that the user picked in the menu of the badge at the caret, then shows the
    /// panes as they are now.
    pub fn on_badge_command(&mut self, command: BadgeCommand, shell: &mut dyn Shell) {
        match command {
            BadgeCommand::TogglePause => self.on_tray_command(TrayCommand::TogglePause, shell),
            BadgeCommand::SwitchScreenText { app_id, is_on } => {
                self.on_screen_text_action(ScreenTextAction::SwitchApp { app_id, is_on });
                self.render_settings(shell);
            }
            BadgeCommand::SwitchAppSuggestions { app_id, is_on } => {
                self.switch_suggestions_here(SuggestionPlacesAction::SwitchApp { app_id, is_on }, shell);
            }
            BadgeCommand::SwitchSiteSuggestions { host, is_on } => {
                self.switch_suggestions_here(SuggestionPlacesAction::SwitchSite { host, is_on }, shell);
            }
            BadgeCommand::SelectHistoryScope { app_id, scope } => {
                self.on_personalization_action(PersonalizationAction::SelectAppScope { app_id, scope });
                self.render_settings(shell);
            }
        }
    }

    /// Turns suggestions on or off in the place where the user types, then hides the ghost text
    /// and shows the badge and the panes as they are now.
    fn switch_suggestions_here(&mut self, action: SuggestionPlacesAction, shell: &mut dyn Shell) {
        self.on_suggestion_places_action(action);
        self.tab_hold.end(shell);
        self.hide_ghost(shell);
        self.render_waiting_ghost(shell);
        self.render_settings(shell);
    }

    /// Runs a command that the user picked in the tray menu.
    pub fn on_tray_command(&mut self, command: TrayCommand, shell: &mut dyn Shell) {
        match self.app.on_tray_command(command) {
            TrayOutcome::Paused | TrayOutcome::Resumed => self.on_stop_change(shell),
            TrayOutcome::OpenSettings if self.needs_setup() => self.open_setup(shell),
            TrayOutcome::OpenSettings => self.open_settings(None, shell),
            TrayOutcome::Quit => {
                self.save_history();
                shell.quit();
            }
        }
    }

    /// Opens the settings window at the Permissions pane, because TypeAlike lacks a permission that
    /// it needs to show suggestions. Until the user finishes the setup, it opens the setup, which
    /// asks for the permission.
    pub fn on_missing_permission(&mut self, shell: &mut dyn Shell) {
        if self.needs_setup() {
            self.open_setup(shell);
        } else {
            self.open_settings(Some(PageIcon::Hand), shell);
        }
    }

    /// Shows the new state of the permissions in the settings window, after the user gave or took
    /// back a permission.
    pub fn on_permission_change(&mut self, shell: &mut dyn Shell) {
        self.render_settings(shell);
    }

    /// Handles something that the user did with a control of the settings window or of the setup
    /// window, then shows the panes and the setup as they are now. A menu then shows its selected
    /// entry again, also when the picked entry does nothing.
    pub fn on_settings_input(&mut self, input: &SettingsInput, shell: &mut dyn Shell) {
        let action = self.pane_action(input);
        if !matches!(action, Some(PaneAction::Personalization(_))) {
            self.personalization.cancel_delete();
        }
        if action != Some(PaneAction::ResetSettings) {
            self.general.cancel_reset();
        }
        match action {
            Some(PaneAction::Setup(action)) => self.on_setup_action(action, shell),
            Some(PaneAction::ResetSettings) => {
                if self.general.press_reset() == ResetPress::Confirmed {
                    self.reset_settings(shell);
                }
            }
            Some(PaneAction::General(action)) => self.on_general_action(action, shell),
            Some(PaneAction::SuggestionPlaces(action)) => self.on_suggestion_places_action(action),
            Some(PaneAction::ScreenText(ScreenTextAction::AskForOcrPermission)) => {
                shell.ask_for_permission(Permission::ScreenRecording);
            }
            Some(PaneAction::ScreenText(action)) => self.on_screen_text_action(action),
            Some(PaneAction::Personalization(action)) => self.on_personalization_action(action),
            Some(PaneAction::EnergySaving(action)) => self.on_energy_saving_action(action, shell),
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
            Some(PaneAction::Permissions(permission)) => shell.ask_for_permission(permission),
            None => {}
        }
        self.render_settings(shell);
    }

    /// The action of the pane that owns the control of `input`, if the input does something.
    fn pane_action(&self, input: &SettingsInput) -> Option<PaneAction> {
        let installed = self.models.installed_files();
        if let Some(action) = SetupFlow::action_for(input) {
            return Some(PaneAction::Setup(action));
        }
        if presses_reset(input) {
            return Some(PaneAction::ResetSettings);
        }
        self.general
            .action_for(input, &self.playground, &installed)
            .map(PaneAction::General)
            .or_else(|| {
                self.suggestion_places
                    .action_for(input)
                    .map(PaneAction::SuggestionPlaces)
            })
            .or_else(|| self.screen_text.action_for(input).map(PaneAction::ScreenText))
            .or_else(|| {
                self.personalization
                    .action_for(input)
                    .map(PaneAction::Personalization)
            })
            .or_else(|| self.energy_saving.action_for(input).map(PaneAction::EnergySaving))
            .or_else(|| {
                self.playground
                    .action_for(input, &installed)
                    .map(PaneAction::Playground)
            })
            .or_else(|| {
                self.models
                    .action_for(input, self.general.settings().backend, &self.translations)
                    .map(PaneAction::Models)
            })
            .or_else(|| permissions::action_for(input).map(PaneAction::Permissions))
    }

    /// Shows the panes in the settings window, and the setup in its window while it shows.
    fn render_settings(&self, shell: &mut dyn Shell) {
        shell.render_settings(&self.settings_view(shell));
        if let Some(view) = self.setup_view(shell) {
            shell.render_setup(&view);
        }
    }

    fn needs_setup(&self) -> bool {
        self.general.settings().setup == Setup::Needed
    }

    /// Shows the setup window at the step that it shows, or at its first step when it is closed.
    fn open_setup(&mut self, shell: &mut dyn Shell) {
        let request = self.models.on_open();
        self.hub.submit(request, &self.hub_sink);
        self.setup.get_or_insert_with(SetupFlow::new);
        if let Some(view) = self.setup_view(shell) {
            shell.show_setup(&view);
        }
    }

    fn on_setup_action(&mut self, action: SetupAction, shell: &mut dyn Shell) {
        let Some(mut flow) = self.setup.take() else {
            return;
        };
        let installed = self.models.installed_files();
        let permissions = shell.permissions();
        let outcome = flow.on_action(action, &self.setup_context(&installed, &permissions));
        match outcome {
            SetupOutcome::Shown => self.setup = Some(flow),
            SetupOutcome::Finished(autostart) => {
                shell.close_setup();
                self.on_general_action(autostart, shell);
                self.on_general_action(GeneralAction::FinishSetup, shell);
            }
        }
    }

    fn setup_view(&self, desktop: &dyn Desktop) -> Option<SetupView> {
        let flow = self.setup.as_ref()?;
        let installed = self.models.installed_files();
        let permissions = desktop.permissions();
        Some(flow.view(&self.setup_context(&installed, &permissions)))
    }

    fn setup_context<'context>(
        &'context self,
        installed: &'context [ModelFile],
        permissions: &'context [(Permission, PermissionState)],
    ) -> SetupContext<'context> {
        SetupContext {
            general: &self.general,
            models: &self.models,
            playground: &self.playground,
            installed,
            personalization: self.personalization.settings(),
            screen_text: self.screen_text.settings(),
            permissions,
            tray: self.tray,
            translations: &self.translations,
        }
    }

    /// Gives each pane the settings of the first start, except the suggestion model, and opens the
    /// setup at its first step. The downloaded models and the earlier writing stay.
    fn reset_settings(&mut self, shell: &mut dyn Shell) {
        let first_start = self.settings_store.first_start_settings();
        let outcome = self.general.reset(first_start.clone(), &mut self.playground);
        self.use_language(shell);
        self.suggestion_places = SuggestionPlacesPane::new(first_start.suggestion_places);
        self.screen_text = ScreenTextPane::new(first_start.screen_text);
        self.personalization = PersonalizationPane::new(first_start.personalization);
        self.energy_saving = EnergySavingPane::new(first_start.energy_saving);
        let settings = self.save_settings(self.general.settings().clone());
        self.app.use_settings(&settings);
        self.playground.use_settings(&settings);
        shell.use_clipboard_access(settings.clipboard_access);
        if let Some(request) = outcome.request {
            self.runner.submit(request, &self.playground_sink);
        }
        self.load_history_when_on();
        self.read_gpu_memory_for_recommendation();
        self.restart_idle_timer(shell);
        self.setup = None;
        self.on_stop_change(shell);
        self.open_setup(shell);
    }

    /// Handles the answer of a service.
    pub fn on_delivery(&mut self, delivery: Delivery, shell: &mut dyn Shell) {
        match delivery {
            Delivery::Hub(event) => self.on_hub_event(event, shell),
            Delivery::Playground(event) => self.on_playground_event(event, shell),
            Delivery::Suggestion(event) => self.on_suggestion_event(event, shell),
            Delivery::Recommendation(event) => self.on_recommendation_event(&event, shell),
            Delivery::Screen(read) => {
                let work = self.app.on_screen_read(read, &*shell);
                self.do_settled_work(work, shell);
            }
        }
    }

    fn refresh_focus(&mut self, shell: &mut dyn Shell) {
        if shell.refresh_focus() == FocusChange::Changed {
            self.on_context_lost(shell);
        }
    }

    fn on_caret_settled(&mut self, shell: &mut dyn Shell) {
        self.caret_wait.on_timer();
        self.render_waiting_ghost(shell);
    }

    /// Shows the ghost text that waits for the caret, and the badge, at the caret: at once when the
    /// shell reads the caret at once, else when the shell gives its look at the caret.
    fn render_waiting_ghost(&mut self, shell: &mut dyn Shell) {
        self.caret_wait.wait_for_look();
        if let CaretLook::Now(read) = shell.read_caret() {
            self.on_caret_read(read, shell);
        }
    }

    /// Shows `ghost` at once when the caret settled after the last key, else when it settles.
    fn show_at_settled_caret(&mut self, ghost: Ghost, shell: &mut dyn Shell) {
        if self.caret_wait.show_at_settled_caret(ghost, shell) == GhostTiming::Now {
            self.render_waiting_ghost(shell);
        }
    }

    fn on_input_settled(&mut self, shell: &mut dyn Shell) {
        self.refresh_focus(shell);
        if mem::take(&mut self.has_unsaved_app) {
            self.keep_pane_settings();
            self.render_settings(shell);
        }
        self.save_history();
        let work = self.app.on_input_settled(&*shell);
        self.do_settled_work(work, shell);
    }

    fn do_settled_work(&mut self, work: Option<SettledWork>, shell: &mut dyn Shell) {
        match work {
            Some(SettledWork::ReadScreen { read, source }) => shell.read_screen(read, source),
            Some(SettledWork::Prepare(request)) => self.request_suggestion(request, shell),
            Some(SettledWork::Suggest(reaction)) => {
                self.apply(reaction, shell);
            }
            None => {}
        }
    }

    /// Adds the app where a suggestion shows to the Apps and websites, Screen text and
    /// Personalization panes, and its website to the Apps and websites pane, when they do not have
    /// them. A window without a text field, such as a game, shows no suggestion, so the lists get
    /// no such app. The settings are saved when the input settles, because a key must not wait for
    /// the disk.
    fn add_suggested_place(&mut self) {
        if let Some(app) = self.app.typed_app() {
            self.has_unsaved_app |= self.suggestion_places.add_app(&SuggestionApp::of(app));
            self.has_unsaved_app |= self.screen_text.add_app(app);
            self.has_unsaved_app |= self.personalization.add_app(&HistoryApp::of(app));
        }
        if let Some(host) = self.app.typed_host() {
            self.has_unsaved_app |= self.suggestion_places.add_site(host);
        }
    }

    /// Keeps the settings of the Apps and websites, Copied and screen text, Personalization and Energy saving
    /// panes for the next run, and uses them.
    fn keep_pane_settings(&mut self) {
        let settings = self.save_settings(self.general.settings().clone());
        self.app.use_settings(&settings);
    }

    /// Saves `general`, the settings of the General pane, with the settings of the Apps and
    /// websites, Copied and screen text, Personalization and Energy saving panes, and returns them all.
    fn save_settings(&mut self, general: Settings) -> Settings {
        let settings = Settings {
            suggestion_places: self.suggestion_places.settings().clone(),
            screen_text: self.screen_text.settings().clone(),
            personalization: self.personalization.settings().clone(),
            energy_saving: *self.energy_saving.settings(),
            ..general
        };
        self.settings_store.save(&settings);
        settings
    }

    fn on_suggestion_places_action(&mut self, action: SuggestionPlacesAction) {
        if self.suggestion_places.on_action(action) {
            self.keep_pane_settings();
        }
    }

    fn on_screen_text_action(&mut self, action: ScreenTextAction) {
        if self.screen_text.on_action(action) {
            self.keep_pane_settings();
        }
    }

    fn on_personalization_action(&mut self, action: PersonalizationAction) {
        match self.personalization.on_action(action) {
            PersonalizationChange::None => {}
            PersonalizationChange::Settings => {
                self.keep_pane_settings();
                self.load_history_when_on();
            }
            PersonalizationChange::AppOff(app_id) => {
                self.keep_pane_settings();
                self.load_history();
                self.app.forget_history_of(&app_id);
                self.save_history();
            }
            PersonalizationChange::DeleteAll => {
                self.app.use_history(Vec::new());
                self.history.delete();
            }
        }
    }

    /// Uses the change of the Energy saving pane: the idle timeout starts again with its new time,
    /// and the suggestion model loads or unloads for the new stop reason.
    fn on_energy_saving_action(&mut self, action: EnergySavingAction, shell: &mut dyn Shell) {
        if !self.energy_saving.on_action(action) {
            return;
        }
        self.keep_pane_settings();
        self.restart_idle_timer(shell);
        self.on_stop_change(shell);
    }

    /// Starts the idle timeout again with the time of the Energy saving pane, or stops it when the
    /// pane has no timeout.
    fn restart_idle_timer(&mut self, shell: &mut dyn Shell) {
        if let Some(timeout) = self.energy_saving.settings().idle_timeout.duration() {
            shell.start_timer(Timer::Idle, timeout);
        } else {
            shell.stop_timer(Timer::Idle);
            self.app.end_idle();
        }
    }

    /// Gives the app the entries of the history store the first time that the history is on.
    fn load_history_when_on(&mut self) {
        if self.personalization.settings().history == HistoryAccess::On {
            self.load_history();
        }
    }

    /// Gives the app the entries of the history store, when it does not have them yet.
    fn load_history(&mut self) {
        if !self.is_history_loaded {
            self.app.use_history(self.history.load());
            self.is_history_loaded = true;
        }
    }

    /// Keeps the earlier writing of the user when it changed.
    fn save_history(&mut self) {
        if let Some(entries) = self.app.history_to_save() {
            self.history.save(entries);
        }
    }

    fn apply(&mut self, reaction: Reaction, shell: &mut dyn Shell) -> Disposition {
        match reaction {
            Reaction::Show(text) => {
                self.show_at_settled_caret(Ghost::Text(text), shell);
                Disposition::Pass
            }
            Reaction::ShowFix(fix) => {
                self.show_at_settled_caret(Ghost::Fix(fix), shell);
                Disposition::Pass
            }
            Reaction::Hide => {
                self.hide_ghost(shell);
                Disposition::Pass
            }
            Reaction::Accept { text, then } => {
                self.hide_ghost(shell);
                shell.replace_before_caret(0, text);
                self.apply_after_accept(then, shell);
                Disposition::Swallow
            }
            Reaction::Replace { deleted, text, then } => {
                self.hide_ghost(shell);
                self.caret_wait.on_replace(deleted, &text);
                shell.replace_before_caret(deleted, text);
                self.apply_after_accept(then, shell);
                Disposition::Swallow
            }
            Reaction::Swallow => {
                self.hide_ghost(shell);
                Disposition::Swallow
            }
            Reaction::Request(request) => {
                self.hide_ghost(shell);
                self.request_suggestion(request, shell);
                Disposition::Pass
            }
        }
    }

    fn apply_after_accept(&mut self, then: AfterAccept, shell: &mut dyn Shell) {
        match then {
            AfterAccept::Rest(rest) => {
                if !self.caret_wait.show_rest_after_settle(rest, shell) {
                    self.tab_hold.start(shell);
                }
            }
            AfterAccept::Next(reaction) => {
                self.apply(*reaction, shell);
                if self.app.has_offer() {
                    self.tab_hold.start(shell);
                }
            }
        }
    }

    fn on_suggestion_event(&mut self, event: InferenceEvent, shell: &mut dyn Shell) {
        let reaction = match event {
            InferenceEvent::Partial(text) => self.app.on_partial_completion(&text),
            InferenceEvent::Completed(result) => Some(self.app.on_completion(result)),
            InferenceEvent::Loaded { .. } | InferenceEvent::GpuMemory { .. } => None,
        };
        match reaction {
            // A completion does not move the caret, so it must not make the wait for the caret longer.
            Some(Reaction::Show(text)) if self.caret_wait.is_timer_pending() => {
                self.caret_wait.replace_ghost(Ghost::Text(text));
            }
            Some(Reaction::ShowFix(fix)) if self.caret_wait.is_timer_pending() => {
                self.caret_wait.replace_ghost(Ghost::Fix(fix));
            }
            Some(reaction) => {
                self.apply(reaction, shell);
            }
            None => {}
        }
        if !self.app.has_offer() {
            self.tab_hold.end(shell);
        }
    }

    /// Gives `event` to the Models pane. When the first model download finishes and no suggestion
    /// model is selected, the model becomes the suggestion model.
    fn on_hub_event(&mut self, event: HubEvent, shell: &mut dyn Shell) {
        let finished = match &event {
            HubEvent::DownloadFinished { file, result: Ok(_) } => Some(file.clone()),
            _ => None,
        };
        let is_first_model =
            self.models.installed_files().is_empty() && self.general.settings().suggestion_model.is_none();
        self.models.on_event(event);
        let first_model =
            finished.filter(|file| is_first_model && self.models.installed_files().contains(file));
        if let Some(file) = first_model {
            self.on_general_action(GeneralAction::SelectSuggestionModel(file), shell);
        }
        self.render_settings(shell);
    }

    fn on_playground_event(&mut self, event: InferenceEvent, shell: &mut dyn Shell) {
        self.playground.on_event(event);
        if let Some(settings) = self.general.keep_suggestion_model(&self.playground) {
            self.save_settings(settings);
        }
        self.render_settings(shell);
    }

    fn on_recommendation_event(&mut self, event: &InferenceEvent, shell: &mut dyn Shell) {
        if let InferenceEvent::GpuMemory { backend, result } = event {
            self.models.on_gpu_memory(*backend, *result);
            self.render_settings(shell);
        }
    }

    fn on_general_action(&mut self, action: GeneralAction, shell: &mut dyn Shell) {
        let backend = self.general.settings().backend;
        let outcome = self.general.on_action(action, &mut self.playground);
        if let Some(settings) = outcome.settings {
            let settings = self.save_settings(settings);
            self.app.use_settings(&settings);
            self.playground.use_settings(&settings);
            shell.use_clipboard_access(settings.clipboard_access);
        }
        if let Some(request) = outcome.request {
            self.runner.submit(request, &self.playground_sink);
        }
        if self.general.settings().backend != backend {
            self.read_gpu_memory_for_recommendation();
        }
        self.use_language(shell);
    }

    /// Takes the texts of the language of the General pane, and shows the tray in it, when the
    /// language changed.
    fn use_language(&mut self, shell: &mut dyn Shell) {
        let language = self.general.language();
        if language == self.translations.language() {
            return;
        }
        self.translations = Translations::new(language);
        shell.render_tray(&self.app.tray_view(&self.translations));
    }

    /// Reads the free memory of the GPU of the backend while no suggestion model is selected, so
    /// that the model that TypeAlike recommends fits in it.
    fn read_gpu_memory_for_recommendation(&mut self) {
        let settings = self.general.settings();
        if settings.suggestion_model.is_some() || settings.backend == Backend::Cpu {
            return;
        }
        let backend = settings.backend;
        self.models.start_gpu_memory_read(backend);
        self.runner.submit(
            InferenceRequest::ReadGpuMemory { backend },
            &self.recommendation_sink,
        );
    }

    /// Submits `request` for a suggestion. After the idle timeout or a game, it first loads the
    /// suggestion model again, so that the runner completes the request after the load.
    fn request_suggestion(&mut self, request: InferenceRequest, shell: &mut dyn Shell) {
        if self.app.end_stops_before_request() {
            self.submit_model_request();
            self.render_settings(shell);
        }
        self.runner.submit(request, &self.suggestion_sink);
    }

    /// Unloads the suggestion model while a stop reason applies, and loads it again when no stop
    /// reason applies and no model is in memory.
    fn submit_model_request(&mut self) {
        let request = match self.app.stop_reason() {
            Some(reason) => self.general.unload_suggestion_model(&mut self.playground, reason),
            None if self.playground.is_model_unloaded() => {
                self.general.load_suggestion_model(&mut self.playground)
            }
            None => None,
        };
        if let Some(request) = request {
            self.runner.submit(request, &self.playground_sink);
        }
    }

    /// Uses the stop reason of the app after it changed: the ghost text hides while a stop reason
    /// applies, the suggestion model loads or unloads, and the tray, the badge and the panes show
    /// the new state.
    fn on_stop_change(&mut self, shell: &mut dyn Shell) {
        if self.app.stop_reason().is_some() {
            self.tab_hold.end(shell);
            self.hide_ghost(shell);
        }
        self.submit_model_request();
        shell.render_tray(&self.app.tray_view(&self.translations));
        self.render_waiting_ghost(shell);
        self.render_settings(shell);
    }

    /// Opens the settings window at the page with the icon `page`, else at the page that it shows.
    fn open_settings(&mut self, page: Option<PageIcon>, shell: &mut dyn Shell) {
        self.personalization.cancel_delete();
        let request = self.models.on_open();
        self.hub.submit(request, &self.hub_sink);
        let view = self.settings_view(shell);
        let page = page.and_then(|icon| view.pages.iter().position(|shown| shown.icon == icon));
        shell.show_settings(&view, page);
    }

    fn settings_view(&self, desktop: &dyn Desktop) -> SettingsView {
        let translations = &self.translations;
        let installed = self.models.installed_files();
        let backend = self.general.settings().backend;
        let recommended = self.models.recommended_row(backend, translations);
        let first_run: Vec<Row> = self
            .models
            .recommendation_warning(backend, translations)
            .into_iter()
            .chain(recommended.clone())
            .collect();
        let general = &self.general;
        let mut pages = vec![
            general.page(first_run, translations),
            self.suggestion_places.page(translations),
            self.screen_text
                .page(
                    desktop.ocr_availability(),
                    desktop.ocr_levels(),
                    self.suggestion_places.settings(),
                    translations,
                )
                .with_first_section(general.copied_text_section(translations)),
            self.personalization
                .page(self.suggestion_places.settings(), translations)
                .with_first_section(general.writer_section(translations)),
            self.energy_saving
                .page(self.app.energy_saver(), translations)
                .with_last_section(general.preparation_section(translations)),
            self.models
                .page(backend, translations)
                .with_first_section(general.model_section(&self.playground, &installed, translations)),
        ];
        pages.extend(permissions::page(&desktop.permissions(), translations));
        if self.playground_page == PlaygroundPage::Shown {
            pages.push(self.playground.page(&installed, recommended, translations));
        }
        SettingsView {
            pages,
            language: translations.language(),
        }
    }

    fn render_badge_after_caret_settles(&mut self, shell: &mut dyn Shell) {
        if self.app.has_badge() {
            self.caret_wait.restart_timer(shell);
        } else {
            shell.hide_badge();
        }
    }

    /// Shows the waiting ghost text and the badge at the caret of `read`, with the ghost text cut
    /// to the field of `read`. When the ghost text shows, its app and its website go into the
    /// lists of the panes.
    fn render_at_caret(&mut self, read: CaretRead, shell: &mut dyn Shell) {
        let placement = if self.app.is_caret_at_line_end() {
            GhostPlacement::AfterCaret
        } else {
            GhostPlacement::BelowCaret
        };
        let ghost = self
            .caret_wait
            .ghost_view(self.app.text_before_caret(), placement, |view| {
                fit_to_field(view, read, &*shell)
            });
        shell.render_at_caret(ghost.as_ref(), self.app.badge_view(&self.translations));
        let is_suggestion_shown = ghost.is_some() && shell.is_ghost_shown();
        if is_suggestion_shown {
            self.add_suggested_place();
        }
    }

    fn hide_ghost(&mut self, shell: &mut dyn Shell) {
        self.caret_wait.stop(shell);
        shell.hide_ghost();
    }
}

/// Cuts `view` to the room between the caret of `read` and the right edge of its field, less
/// [`FIELD_EDGE_GAP`]. Without a caret or a field frame with area, the view stays whole.
fn fit_to_field(view: GhostView, read: CaretRead, shell: &dyn Shell) -> GhostView {
    let (Some(caret), Some(field)) = (read.caret, read.field.filter(ScreenRect::has_area)) else {
        return view;
    };
    let room = field
        .right
        .saturating_sub(FIELD_EDGE_GAP)
        .saturating_sub(caret.right);
    view.fit_to(room, |ghost| shell.ghost_width(ghost))
}

/// The desktop of a shell while the rest of a suggestion that Tab took a part of waits for the
/// caret. The rest counts as shown, because the user saw it before Tab.
struct DesktopWithPendingRest<'shell>(&'shell dyn Shell);

impl Desktop for DesktopWithPendingRest<'_> {
    fn field(&self) -> Field {
        self.0.field()
    }

    fn surroundings(&self) -> Surroundings {
        self.0.surroundings()
    }

    fn copied_text(&self) -> Option<CopiedText> {
        self.0.copied_text()
    }

    fn ocr_availability(&self) -> OcrAvailability {
        self.0.ocr_availability()
    }

    fn ocr_levels(&self) -> OcrLevels {
        self.0.ocr_levels()
    }

    fn permissions(&self) -> Vec<(Permission, PermissionState)> {
        self.0.permissions()
    }

    fn now(&self) -> Option<Timestamp> {
        self.0.now()
    }

    fn is_ghost_shown(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests;
