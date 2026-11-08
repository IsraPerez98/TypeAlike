# macOS platform

What `typealike_macos` does. All FFI. The look is in `.claude/rules/macos-ui.md`.

## Parts

- Event tap.
- Accessibility (AX) for the caret, the text around it, the field labels (`AXTitle`, else the text of `AXTitleUIElement`, `AXDescription`, `AXPlaceholderValue`, `AXHelp`) and the host of the web page.
- The window title is the `AXTitle` of the AX window of the field. Chromium browsers give their accessible title there, with the private window mark ("Google Chrome (Incognito)"). Safari gives ", Private Browsing".
- The static text of the focused window, else the OCR of a band of the window around the field (`ScreenCaptureKit` and Vision, only with the Screen & System Audio Recording permission).
- Overlay panel.
- The mirror, for a field that gives no caret through AX (the editor of VS Code while its screen reader mode is off: an empty EditContext element).
- CGEvent injection.
- Menu bar extra: the robot of the logo as a template image, and a yellow image while a stop of the Energy saving pane applies (`docs/icons.md`).
- Settings window: a sidebar of the panes, each with its symbol on a colored tile, as in System Settings.
- Setup window.
- Spelling dictionary.
- Web pages of the About pane: `NSWorkspace` opens them in the default browser.

## Permissions

- TypeAlike needs Accessibility access (`cargo run` from a terminal: grant it to the terminal app).
- Without it, TypeAlike starts without the event tap, shows no prompt of macOS, opens the Permissions pane, and reads the permission every second (`AXIsProcessTrusted`). It starts the tap when the user gives the permission, with no restart.
- "Give access" first runs `tccutil reset Accessibility|ScreenCapture <bundle id>` (`permissions::ask`), because a build with an ad-hoc signature (a release before 0.7.0, a build from the source code) changes its signature, and macOS refuses the grant of an older build while System Settings shows it on. Then the prompt of macOS adds TypeAlike to the list again.
- A program without a bundle (`cargo run`) skips the reset.
- Each activation of TypeAlike reads the permissions again.
- The OCR also needs Screen & System Audio Recording, which `cargo run` also takes from the terminal app.
- The runtime calls `Controller::on_launch` in the first pass of the run loop, before it waits for the Accessibility permission.

## Screen reads and watches

- The screen reads run one at a time on a dispatch queue with a low priority.
- The block of that queue never returns, so each read drains its own autorelease pool.
- An observer of Low Power Mode (`EnergySaverObserver`: `NSProcessInfoPowerStateDidChangeNotification` on the main queue) gives each change to the runtime.
- At the start, at each activation of an app and at each change of the space, the runtime reads whether the app in front shows in full screen (`full_screen`): the presentation options of the active app (`currentSystemPresentationOptions`) have `FullScreen` or `HideMenuBar`.
- At the start and at each activation of an app, the runtime reads whether the app in front is a game (`game`): its `Info.plist` has `public.app-category.games` or a genre that ends with `-games` in `LSApplicationCategoryType`, the key that Game Mode uses.

## Screen text

- The read of the AX tree looks at the siblings of the focused field first, then at the siblings of each parent, up to the window. It looks at 2000 elements at most (`MAX_ELEMENTS`).
- An element that does not overlap the window does not show. An element without area can hold elements that show, so the read looks at its children.
- A static text with a frame less than 2 pt high does not show (`MIN_TEXT_HEIGHT`). Chrome gives a static text that scrolled out of the view a frame 1 pt high at the top edge of the view.
- Chromium gives each run of a paragraph as its own static text, for example a bold word. The runs carry their own spaces. A run that wraps has a frame from the top of its first row to the bottom of its last row, with the left edge of the block.
- Two static texts of one element that follow each other join into one line when the second continues the first (`continues`):
  - The second overlaps the row of the first.
  - The left edge of the second is at most 2 pt right of the right edge of the first (`MAX_RUN_GAP`). A run that wraps starts left of that edge, so it continues too.
- A line of joined texts has the texts as they are, with no added space, and the smallest frame that holds them.
- A static text that does not show never joins. The static texts of a child element, for example a link, do not join the static texts around it.
- The OCR uses the visible part of the field: the part of the field frame in the frame of its AX window. A text view that scrolled can have a frame taller than its window, for example in TextEdit.
- The OCR captures the window of the app that holds the middle of the visible part. It reads a band from 600 pt above the visible part to 600 pt below it (`BAND_POINTS`).
- Two windows of one app can have the same frame. Of the windows that hold that point, the OCR captures the window with the longest title that starts the title of the AX window, else the first one (`picked_window`). Chrome gives its name after the title of the page only in the AX title.
- When the frame of the AX window is unknown, or the field has no part in it, the OCR uses the full frame of the field.
- The OCR keeps the lines of its last read, and reads again only when the pixels of the band change (`pixels_hash`). The pixels of the field do not count, so that typing does not start a new OCR.
- In a field that shows its text only on the screen, the OCR reads the text of the field, so its pixels count. Thus a new answer in a terminal starts a new OCR.

