# Windows platform

What `typealike_windows` does. All FFI. The threads and the runtime rules are in `.claude/rules/windows.md`, and the look in `.claude/rules/windows-ui.md`.

## Main thread

- Hooks, overlay, badge at the caret, `SendInput`, tray (Win32, main thread).
- The tray icon is the robot of the logo, from the icon files in `assets/tray` (`docs/icons.md`).

### Overlay

- A fix shows a red line through the typed letters, as wide as the letters in the font of the overlay, and the fixed word in green (Fluent colors, or the colors of high contrast).
- Without the color of the field text (also in a terminal of xterm.js), the ghost text after the caret is a neutral gray (`NEUTRAL_GRAY`, the same in light and dark mode, so it shows on a light and on a dark background). In high contrast, it is `COLOR_GRAYTEXT`.
- The overlay draws in the face of the caret when it is installed, else in Segoe UI, always at the height of the caret.
- The overlay draws each wrapped line of a ghost text in a bitmap of its own, then puts the lines in one bitmap: the first line at the caret, and each wrapped line at the left edge that core gives, one caret height below the line above it.
- When the overlay cannot show a ghost text, it hides the old ghost text, so that Tab never types a suggestion that the user did not see.
- With `TYPEALIKE_LOG=typealike_windows=debug`, the log tells at each render at the caret whether core gave a ghost text, a badge and a caret ("rendered at the caret"). It also gives the placement of each ghost text that shows, its number of wrapped lines and the rectangle that the overlay covers. It also gives the frame of the foreground window without its invisible borders (`DWMWA_EXTENDED_FRAME_BOUNDS`) and the edges of that frame that the overlay goes past (`leaves_window`, `crossed_edges`). It never gives the text.

### Card below the caret

- When text follows the caret on its line, the overlay shows the typed word and the ghost text on a card below the line of the caret, with the ghost text under the caret, or above the line at the bottom of the work area.
- The card stays inside the frame of the window in front, in the work area (`display::part_inside`). Thus a card at a caret near the right edge or the bottom of its window moves left or above the line.
- The card has the fill and the border of the Fluent tooltip, as the badge.
- The typed word and the typo of a fix are in the Fluent primary text color and semibold.
- The ghost text is in the Fluent secondary text color in light mode and the tertiary one in dark mode (`CardText`).
- In high contrast, they are `COLOR_WINDOWTEXT` and `COLOR_GRAYTEXT`.

### Badge

- The badge shows in the frame that core gives it, at the end of the line of the caret (`docs/core/ghost-text.md`, "Badge"). Without a frame, it hides.
- The shell gives core the side of the badge (`Shell::badge_side`): the height of the line of the caret, from 16 to 24 DIPs on the monitor of the caret (`badge::side_on_line`).
- The glyph is at most 16 DIPs and keeps 2 DIPs of room inside each edge. Thus a badge lower than 20 DIPs has a smaller glyph.
- The badge menu opens below the badge, with its right edge at the right edge of the badge (`TPM_RIGHTALIGN`), so it opens toward the field.
- With `TYPEALIKE_LOG=typealike_windows=debug`, the log at each render at the caret also gives the frame of the badge (`badge_frame`).

### Input and hooks

- A replacement sends a Backspace key for each deleted character before the text.
- The shell queues each replacement, and each inject message types the oldest one.
- The mouse hook gives `WM_MOUSEWHEEL` and `WM_MOUSEHWHEEL` to `Controller::on_scroll`.
- The keyboard hook does not mark the keys that Windows repeats, so a key-down of the key that went down last, without its key-up, is a repeat (`HeldKey`).
- A WinEvent hook (`EVENT_OBJECT_FOCUS`, out of context, not for the own process) gives each focus move in another app to `Controller::on_focus_moved`. Thus the ghost text and the badge hide when another window comes to the front without a key, for example when the window in front closes.

## Caret reader

The caret reader runs on its own thread. When it cannot connect to UIA, it tries again at each later read.

### Sources of the caret

- The system caret, else the caret object of the focused window (`OBJID_CARET` of Microsoft Active Accessibility: Chromium browsers and Electron apps such as VS Code), else UI Automation (UIA) for apps that draw their own caret (WinUI).
- Each read of the caret also gives the frame of the focused field: the UIA `CurrentBoundingRectangle` of the focused element when it has an area, else the client area of the focused window (`GetClientRect`, `ClientToScreen`).
- In a terminal of xterm.js, the frame of the field is the frame of the screen of the terminal (the class `xterm-screen`), because the hidden input box is only one cell.
- In Windows Terminal (the UIA class `TermControl`), the frame of the field ends at the right edge of the box around the caret (`CaretBox`). Thus the ghost text, its wrapped lines and the badge stay inside that box.
  - A terminal app such as opencode draws its input box as cells with a background of their own. The box often has no border character on the right, so the text of the row cannot show its edge.
  - The box is the run of cells on the row of the caret, from the caret, with the background of the cell before the caret (UIA `BackgroundColor`). The cell at the caret is always in the box, because an app can draw its own cursor there in other colors.
  - Windows Terminal has no `FindAttribute`, so a binary search with `GetAttributeValue` finds the end of the box. For a range of several cells, Windows Terminal gives the background without the last cell. Thus the read also checks the last cell of the box alone.
  - When the box goes to the end of the row, as in a shell, the frame stays the frame of the terminal.
