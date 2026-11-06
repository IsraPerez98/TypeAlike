# Settings window, setup and menus

The settings model, the panes, the badge menu, the setup and the UI languages (`typealike_core`: `settings`, `settings_window`, `setup`, `app`, `localization`). The rules for the UI text are in `.claude/rules/ui-text.md`.

## Pages and rows

- Core has the settings model, and the Models, Playground, General, Apps and websites, Copied and screen text, Personalization, Energy saving and About panes.
- Each pane (`settings_window`) gives its page of the settings window as a tree of sections and rows (`settings_window::view`), with a stable id for each control.
- Each row of a setting also has a picture before its label (`RowIcon`).
- Each platform has one renderer for each kind of row and sends back what the user did with a control, so a new setting needs no platform code.

## Settings file

- The settings file is `%LOCALAPPDATA%\TypeAlike\settings.json` on Windows, `~/Library/Application Support/TypeAlike/settings.json` on macOS.
- The General, Apps and websites, Copied and screen text, Personalization, Energy saving and Models panes and the setup write it through `typealike_settings::SettingsFile`.
- Without a settings file, the bin gives the settings of the first start (`SettingsFile::new`):
  - The name of the writer is the full name of the user account (`account_name` of each platform crate).
  - The notes are the default notes in the language of the UI (`WriterNotes::default_for`, `ui_language` of each platform crate).
  - The setup is to do (`Setup::Needed`).
  - On Windows, the backend is the GPU backend of the build (`typealike_llm::DEFAULT_BACKEND`: CUDA, else Vulkan, else the CPU). A build without `vulkan` and `cuda` thus starts on the CPU and recommends the model for the CPU.
- A file of an older version without `setup` counts as finished.

## General pane

- The General pane (`GeneralPane`) keeps the settings that no other pane keeps.
- Its page shows the language menu first, then the autostart switch and how suggestions show.
- The sections of the General pane show the copied text switch on the Copied and screen text page, the name and the notes of the writer on the Personalization page, "Prepare suggestions early" on the Energy saving page, and the suggestion model and the backend menus at the top of the Models page.
- The suggestion length, the rule for typos and "Stop at the end of a sentence" are in `docs/core/suggestions.md` and `docs/llm.md`.

### Reset all settings

- The last section of the General pane has "Reset all settings", which needs a second press (`ResetPress`, as "Delete all earlier writing").
- Any other input cancels the first press.
- A reset gives each pane the settings of the first start (`SettingsStore::first_start_settings`), keeps the suggestion model and the downloaded models and the earlier writing, and opens the setup at its first step.

## Energy saving pane and stops

- The Energy saving pane (`EnergySavingPane`) has:
  - the idle timeout (off, 15, 30 or 60 minutes, 30 by default)
  - the switch "Stop in energy saver" (on by default)
  - the switch "Stop during games" (on by default)
  - the switch "Stop in full screen" (on by default)

### Idle timeout

- Each key or click starts the idle timeout again (`Timer::Idle`).
- When it runs out, the session ends and the suggestion model unloads.
- The next request for a suggestion first loads the model again (`App::end_stops_before_request`), and the runner completes the request after the load.

### Energy saver

- While the energy saver of the computer is on (`EnergySaver`: energy saver or battery saver on Windows, Low Power Mode on macOS) and the switch is on, suggestions stop as in a pause: no request, no prepare, no screen read and no phrase.
- The model unloads, and the badge and the tray show "TypeAlike (off to save energy)".
- The platform gives the state at start (`Startup`) and each change (`Controller::on_energy_saver`).
- When the energy saver goes off, the model loads again, unless the idle timeout ran out.

### Games

- While a game is in front (`GameActivity`) and the switch "Stop during games" is on, suggestions stop as in the energy saver, the model unloads, and the badge and the tray show "TypeAlike (off during a game)".
- The platform gives each change of the app in front (`Controller::on_game`).
- After the game, the model stays unloaded until the next request for a suggestion, as after the idle timeout. Thus a quick switch out of a game does not load the model.

### Full screen

- While an app in full screen is in front (`FullScreenActivity`: a video, a slideshow or a game that fills the display) and the switch "Stop in full screen" is on, suggestions stop as during a game, and the badge and the tray show "TypeAlike (off in full screen)".
- The platform gives each change (`Controller::on_full_screen`), and after the full screen the model also stays unloaded until the next request for a suggestion.

### Reason of a stop

- The stops that last until that request (idle, game, full screen) are one set (`App::stops_until_request`).
- `App::stop_reason` gives the reason for an unloaded model (pause, energy saver, game, full screen, idle, in this order), and the model menu of the General and Playground panes tells it.

## Apps and websites

