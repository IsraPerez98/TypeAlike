# Windows platform

What `typealike_windows` does. All FFI. The threads and the runtime rules are in `.claude/rules/windows.md`, and the look in `.claude/rules/windows-ui.md`.

## Main thread

- Hooks, overlay, badge at the caret, `SendInput`, tray (Win32, main thread).

### Overlay

- A fix shows a red line through the typed letters, as wide as the letters in the font of the overlay, and the fixed word in green (Fluent colors, or the colors of high contrast).
- Without the color of the field text (also in a terminal of xterm.js), the ghost text after the caret is a neutral gray (`NEUTRAL_GRAY`, the same in light and dark mode, so it shows on a light and on a dark background). In high contrast, it is `COLOR_GRAYTEXT`.
- The overlay draws in the face of the caret when it is installed, else in Segoe UI, always at the height of the caret.

### Card below the caret

- When text follows the caret on its line, the overlay shows the typed word and the ghost text on a card below the line of the caret, with the ghost text under the caret, or above the line at the bottom of the work area.
- The card has the fill and the border of the Fluent tooltip, as the badge.
- The typed word and the typo of a fix are in the Fluent primary text color and semibold.
- The ghost text is in the Fluent secondary text color in light mode and the tertiary one in dark mode (`CardText`).
- In high contrast, they are `COLOR_WINDOWTEXT` and `COLOR_GRAYTEXT`.

### Input and hooks

- A replacement sends a Backspace key for each deleted character before the text.
- The shell queues each replacement, and each inject message types the oldest one.
- The mouse hook gives `WM_MOUSEWHEEL` and `WM_MOUSEHWHEEL` to `Controller::on_scroll`.
- The keyboard hook does not mark the keys that Windows repeats, so a key-down of the key that went down last, without its key-up, is a repeat (`HeldKey`).

## Caret reader

The caret reader runs on its own thread.

### Sources of the caret

- The system caret, else the caret object of the focused window (`OBJID_CARET` of Microsoft Active Accessibility: Chromium browsers and Electron apps such as VS Code), else UI Automation (UIA) for apps that draw their own caret (WinUI).
- Each read of the caret also gives the frame of the focused field: the UIA `CurrentBoundingRectangle` of the focused element when it has an area, else the `GetWindowRect` of the focused window.
- The runtime gives the answer to the newest read to the controller, and the shell draws at the caret of that answer (`NewestCaret`).

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
- A field with the ARIA role `searchbox` (UIA `AriaRole`: a web search box or an HTML `<input type="search">`) is a search field. A native search box gives no ARIA role.
- After a click or a focus move, the field also counts as a password field until a read of the new field tells that it shows its text.
- A read that gives no focused element (`FieldAnswer::Unknown`: the request failed or did not finish in time) does not change whether the field counts as a password field.
- The caret reader waits at most 500 ms for an element and 3 s for each request about an element (`Uia::limit_waits`), so that one app that does not answer cannot stop the reads in all apps.

## Screen text reader

- The screen text reader runs on its own thread with a low priority: the UIA text elements of the foreground window, else the OCR of a band of the window around the field (`Windows.Graphics.Capture` and `Windows.Media.Ocr`).
- The OCR needs Windows 11, because Windows 10 draws a border around the window at each capture.
- It stops while energy saver is on.

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
- It has a color icon of the Fluent UI System Icons for each page in the navigation (`assets/fluent`, the page symbol in high contrast), and a glyph of the icon font at the leading edge of each card of a setting.
- Each window keeps the edits that went to the controller (`TypedTexts`). A view text that is no such edit, while no edit waits, is a change of the controller, and the field shows it.

## Setup window

- The setup window (`settings/setup_page.rs`) is a second WinUI window on the same thread, 720 x 720 DIPs (lower when the work area is too small), in the center of the work area of the main monitor.
- It has no resize frame and no maximize button (`settings/fixed_window.rs`), so that each step of the longest language fits without a scroll bar.
- It has Mica, the Fluent color picture of each step (`assets/fluent/setup`, the symbol in high contrast), the cards of the settings window, a footnote and a footer with a gray dot for each step, Back and the accent Next button.

### Step dots and motion

- An accent pill covers the dot of the step that shows, and the dots after it move away by the width that the pill adds, so the gaps between the dots stay the same.
- When the step changes, the pill and the dots slide to their new places (translation transitions).
- A new step slides in by 40 DIPs and the step before fades out.
- The demo types its text, shows the suggestion in gray, and presses Tab, which types only the next part of the suggestion (`split_next_part`, as Tab does) while the rest stays gray.
- With the animation effects of Windows off (`SPI_GETCLIENTAREAANIMATION`), the step shows in its place and the demo shows its gray suggestion at once.
- A press of a button in the 350 ms of a change of the step does not count, so a double click goes one step.

## Other services

- The physical cores of the CPU and their efficiency classes (`cpu_cores`).
- The spelling dictionaries (`SpellingDictionary`): an `ISpellChecker` for each language of `GlobalizationPreferences::Languages`, on the main thread in the multithreaded COM apartment. `ISpellChecker` gives no completions, so only letters that are a whole word count as the start of a word.

## Autostart

- The General pane writes the `TypeAlike` value in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`. It starts the program that wrote it.
- To turn it on also removes the `TypeAlike` value in `...\Explorer\StartupApproved\Run`, so a stop from Task Manager does not stay (`typealike_windows::StartupEntry`).