- A UIA frame covers the whole element, also the part that the edge of the window or a scrolled page hides. Thus the frame of the field is only the part of the UIA frame inside the client area of the focused window (`display::visible_field_frame`), and the wrapped ghost text stays inside the window. A scroll container inside the window can still hide a part of the frame.
- When the client area shows no part of the UIA frame (a hidden focus window without a size), the frame stays the whole UIA frame. A mirrored window (right to left) maps the corners of its client area in the opposite order (`display::rect_between`).
- The runtime gives the answer to the newest read to the controller, and the shell draws at the caret of that answer (`NewestCaret`).
- With `TYPEALIKE_LOG=typealike_windows=debug`, the log gives the time, the caret, the UIA frame, the client area, the frame of the field and what follows the caret of each read of the caret, and the area of its pixels after the caret.

### What follows the caret

- Each read of the caret also tells what follows the caret now (`AfterCaret`, from the UIA `TextPattern` of the focused element): selected text, else text after the caret on its line, else text on the lines below, else nothing.
- The read takes one character of the selection and at most 400 characters after it (`CHARS_TO_FIND_LINE_TEXT`), so that a slow app does not hold the read. A line with only spaces in these characters has no text, because a terminal pads its lines with spaces. 400 characters hold the padding of a line of a terminal on a wide monitor and the start of the line below.
- In Windows Terminal, the line of the caret ends at the end of the box around the caret, and the text after the box counts as the lines below (`AfterCaret::seen_on_line`).
- An app that fills each row up to its last column, as opencode does in a narrow window, makes Windows Terminal join the rows without line breaks. Thus the read ends the line at the end of the box, and not at a line break. Else the left border of the next row counts as text after the caret, and the ghost text goes below the caret, over the lower border of the box.
- Claude Code in Windows Terminal shows a rule and status lines below its input line, so its reads give text on the lines below, and a long ghost text does not wrap over them.
- The address bar of Chromium browsers gives its inline completion as the selection, so the ghost text hides while it shows.
- Claude Code in Windows Terminal gives its placeholder as text after the caret in an empty field. The look sees that the placeholder is gone after the first key.
- The answer is `Unknown` for a password field, for an element without a `TextPattern`, and for a read that fails. Core then uses the text that the session read at its start (`docs/core/ghost-text.md`, "Placement").

### Pixels after the caret

- UIA cannot tell what follows the caret in the editor of VS Code (class `native-edit-context`) and in the input box of a terminal of xterm.js. The editor gives a message in place of its text, and the input box has none of the text of the terminal.
- In these two fields, each read of the caret also captures the pixels after the caret (`caret_pixels.rs`), in the area that core asks for (`ScreenPixels::area_after_caret`). Core decides what they show (`docs/core/ghost-text.md`, "Pixels after the caret").
- `Windows.Graphics.Capture` captures only the foreground window (`CreateForWindow`), so the ghost text, the card and the badge of TypeAlike are never in the pixels.
- Other ways that did not fit:
  - A copy of the screen also holds the ghost text of the last look, because the ghost text stays at the old caret while the user types its letters.
  - `SetWindowDisplayAffinity` with `WDA_EXCLUDEFROMCAPTURE` on the overlay would also hide the ghost text from screen sharing, recordings and screenshots.
  - A GDI read of the window of VS Code gives no pixels, because Chromium draws with the GPU.
- The start of a capture took 40 to 100 ms for VS Code on 2026-11-11, too long for each read. Thus the capture of the window stays open while the reads need its pixels.
- The handler of the frames keeps only the newest frame, because a frame keeps its buffer, and a capture without a free buffer gives no new frames. Each read copies only the area after the caret from that frame: 0.5 to 4 ms in a release build.
- A field read that finds one of these fields starts the capture, so that the first read of the caret does not wait for it.
- A read of the caret in another window or another field stops the capture, and so do 60 s without a read (`IDLE_CAPTURE_TIMEOUT`).
- When the window changes its size, the next read starts the capture again.
- Windows 10 draws a border around the window at each capture, so there the reads give no pixels, and the ghost text stays below the caret.
- The pixels go only to core, which does not keep them. With `TYPEALIKE_LOG=typealike_windows=debug`, the log gives the area of the pixels and the time of their capture at each read of the caret ("read the caret"), and the time of each start of a capture. It never gives the pixels.