## Fields

- Chromium browsers and Electron apps build the AX tree of their web content only after an assistive app asks for it. When an app comes to the front, the focus tracker sets `AXEnhancedUserInterface` on a Chromium browser and `AXManualAccessibility` on any other app (`wake_web_accessibility`).
- An Electron app that still starts answers that request with `CannotComplete`, and then it builds no tree. Thus the tracker asks again at each key and each settled input until the app answers (`FrontmostApp::is_web_content_asleep`). The request waits at most as long as the other AX queries (`bound_queries`).
- Each look at the caret also gives the frame of the focused field.
- A field whose app does not answer the read of its subrole in time (0.1 s, `AXError::CannotComplete`) counts as a password field.
- A field with the subrole `AXSearchField` (a native search field or an HTML search box) is a search field. The address bar of Chrome gives no subrole.
- A field with the DOM class `native-edit-context` (the editor of VS Code) has unreadable text (`Surroundings::has_unreadable_text`): its tree holds the text only while the screen reader mode of VS Code is on.
- The terminal of Ghostty gives its mouse selection as its selection, never its cursor, and gives no caret (`gives_mouse_selection_only`).
  - Its text then does not tell where the caret is. The value would put the whole screen after the caret.
  - Thus a field of Ghostty without a caret gives no field text and has unreadable text, and core reads its text from the OCR of the screen (`docs/core/prompt.md`, "Field that hides its text").
  - When Ghostty gives a caret, its field text counts again.
- The hidden input box of a terminal of xterm.js (the terminal of VS Code) never holds the text of the terminal. Thus it also gives no field text and has unreadable text, and core reads the terminal from the OCR.
  - For a screen read, its field frame is the frame of the terminal screen (`xterm-screen`), so that the lines of the terminal are the lines of the field.
  - The tree of VS Code holds the text of the terminal only while its screen reader mode is on. Without the OCR, the prompt got only the side bar and the status bar.

## Caret

- The caret is the right edge of the character before it (`AXBoundsForRange`).
- At the start of the text or after a line break, the caret is the left edge of its line, with the top and the height of that line (`AXLineForIndex`, `AXRangeForLine`, then `AXBoundsForRange` of the line).
- A line that starts before the caret is the line above, and the caret is one line lower.
- When the line of the caret is empty, as after the last line break in TextEdit, the line of the character before the caret gives the box.
- When that read gives no box, as in a contenteditable field of Chromium, the caret is the box of the selection as a text marker range (`AXSelectedTextMarkerRange`, `AXBoundsForTextMarkerRange`).
- A selection box that is the frame of the field gives no caret, because Chromium gives that frame for a field without laid-out text (the editor of VS Code).
- A caret box needs a height above 0 and finite edges, and the frame of a field needs finite edges.

### Terminal of xterm.js

- In a terminal of xterm.js (the class `xterm-helper-textarea` in `AXDOMClassList`), the caret is the left edge of that hidden input box, moved up to the row of the terminal screen that holds it.
- The text in the box does not give the caret, because xterm.js keeps the text that TypeAlike types there until Enter.
- The text in the box is not the field text either (see "Fields").

## Text after the caret

- When the read of the text after the caret fails, macOS cuts the text from `AXValue`.
- A Chromium contenteditable puts a line break between blocks in `AXValue`, but its range text joins the blocks. Thus the cut first moves the selection past the line breaks that the range text leaves out (`value_offset_after`).
- When the two texts differ in more than these line breaks, the cut keeps the offsets of the range text.
- A cut never splits a character. When a limit of the text around the caret falls in an emoji, the half of the emoji goes.

## CodeMirror

