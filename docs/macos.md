# macOS platform

What `typealike_macos` does. All FFI. The look is in `.claude/rules/macos-ui.md`.

## Parts

- Event tap.
- Accessibility (AX) for the caret, the text around it, the field labels (`AXTitle`, else the text of `AXTitleUIElement`, `AXDescription`, `AXPlaceholderValue`, `AXHelp`) and the host of the web page.
- The static text of the focused window, else the OCR of a band of the window around the field (`ScreenCaptureKit` and Vision, only with the Screen & System Audio Recording permission).
- Overlay panel.
- The mirror, for a field that gives no caret through AX (the editor of VS Code while its screen reader mode is off: an empty EditContext element).
- CGEvent injection.
- Menu bar extra: the robot of the logo as a template image (`docs/icons.md`).
- Settings window: a sidebar of the panes, each with its symbol on a colored tile, as in System Settings.
- Setup window.
- Spelling dictionary.

## Permissions

- TypeAlike needs Accessibility access (`cargo run` from a terminal: grant it to the terminal app).
- Without it, TypeAlike starts without the event tap, shows no prompt of macOS, opens the Permissions pane, and reads the permission every second (`AXIsProcessTrusted`). It starts the tap when the user gives the permission, with no restart.
- "Give access" first runs `tccutil reset Accessibility|ScreenCapture <bundle id>` (`permissions::ask`), because each release has another ad-hoc signature, and macOS refuses the grant of an older build while System Settings shows it on. Then the prompt of macOS adds TypeAlike to the list again.
- A program without a bundle (`cargo run`) skips the reset.
- Each activation of TypeAlike reads the permissions again.
- The OCR also needs Screen & System Audio Recording, which `cargo run` also takes from the terminal app.
- The runtime calls `Controller::on_launch` in the first pass of the run loop, before it waits for the Accessibility permission.

## Screen reads and watches

- The screen reads run one at a time on a dispatch queue with a low priority.
- An observer of Low Power Mode (`EnergySaverObserver`: `NSProcessInfoPowerStateDidChangeNotification` on the main queue) gives each change to the runtime.
- At the start, at each activation of an app and at each change of the space, the runtime reads whether the app in front shows in full screen (`full_screen`): the presentation options of the active app (`currentSystemPresentationOptions`) have `FullScreen` or `HideMenuBar`.
- At the start and at each activation of an app, the runtime reads whether the app in front is a game (`game`): its `Info.plist` has `public.app-category.games` or a genre that ends with `-games` in `LSApplicationCategoryType`, the key that Game Mode uses.

## Fields

- Each look at the caret also gives the frame of the focused field.
- A field whose app does not answer the read of its subrole in time (0.1 s, `AXError::CannotComplete`) counts as a password field.
- A field with the subrole `AXSearchField` (a native search field or an HTML search box) is a search field. The address bar of Chrome gives no subrole.

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

## Text after the caret

- When the read of the text after the caret fails, macOS cuts the text from `AXValue`.
- A Chromium contenteditable puts a line break between blocks in `AXValue`, but its range text joins the blocks. Thus the cut first moves the selection past the line breaks that the range text leaves out (`value_offset_after`).
- When the two texts differ in more than these line breaks, the cut keeps the offsets of the range text.

## Event tap

- The event tap also watches the scroll-wheel event and gives it only to `Controller::on_scroll`, with no AX read and no read of the pasteboard.
- A key-down event with the field `CGEventField::KeyboardEventAutorepeat` is a repeat.

## Overlay

- A fix: an attributed string with a red line through clear letters, then the fixed word in `systemGreen`.
- The overlay reads the color and the font face of the character before the caret (`AXForegroundColor` and `AXFont` of `AXAttributedStringForRange`, not in a terminal of xterm.js, and not for text with an alpha below 0.5) and draws in `DarkAqua` behind light text, else in `Aqua`.
- It draws in that face when it is installed, else in the system font, at the size whose line fits the caret.
- A face name that does not belong to its family (Word gives "Helvetica" for each face) gives the family.
- The shell reads the style at its first use after each look at the caret, so the measure and the drawing use the same font.
- With Increase Contrast, the ghost text is `secondaryLabelColor`.
- Without that color (also in a terminal of xterm.js), it is `systemGrayColor`, which is the same gray in the light and the dark appearance, so it shows on a light and on a dark background.

## Mirror

- The mirror is a panel below the frame of the field, or above it, with the typed word in `labelColor` and the semibold system font, and the ghost text in `labelColor` at 60% opacity (`secondaryLabelColor` with Increase Contrast).
- A control without a text selection gets no mirror.
- When text follows the caret on its line, the mirror shows the typed word and the ghost text below the line of the caret, with the first letter of the ghost text under the caret, or above the line.

## Injection

- A replacement posts a delete key for each deleted character first.
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
