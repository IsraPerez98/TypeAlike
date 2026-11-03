# Ghost text and caret

When and where `typealike_core` shows the ghost text and the badge (`controller`, `GhostView`). The drawing of each platform is in `docs/windows.md` and `docs/macos.md`.

## Caret looks

- After a key, the ghost text shows only at a caret that moved, because a terminal moves its caret only after the program in it writes the text back.
- A shell that reads the caret off its thread (`CaretLook::Later`, Windows) gives each read (`CaretRead`: the caret and the frame of the focused field) to `Controller::on_caret_read`.
- A shell that reads the caret at once (`CaretLook::Now`, macOS) gives the read as its answer.
- Core compares the caret with the caret of the last look that found a caret that moved, and a caret that did not move gets only the badge.
- A caret more than 4 caret heights outside the frame of its field counts as no caret, and core hides the ghost text and the badge.

## Timing

- The ghost text and the badge show 40 ms after a key (`CARET_SETTLE_DELAY`), at the caret that the key moved.
- A suggestion that comes after this wait shows at once (`GhostTiming::Now`).
- While the ghost text does not show, core looks at the caret again every 20 ms, at most 20 times, and then shows the ghost text at the caret where it is.
- The fix of a typo of the same length does not move the caret, so the rest after it shows at once.

## Scroll

- While the badge shows, a scroll (`Controller::on_scroll`) hides the ghost text and the badge, and keeps the suggestion.
- They show again at the caret 40 ms after the last scroll event (`CARET_SETTLE_DELAY`).
- The look at the caret that waits ends, because its caret can be stale.
- A scroll does not change the text, so the ghost text also shows again at a caret that did not move.
- A scroll does not end the session, starts no input settle and reads no screen.

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

### Fit in the field

- The ghost text after the caret keeps only the whole Tab parts that fit before the right edge of the frame of its field, less 4 px (`FIELD_EDGE_GAP`, `GhostView::fit_to`).
- The platform measures each cut in the font of its overlay (`Shell::ghost_width`).
- When not even the first part fits (for a fix, the fixed word), the whole ghost text goes below the caret.
- Without a frame with an area, the ghost text stays whole.