- The Apps and websites pane (`SuggestionPlaces`) has a switch for each app and for each website (host) where a suggestion showed.
- TypeAlike adds each place with its switch on when its first ghost text shows on the screen (`Desktop::is_ghost_shown`).
- The Copied and screen text and Personalization panes add the app at the same time. Thus a window without a text field, such as a game, never goes into these lists.
- A website matches only its exact host.

### A place that is off

- A field whose app or website is off works as a paused field: a key gets no request, no ghost text and no phrase, and Tab goes to the app.
- The settled input reads no screen and prepares no prompt there.
- Without a session, only a key that types or deletes a character reads the field, one time.
- The badge there shows as paused ("TypeAlike (off here)").
- A session in a place that the user turns off ends, and the earlier writing keeps none of its text. The pause keeps that text.
- The Copied and screen text and Personalization panes dim the row of an app whose suggestions are off.

## Badge menu

- The badge at the caret opens a menu (`BadgeMenu`):
  - pause or resume
  - the suggestion switches of the app and of the website where the user types ("Show suggestions in <app>", "Show suggestions on <host>")
  - the screen text switch of the app
  - the scope of its earlier writing (Off, Only this app, All apps)
- An entry is dimmed while the main switch of its pane is off.
- The screen text switch and the scope are also dimmed while suggestions are off in that app or on that website.
- The suggestion switches stay enabled.
- A pick changes the settings as the pane does.

## Model recommendation

- Until a model is downloaded, the General pane shows first the row of the model that TypeAlike recommends for the backend, and the Playground pane shows it below its model menu (`ModelsPane::recommended_row`).
- On the CPU, it is Qwen3.5 0.8B.
- On a GPU, it is the first of Gemma 4 E4B, Gemma 4 E2B and Qwen3.5 2B whose file, 768 MiB of context (`CONTEXT_MEMORY`, Gemma 4 E4B takes 480 MiB on Metal with 4096 tokens, thus about 700 MiB with the 8192 tokens of a GPU), 64 MiB for a drafter (`DRAFTER_MEMORY`, only Gemma 4) and the margin of a load (`GPU_MEMORY_MARGIN`, 1 GiB) fit in the free memory of the GPU, else Qwen3.5 0.8B.
- While no suggestion model is selected, the controller asks the runner for the free memory of the GPU of the backend (`InferenceRequest::ReadGpuMemory`) at the start and at each change of the backend.
- Until the answer comes, or when the read fails, a GPU gets Gemma 4 E2B.
- After a failed read, a warning above the row tells that TypeAlike cannot read the graphics card (`ModelsPane::recommendation_warning`).
- The Models pane marks the same model in its list.
- When the first download finishes and no suggestion model is saved, that model loads as the suggestion model.

## Drafters

- Gemma 4 E2B and E4B have a drafter: a second file of their repository, of about 60 MB, that makes suggestions show sooner (`ModelFile::drafter`, `docs/llm.md`).
- Each model file of such a repository uses that drafter, also a file from the Hugging Face search.
- The hub downloads the drafter after its model, and the Models pane shows the download of the model only. A failed download of the drafter goes only to the log, because the model runs without it.
- A delete of the last model of a repository also deletes its drafter.
- The lists of the Models pane and the model menus show no drafter, vision projector or part of a split model.
- When the hub lists the downloaded models, the Models pane asks it to download each drafter that a downloaded model does not have (`ModelsPane::on_event`). This download shows no row and no progress.
- The hub lists the downloaded models when TypeAlike starts with a suggestion model that has a drafter, and each time the settings window or the setup opens. Thus a model that the user downloaded before TypeAlike had drafters also gets its drafter.
- The pane asks for each drafter only once while TypeAlike runs, so a failed download tries again only after TypeAlike starts again. A model that downloads at that time gets its drafter with its own download.
- The runner uses a new drafter from the next load of its model.

## Permissions pane

- The Permissions pane (`settings_window::permissions`) shows each permission that the OS asks the user for (`Desktop::permissions`: Accessibility and Screen & System Audio Recording on macOS).
- Windows asks for none, so it has no Permissions pane.
- Each row shows "Allowed" or a "Give access" button (`Shell::ask_for_permission`).
- `Controller::on_missing_permission` opens the settings window at that pane (`Shell::show_settings` with the index of the page), and `Controller::on_permission_change` shows a new state.

## About pane

- The About pane (`settings_window::about`) is the page after the Permissions pane, before the Playground pane.
- Its first row shows "TypeAlike" and the version of the build, with "Check for updates" at the trailing edge. A link below it opens the source code (`Cargo.toml`, `repository`) in the web browser (`Shell::open_web_page`).
- The footer tells that TypeAlike checks for a new version only when the user asks, and that TypeAlike has the GNU General Public License, version 3 or later.

