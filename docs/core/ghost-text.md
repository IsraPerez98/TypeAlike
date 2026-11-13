# Ghost text and caret

When and where `typealike_core` shows the ghost text and the badge (`controller`, `GhostView`). The drawing of each platform is in `docs/windows.md` and `docs/macos.md`.

## Caret looks

- After a key, the ghost text shows only at a caret that moved, because a terminal moves its caret only after the program in it writes the text back.
- A shell that reads the caret off its thread (`CaretLook::Later`, Windows) gives each read (`CaretRead`: the caret and the frame of the focused field) to `Controller::on_caret_read`.
- A shell that reads the caret at once (`CaretLook::Now`, macOS) gives the read as its answer.
- Core compares the caret with the caret of the last look that found a caret that moved, and a caret that did not move gets only the badge.
- Some fields keep the caret at their right side while the user types. Examples are classic Notepad without word wrap, which scrolls the line sideways, and a field that aligns its text to the right.
- Thus a caret near the right side of its field counts as a caret that moved when it stayed at that place over a whole key (`RightSideCaret::Stays`). Near means at most 2 caret heights before the right side (`RIGHT_SIDE_IN_CARET_HEIGHTS`).
- The first key there still waits, because a terminal also gives the old caret at its right side until the program in it writes the key back.
- Without this rule, in classic Notepad, the ghost text showed after 2 percent of the looks at the right side, against 95 percent inside the line, because each key started the looks again. With it, 92 percent of the looks at the right side showed the ghost text.
- A focus change, a click and a screen change forget that caret (`Controller::on_context_lost`), so the first look in another field shows the ghost text also at the same place on the screen.
- A move of the focus to another window hides the ghost text and the badge at once, also without a key or a click (`Controller::on_focus_moved`). Examples are a window that closes, and an app that opens a window in front.
- A caret more than 4 caret heights outside the frame of its field counts as no caret, and core hides the ghost text and the badge.

## Timing

- The ghost text and the badge show 40 ms after a key (`CARET_SETTLE_DELAY`), at the caret that the key moved.
- After Enter, core also reads the field at the end of this wait, so that the suggestion for the new line does not wait for the input to settle (`docs/core/typing.md`, "Enter").
- A suggestion that comes after this wait shows at once (`GhostTiming::Now`).
- A completion does not move the caret, so it does not change this wait. A suggestion that comes during the wait shows when the wait ends.
- A completion that hides the ghost text or asks again also keeps the wait. Thus the badge still shows at the caret that the key moved, and the next suggestion does not wait 40 ms again. Examples:
  - The completion of a request that the next key replaced.
  - A completion that ends before the text that the user typed while it ran. Core asks again (`docs/core/typing.md`, "Keys while a completion runs").
  - A completion that stops for a typo in letters that the dictionary knows. Core asks again without the typo rule (`docs/core/suggestions.md`).
- While the ghost text does not show, core looks at the caret again every 20 ms, at most 20 times, and then shows the ghost text at the caret where it is.
- The fix of a typo of the same length does not move the caret, so the rest after it shows at once.
- Some apps show the accent of a dead key as marked text, so the caret moves after the dead key (`DeadKeyAccent::Marked`). The letter after it takes the place of the accent and does not move the caret. Thus the ghost text after that letter shows at once at the same caret.
- After a dead key that did not move the caret, the next letter moves the caret, so its ghost text waits for a caret that moved.

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
- A field that hides the text of that line (`Surroundings::is_caret_line_hidden`: the input box of a terminal of xterm.js) gets no phrases. It gets the ghost text below the caret, except where the pixels after the caret show no text (see "Pixels after the caret").
- Each look at the caret also tells what follows the caret now (`CaretRead::after`, `AfterCaret`: nothing, text on the line, text only on the lines below, selected text, or unknown).
- The look decides the placement, because the text that the session read at its start can be stale. An empty field can show a placeholder that goes away at the first key, as Claude Code does.
- Thus a look that sees no text after the caret puts the ghost text after the caret, also when the session read a placeholder there. A look that sees text puts it below the caret.
- When the platform cannot tell (`AfterCaret::Unknown`), the text that the session read at its start counts.
- A look with pixels after the caret decides with them, and not with `CaretRead::after` or the text of the session (see "Pixels after the caret").
- While the app selects text after the caret, the ghost text hides. An example is the inline completion of the address bar of Chrome. The ghost text then never covers the suggestion of the app.
- The ghost text waits, and core looks at the caret again as for each ghost text that does not show. It shows when the selection goes away.

### Fit in the field