### UIA caret

- A UIA caret with a value that is not finite counts as no caret.
- The UIA caret is the right edge of the character before it.
- At the start of the field or after a line break, it is the left edge of the character after it, else the left edge of its line (`ExpandToEnclosingUnit` with `TextUnit_Line`), else no caret.
- A line that starts before the caret is the line above, and the caret is one line lower.
- A caret shorter than a line of text does not count, because Qt apps such as Telegram keep a transparent 2 x 2 px system caret for input methods.
- In a terminal of xterm.js (the class `xterm-helper-textarea` in UIA), the caret moves up to the row of the terminal screen that holds the hidden input box, because VS Code draws that box below the top of its row.

### Color and font of the caret

- Each caret has the color and the font face of the text before it (UIA `ForegroundColor` and `FontName`), but not in a terminal of xterm.js, and not when that color is the `BackgroundColor`: Chromium drops the alpha of the transparent text of a hidden input box.
- A system caret keeps the face of `WM_GETFONT` and takes the UIA face only when the window gives none.

### Field reads

- The caret reader also reads the text around the caret (UIA `TextPattern`, else a classic edit control), the field labels (UIA `LabeledBy`, `Name`, then `HelpText`, else `FullDescription`) and the host of the web page (the URL in the value of the nearest Chromium or Gecko document) 40 ms after each key, click and focus move.
- The shell keeps that read for the hook, because the hook must not wait for UIA.
- A field that UIA marks as a password field (`IsPassword`) gets no suggestions.
- The window title is the title of the foreground window (`GetWindowTextW`). Chrome, Brave, Opera and Vivaldi write their private window mark only in the accessible title, not in this title.
- Thus in a Chromium window (class `Chrome_WidgetWin_1`), the field read also reads the name of the `BrowserRootView` child of the window in UIA, and that name replaces the window title (`ShownField::window_title`). Core then finds the private windows of these browsers too (`docs/core/settings-window.md`, "Private windows").
- Checked on 2026-11-10: Chrome 2026-11 gives "Page - Google Chrome (Incognito)" there, and the window title "Page - Google Chrome". A Firefox private window has its mark in the window title. An app of Electron has no such view and keeps its window title.
- A field with the ARIA role `searchbox` (UIA `AriaRole`: a web search box or an HTML `<input type="search">`) is a search field. A native search box gives no ARIA role.
- A field with the class `native-edit-context` in UIA (the editor of VS Code) has unreadable text (`Surroundings::has_unreadable_text`): UIA gives its text only while the screen reader mode of VS Code is on.
- After a click or a focus move, the field also counts as a password field until a read of the new field tells that it shows its text.
- A read that gives no focused element (`FieldAnswer::Unknown`: the request failed or did not finish in time) does not change whether the field counts as a password field.
- The caret reader waits at most 500 ms for an element and 3 s for each request about an element (`Uia::limit_waits`), so that one app that does not answer cannot stop the reads in all apps. Only the UIA object of Windows 8 and later (`CUIAutomation8`) can set these limits, so `Uia::connect` makes that object.

## Screen text reader

- The screen text reader runs on its own thread with a low priority: the UIA text elements of the foreground window, else the OCR of a band of the window around the field (`Windows.Graphics.Capture` and `Windows.Media.Ocr`).
- The OCR needs Windows 11, because Windows 10 draws a border around the window at each capture.
- The OCR reads the band of the whole window, also when core asks for the content of the field (`OcrArea::Content`).
- It stops while energy saver is on.
- When it cannot connect to UIA, it tries again at each later read.

## Clipboard reader

- The clipboard reader reads the copied text on its own thread, so that the thread of the hooks never waits for it. A read can wait for the app that copied the text (delayed rendering, `WM_RENDERFORMAT`). Windows removes a keyboard hook that does not answer in time, and gives no notice.
- The main window gets `WM_CLIPBOARDUPDATE` after each copy and asks the reader for a read. The main thread keeps only the answer to the newest read (`ClipboardWatch`).
- A copy or a change of the clipboard access after a read makes its answer stale. From a copy until its answer, the controller gets no copied text.

## Watches

### Energy saver

- A watch of the energy saver (`EnergySaverWatch`: `PowerManager::EnergySaverStatusChanged`) sends each change to the main thread.

### App in front