### Check for updates

- TypeAlike never checks for a new version by itself. Only a click on "Check for updates" asks the release feed for the newest release (`ReleaseFeed::read_newest`, `typealike_releases::GitHubReleases`). A second click while a check runs does nothing.
- The feed reads the newest release of the GitHub API (`/releases/latest`), which skips drafts and pre-releases.
- While the check runs, the row shows "Checking for updates…" and the button is dimmed.
- A tag `vX.Y.Z` higher than the version of the build shows "Version X.Y.Z is available." and a "Download" button, which opens the page of the newest release in the web browser.
- The same version or a lower one shows "This is the newest version."
- A failed read, or a tag of another form, shows that TypeAlike cannot check for updates, with "Try again".
- TypeAlike does not download or install the new version by itself.

### Third-party software

- The section "Third-party software" lists each third-party library of the build (`Services::libraries`), with its license and its version, and "Show license".
- "Show license" replaces the list with the text of each license file of the library, and "Back to the list" shows the list again.
- `packaging/build_licenses.py` writes the list of each build (`crates/typealike/src/platform/<os>_licenses.rs`) from `cargo metadata`: the normal dependencies of the bin for the target, without the build and the dev dependencies, and the license files of each package.
  - A package without a license file gets the MIT license file of its repository from `packaging/licenses`.
  - The llama.cpp in `llama-cpp-sys-2` adds its `LICENSE`, and the Windows build adds the license of the Fluent UI System Icons.
- CI runs the script and fails when the files change. Thus a change of the dependencies needs the script.

## Setup

- The setup (`setup`) is a window of its own that shows at each start while `Settings::setup` is `Setup::Needed` (`Controller::on_launch`).
- Until the user finishes it, "Settings" of the tray and a missing permission also open it.
- Next waits until a model is on the computer, so a download must finish, and for the Accessibility permission.
- A close by the user (`Controller::on_setup_closed`) keeps the setup to do, and the setup starts again at its first step.
- Each platform renders `SetupView` with the row renderers of the settings window.
- Only Welcome, the permission and the ready step have a body below the title (`SetupView::body`), because the rows of the other steps tell what they are for.

### Steps

In the order of `SetupStep`:

1. Welcome: what TypeAlike does, a demo of a suggestion and the language menu.
2. The name and the notes with their examples.
3. What suggestions can use: the switches of the earlier writing, the copied text and the screen text, and below them: TypeAlike uses the internet only to find and download models, and to check for updates when the user asks.
4. The permissions, only on an OS that asks for Accessibility: Accessibility, and Screen & System Audio Recording while the screen text switch is on.
5. The model, from top to bottom:
   - the backend menu
   - the recommended row, or "Checking the graphics card…" while the read of the GPU memory runs
   - the menu of the suggestion model while a model is downloaded
   - last, a quiet link "Choose another model" (`Row::Link`), an option that most users do not need. The link shows the search of Hugging Face as a text field and the lists of the Models page (`ModelsPane::browser_sections`) in place of the recommended row until the step changes.
6. The ready step: where to try TypeAlike, where its icon is, and "Open at login", on by default, which applies at the finish.

## UI languages

- The texts of the UI (`localization`) come from a Fluent file for each language (`crates/typealike_core/locales/<tag>/typealike.ftl`): English (`en-US`, the source texts), Spanish (`es`), Brazilian Portuguese (`pt-BR`), French (`fr`), German (`de`) and Italian (`it`).
- `UiLanguage::for_preferred` picks the first preferred language of the user that has a file, with the rule of the spelling dictionaries (`language::closest_languages`), else English.
- `Translations` gives the text of a message with its named arguments (`MessageArgument`).
- A test makes sure that each file has each message of English one time, with the same arguments.
- In a debug build, a message that the file does not have, or an argument that a message needs and does not get, panics.
- Each pane, the tray menu, the badge and the badge menu take their texts from `Translations`, and each view gives the language of its texts (`SettingsView::language`, `TrayView::language`, `BadgeMenu::language`).
- A number with a fraction (`MessageArgument::Decimal`) gets the decimal separator of the language, and the Models pane writes the units of a size through the messages.

### Language menu

- The platform gives the language at start (`Startup::ui_language`).
- The language menu of the General pane and of the Welcome step of the setup (`GeneralPane::language_section`) replaces it: "System language", then the name of each language in that language (`UiLanguage::native_name`).
- A pick shows the panes, the setup and the tray in the new language at once, and notes that are an example take the text of that example in the new language.
- The settings file keeps the pick (`ui_language`, `null` for the system language).
