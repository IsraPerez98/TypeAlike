# Ghost text and caret

When and where `typealike_core` shows the ghost text and the badge (`controller`, `GhostView`). The drawing of each platform is in `docs/windows.md` and `docs/macos.md`.

## Caret looks

- After a key, the ghost text shows only at a caret that moved, because a terminal moves its caret only after the program in it writes the text back.
- A shell that reads the caret off its thread (`CaretLook::Later`, Windows) gives each read (`CaretRead`: the caret and the frame of the focused field) to `Controller::on_caret_read`.
- A shell that reads the caret at once (`CaretLook::Now`, macOS) gives the read as its answer.
- Core compares the caret with the caret of the last look that found a caret that moved, and a caret that did not move gets only the badge.
- A focus change, a click and a screen change forget that caret (`Controller::on_context_lost`), so the first look in another field shows the ghost text also at the same place on the screen.
- A caret more than 4 caret heights outside the frame of its field counts as no caret, and core hides the ghost text and the badge.

## Timing

- The ghost text and the badge show 40 ms after a key (`CARET_SETTLE_DELAY`), at the caret that the key moved.
- A suggestion that comes after this wait shows at once (`GhostTiming::Now`).
- A completion does not move the caret, so it does not change this wait. A suggestion that comes during the wait shows when the wait ends.
- A completion that hides the ghost text or asks again also keeps the wait. Thus the badge still shows at the caret that the key moved, and the next suggestion does not wait 40 ms again. Examples:
  - The completion of a request that the next key replaced.
  - A completion that ends before the text that the user typed while it ran. Core asks again (`docs/core/typing.md`, "Keys while a completion runs").
  - A completion that stops for a typo in letters that the dictionary knows. Core asks again without the typo rule (`docs/core/suggestions.md`).
- While the ghost text does not show, core looks at the caret again every 20 ms, at most 20 times, and then shows the ghost text at the caret where it is.
- The fix of a typo of the same length does not move the caret, so the rest after it shows at once.

## Scroll

- While the badge shows, a scroll (`Controller::on_scroll`) hides the ghost text and the badge, and keeps the suggestion.
- They show again at the caret 40 ms after the last scroll event (`CARET_SETTLE_DELAY`).
- The look at the caret that waits ends, because its caret can be stale.
- A scroll does not change the text, so the ghost text also shows again at a caret that did not move.
- A scroll does not end the session and starts no input settle. The next key of the session reads the screen again, and in a chat or an email app the stop of the scroll also reads it (`docs/core/prompt.md`, "Screen reads").

## View

- Each ghost text goes to the platform with its placement (`GhostView`).
- The session keeps the whole suggestion, so each Tab shows the next parts at the new caret.

### Colors

- The ghost text after the caret has the color of the field text (`TextColor`) at the opacity `GHOST_TEXT_ALPHA` (half), as a placeholder.
- Light text (`TextColor::is_light`: more contrast with black than with white) gets the colors of a fix for a dark background.

### Mirror and card below the caret

- When the platform finds no caret, it can show the ghost text in the mirror next to the focused field.
- The mirror (also the card below the caret) shows the end of the typed word before the ghost text (`GhostView::typed_word`: the text after the last space, at most 24 characters, without the typo of a fix).
- The typed word has the primary text color and a semibold font.
- The ghost text has a lighter color with a contrast of at least 4.5:1.

### Placement

- When text follows the caret on its line, the ghost text goes below the caret (`GhostPlacement::BelowCaret`), because at the caret it would cover that text.
- A field that hides the text of that line (`Surroundings::is_caret_line_hidden`: the input box of a terminal of xterm.js) always gets the ghost text below the caret, and no phrases.
- Each look at the caret also tells what follows the caret now (`CaretRead::after`, `AfterCaret`: nothing, text on the line, selected text, or unknown).
- The look decides the placement, because the text that the session read at its start can be stale. An empty field can show a placeholder that goes away at the first key, as Claude Code does.
- Thus a look that sees no text after the caret puts the ghost text after the caret, also when the session read a placeholder there. A look that sees text puts it below the caret.
- When the platform cannot tell (`AfterCaret::Unknown`), the text that the session read at its start counts.
- While the app selects text after the caret, the ghost text hides. An example is the inline completion of the address bar of Chrome. The ghost text then never covers the suggestion of the app.
- The ghost text waits, and core looks at the caret again as for each ghost text that does not show. It shows when the selection goes away.

### Fit in the field

- The ghost text always shows whole. Core never cuts it, so Tab types only text that the user saw.
- When the ghost text after the caret does not fit before the right edge of the frame of its field, less 4 px (`FIELD_EDGE_GAP`), the rest wraps onto the lines below the line of the caret (`GhostView::fit_to`, `WrappedLines`), as the field wraps the text when the user types it.
- Each wrapped line starts 4 px inside the left edge of the frame, one caret height below the line above it.
- A line breaks only after the whitespace between two words, so the closing punctuation stays with its word. The whitespace at the end of a line does not count in its width.
- A first word that does not fit after the caret goes to the next line. The word of a fix always stays at the caret.
- The platform measures each line in the font of its overlay (`Shell::ghost_width`).
- The whole ghost text goes below the caret (`GhostPlacement::BelowCaret`) when the frame has no room below the caret for the wrapped lines (a field of one line, or the last line of a field), or when a word does not fit on a line.
- Without a frame with an area, the ghost text stays whole on the line of the caret.
- After each Tab, core wraps the rest again at the new caret.