- A watch of the app in front on its own thread with a message loop (`FrontAppWatch`) sends whether the app in front is a game, and whether it shows in full screen.
- A game is a program in the game list of Windows (`HKCU\System\GameConfigStore\Children`, each entry of the kind 1 with its `MatchedExeFullPath`). Game Bar and Game Mode keep this list, which is not a documented API.
- A WinEvent hook (`EVENT_SYSTEM_FOREGROUND`) tells the thread each change of the foreground window, and a registry notification each change of the list.
- A window in full screen (`full_screen`) fills its whole monitor (`rcMonitor`) and has no title bar (`WS_CAPTION`).
- A maximized window keeps its title bar, also when the taskbar hides itself.
- The windows of the shell (the process of `GetShellWindow`: the desktop, Alt+Tab and Task View) also fill the monitor, so they do not count.
- Windows sends no event when a window goes into or out of full screen, so the thread also reads the window in front every second.

## Settings window

- The settings window uses WinUI 3 through windows-reactor, on its own UI thread, as the Settings app.
- It has a color icon of the Fluent UI System Icons for each page in the navigation (`assets/fluent`, the page symbol in high contrast), and a glyph of the icon font at the leading edge of each card of a setting. The About page has the question mark in a circle, because the Fluent icons have no color icon with the letter i.
- The settings and setup windows show the app icon of `TypeAlike.exe` in the title bar and on the taskbar button (`settings/window_icon.rs`).
- Each window keeps the edits that went to the controller (`TypedTexts`). A view text that is no such edit, while no edit waits, is a change of the controller, and the field shows it.
- The installers put a copy of the Windows App Runtime next to `TypeAlike.exe` (the `self-contained` feature). windows-reactor then does not use the dynamic dependency API (`AddPackageDependency`), which only Windows 11 has. The program loads this API only at its first call, so it starts on Windows 10.

## Setup window

- The setup window (`settings/setup_page.rs`) is a second WinUI window on the same thread, 720 x 720 DIPs (lower when the work area is too small), in the center of the work area of the main monitor.
- It has no resize frame and no maximize button (`settings/fixed_window.rs`), so that each step of the longest language fits without a scroll bar.
- It has Mica, the picture of each step, the cards of the settings window, a footnote and a footer with a gray dot for each step, Back and the accent Next button.
- The welcome step shows the app logo (`assets/logo.png`). It keeps its colors in high contrast, as the app icons of Windows do.
- Each other step shows a Fluent color picture (`assets/fluent/setup`), and the symbol of the step in high contrast (`settings/icons.rs`).

### Step dots and motion

- An accent pill covers the dot of the step that shows, and the dots after it move away by the width that the pill adds, so the gaps between the dots stay the same.
- When the step changes, the pill and the dots slide to their new places (translation transitions).
- A new step slides in by 40 DIPs and the step before fades out.
- The demo types its text, shows the suggestion in gray, and presses Tab, which types only the next part of the suggestion (`split_next_part`, as Tab does) while the rest stays gray.
- With the animation effects of Windows off (`SPI_GETCLIENTAREAANIMATION`), the step shows in its place and the demo shows its gray suggestion at once.
- A press of a button in the 350 ms of a change of the step does not count, so a double click goes one step.

## Other services

- The physical cores of the CPU and their efficiency classes (`cpu_cores`).
- The spelling dictionaries (`WindowsDictionaries`, `SpellingDictionary`): the languages of the user are `GlobalizationPreferences::Languages`, and the languages with a dictionary are `ISpellCheckerFactory::SupportedLanguages`. The controller opens an `ISpellChecker` for each language that it picks, on the main thread in the multithreaded COM apartment. `ISpellChecker` gives no completions, so only letters that are a whole word count as the start of a word.
- Windows has a spell checker only for the languages that the user added in Time & language. It does not use Hunspell files in `%APPDATA%\Microsoft\Spelling` as a new language.
- The downloaded dictionaries (`typealike_dictionaries::HunspellDictionaries`, the `DictionaryHub` of core): Hunspell dictionaries from `wooorm/dictionaries` on GitHub at a pinned commit, in `%LOCALAPPDATA%\TypeAlike\Dictionaries\<language>`.
  - Each file must have the size and the SHA-256 digest of the catalog (`catalog/pinned.rs`, from `make_catalog.py`). The files go to `<language>.part` first, and the folder gets its name when each file is right.
  - A worker thread reads each dictionary with `spellbook` (8 ms and about 4 MB for Spanish), so a read never delays the hooks. A check takes about 2.5 µs.
  - Hunspell gives no completions either, so only a whole word counts as the start of a word.
- A web page of the About pane opens in the default browser through `ShellExecuteW` (`web_page.rs`), on a thread of its own in the multithreaded COM apartment, so that the start of the browser never delays the hooks.

## Autostart

- The General pane writes the `TypeAlike` value in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`. It starts the program that wrote it.
- To turn it on also removes the `TypeAlike` value in `...\Explorer\StartupApproved\Run`, so a stop from Task Manager does not stay (`typealike_windows::StartupEntry`).