- In a CodeMirror editor (the DOM class `cm-content`, as in Obsidian), the range text of Chromium has each line after the other with no line break, and one line break for each empty line.
- macOS then reads the text of each child of the editor, which is one line (`AXStringForTextMarkerRange` of `AXTextMarkerRangeForUIElement`), and puts a line break between the lines (`focus/surroundings/code_mirror.rs`). A read of 37 lines takes about 5 ms.
- A line gives U+FFFC for a widget and a line break for the `<br>` of an empty line. Both go.
- At the start of a line after a line with text, the range text has the same offset for the end of the line before. The caret is on the line when the middle of the caret box is not above the top of the line.
- When the lines do not give the range text before the caret, the range text stays.
- CodeMirror holds only the lines that it draws, so the text before the caret starts at the first line that it draws.
- The text of the lines also loses the half of a character at each end of its cut.

## Event tap

- The event tap also watches the scroll-wheel event and gives it only to `Controller::on_scroll`, with no AX read and no read of the pasteboard.
- A key-down event with the field `CGEventField::KeyboardEventAutorepeat` is a repeat.
- macOS turns the tap off after a slow callback (`TapDisabledByTimeout`) or for user input (`TapDisabledByUserInput`). The tap then turns itself on again at once, without the state of the runtime.
- When the tap cannot join the main run loop, TypeAlike invalidates its port.

## Overlay

- A fix: an attributed string with a red line through clear letters, then the fixed word in `systemGreen`.
- The overlay reads the color and the font face of the character before the caret (`AXForegroundColor` and `AXFont` of `AXAttributedStringForRange`, not in a terminal of xterm.js, and not for text with an alpha below 0.5) and draws in `DarkAqua` behind light text, else in `Aqua`.
- It draws in that face when it is installed, else in the system font, at the size whose line fits the caret.
- A face name that does not belong to its family (Word gives "Helvetica" for each face) gives the family.
- The shell reads the style at its first use after each look at the caret, so the measure and the drawing use the same font.
- The wrapped lines of a ghost text are in a second panel, one line each, at the left edge that core gives, with the first one a caret height below the line of the caret.
- One label with a paragraph indent for the first line does not work, because the label does not count the indent in its size and cuts the first line.
- With Increase Contrast, the ghost text is `secondaryLabelColor`.
- Without that color (also in a terminal of xterm.js), it is `systemGrayColor`, which is the same gray in the light and the dark appearance, so it shows on a light and on a dark background.

## Mirror

- The mirror is a panel below the frame of the field, or above it, with the typed word in `labelColor` and the semibold system font, and the ghost text in `labelColor` at 60% opacity (`secondaryLabelColor` with Increase Contrast).
- A control without a text selection gets no mirror.
- When text follows the caret on its line, the mirror shows the typed word and the ghost text below the line of the caret, with the first letter of the ghost text under the caret, or above the line.

## Injection

- A replacement posts a delete key for each deleted character first.
- TypeAlike creates all the key events of a replacement before it posts the first. When it cannot create one, it posts none.
- The shell queues the replacements, and each inject pass types the oldest one.

## Spelling dictionary

- The spelling dictionary (`SpellingDictionary`): `NSSpellChecker` for the languages of `NSLocale.preferredLanguages` (Language & Region), on the main thread.
- A start of a word (`is_word_start`) is a known word, else a word that `completionsForPartialWordRange` gives in a language (it also gives "después" for "despues").

## Setup window

- The setup window (`settings/setup.rs`) is a window with a full-size content view and a transparent title bar.
- It has the SF Symbol of each step in a system color, the rows of the settings window (`SectionList`, shared with the settings page), the demo and a footer with the dots, Back and the default Next button.
- The dot of the step that shows is as wide as a pill, under an accent pill, so the gaps between the dots stay the same.
- The pill slides to the next dot while that dot grows and the old dot shrinks (`NSAnimationContext` with an implicit animation of the layout), and moves at once with Reduce Motion.
- A new step fades in and slides in (`CABasicAnimation` of `objc2-quartz-core`), and the step before goes at once. With Reduce Motion, it only fades in.
- The setup controls send their own actions (`Runtime::on_setup_control`), because the control tags of the two windows overlap.
- A close of the window by the user (`NSWindowWillCloseNotification`) tells the controller, and `close_setup` only hides the window.

## Labels

- The title case of a menu keeps a word with a dot as it is, such as the host of a website.
- Labels get title case only in a language that has it (`UiLanguage::has_title_case`: English), and the other languages keep the sentence case of core.

## Autostart

- The General pane writes the launchd agent `~/Library/LaunchAgents/io.github.israperez98.typealike.plist` (`typealike_macos::LaunchAgent`). It starts the program that wrote it.
- The autostart switch is on only when the agent starts the program that runs now, and that program is still at its path. An agent from a moved copy or from `cargo run` counts as off.