- Tab types only text that the user saw. Thus the words that do not fit in the field wait for the next Tab, and show after it.
- When the ghost text after the caret does not fit before the right edge of the frame of its field, less 4 px (`FIELD_EDGE_GAP`), the rest wraps onto the lines below the line of the caret (`GhostView::fit_to`, `WrappedLines`), as the field wraps the text when the user types it.
- When the badge shows at the end of the line of the caret, the ghost text on that line ends 4 px before the badge (`FieldRoom::before`). The wrapped lines keep the whole width of the field. Thus a first word that fits only behind the badge goes below the caret.
- Each wrapped line starts 4 px inside the left edge of the frame, one caret height below the line above it.
- A line breaks only after the whitespace between two words, so the closing punctuation stays with its word. The whitespace at the end of a line does not count in its width.
- A first word that does not fit after the caret goes to the next line. The word of a fix always stays at the caret.
- The platform measures each line in the font of its overlay (`Shell::ghost_width`).
- The ghost text does not wrap when a line after the line of the caret has text, so that the wrapped lines do not cover that text (`LinesBelow::WithText`).
- Core finds this text in the look at the caret (`AfterCaret::TextBelow`), or in the text after the caret that the session read.
- Any text after the caret that the session read counts, because some apps give the lines below without line breaks. Firefox gives the blocks of an editable web page so.
- A line with only whitespace counts as empty, because a terminal pads its lines with spaces.
- When the rest cannot wrap, only the words that fit on the line of the caret show after the caret, and the next Tab shows the rest after them. The ghost text then covers no text of the app and no edge of its field.
- Examples are the end of a line in the middle of a document, a field of one line, and the input line above the status line of a terminal app.
- The whole ghost text goes below the caret (`GhostPlacement::BelowCaret`) only when not even its first word, or the word of a fix, fits after the caret.
- Without a frame with an area, the ghost text stays whole on the line of the caret.
- After each Tab, core wraps the rest again at the new caret.

### Pixels after the caret

- In some fields, the platform cannot read what follows the caret. Examples are the editor of VS Code (`Surroundings::has_unreadable_text`) and the input box of a terminal of xterm.js.
- In these fields, each look also gives the pixels of the screen after the caret (`CaretRead::pixels`, `ScreenPixels`).
- The pixels show the field from the top of the line of the caret to at most 4 lines below it (`ScreenPixels::area_after_caret`). The ghost text wraps only onto these lines.
- Core reads them as the text after the caret (`ScreenPixels::after_caret`), only where a ghost text after the caret would show:
  - The line of the caret has no text after the caret when each row of its pixels has one color. The row goes from the caret to 4 px before the right edge of the field (`FIELD_EDGE_GAP`).
  - An empty line has one color in each row, and so does the highlight of the line of the caret, also with a border.
  - The cursor does not count as text. The cursor is the run of columns of one color from the caret, at most one caret height wide: the bar of an editor, or the block of a terminal. The top and the bottom eighth of the line do not count in a column, because the highlight of the line can end there.
  - When each row of the line changes its color at the same column, this column is the edge of a box of the app. It is not text. The line then has no text after the caret, and the field ends at the edge (`PixelsAfterCaret::field`).
  - An example is the input box of opencode in the integrated terminal of VS Code. The box has its own background and no border on the right.
  - Glyphs do not fill each row of the line, so text stays text. A vertical line of one color across the whole line also ends the field, for example a ruler of an editor.
  - The lines below have no text when all their pixels in the field have one color. They start one third of a caret height below the caret, because the caret of a terminal of xterm.js is shorter than its line.
  - One color can change by 12 of 255 in each channel (`SAME_COLOR_RANGE`). The highlight of the line in a dark theme can differ from the background by less. A rule line of a terminal differs by more.
- When the line of the caret has no text after the caret, the ghost text goes after the caret, with the rules of "Fit in the field". It wraps when the lines below have no text. Else only the words that fit on the line of the caret show.
- In a box of the app, the ghost text ends 4 px before the edge of the box. The wrapped lines also end there, and the badge shows 4 px inside the edge.
- When the line of the caret has text after the caret, the ghost text goes below the caret.
- A look without pixels, or with pixels that do not show the whole line of the caret, keeps the rules above. The ghost text of a field that hides its line then goes below the caret.
- Checked on 2026-11-11 with the screenshots of the app tests: the rule agreed with a visual check of 11 of 11 looks in VS Code and 25 of 25 test carets on the same screens. In Windows Terminal, a background with a pattern counted as text on the lines below, so these lines got no wrap.
- A real app test on 2026-11-13 found that the old rule read the edge of the input box of opencode as text. The ghost text then went below the caret and covered the status row of opencode.

## Badge

- A shell that gives the side of its badge (`Shell::badge_side`: Windows) gets the frame of the badge from core (`BadgeView::frame`). A shell without a side puts the badge at the caret by itself (macOS: above the caret, on its left).
- The badge shows at the end of the line of the caret, 4 px inside the right edge of the field (`FIELD_EDGE_GAP`), in the middle of the line and inside the field (`line_end_badge`).
- A line that ends at the caret has no text there, and the ghost text after the caret ends before the badge. Thus the badge covers no text of the field and no ghost text.
- The badge does not cover the lines above and below the caret, the menu bar above the first line, or the card below the caret.
- While the user types at the end of a line, the badge stays at the same place. It moves only when the caret goes to another line.
- The badge hides when it can cover text:
  - while text can follow the caret on its line (`App::is_caret_at_line_end`): text after the caret, a selection, or a field that hides the line of the caret. In a field with pixels after the caret, the pixels tell it ("Pixels after the caret"),
  - when the line has no room for the badge 4 px after the caret,
  - when the line of the caret is outside the field, or the field is lower than the badge,
  - without a field frame with an area.
- A real app test on 2026-11-10 found the old place above the caret on its left wrong. In Notepad, it covered a word of the line above, and on the first line it covered the menu bar.
