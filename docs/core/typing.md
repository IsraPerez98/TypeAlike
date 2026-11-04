# Keys, sessions and Tab

How `typealike_core` follows the typing of the user: the key rules, the session, Tab, Escape and the held keys.

## Keys

- Core holds the key-press rules (`Key::from_press`) and the badge click rule (`BadgeClickFilter`).
- Escape always counts, Tab only without a modifier, and Enter and Backspace only alone or with Shift.
- Any other key with a shortcut gives `Key::Other`.
- Else a key gives the typed character, a dead key, or `Key::Other` when it types no single character.
- Each platform maps its key codes and modifiers to the neutral kinds of `key`.
- Core gives the set of line breaks (`desktop::is_line_break`) that the platforms use.

## Fields without suggestions

- A field whose role marks it as a search box (`Surroundings::is_search_field`) gets no suggestions, as the address and subject fields of an email.
- The platform reads the role, never the labels, so the rule works in any language.
- A field whose app or website is off works as a paused field (`docs/core/settings-window.md`, "Apps and websites").

## Session

- A session lasts while the user types in the same field.
- A click and a focus change end the session.
- A dead key (`Key::Dead`) does not end the session.

### Field text

- Core reads each no-break space of the field text (U+00A0, U+202F, U+2007) as a plain space (`excerpt::plain_spaces`), because a web editor shows a typed space at the end of a text as a no-break space.
- Without this, the typo rule and the token healing take the space as a part of the last word.

### Enter

- After Enter, core reads the field again when the input settles or at the next key.
- When the field shows the text of the session (or the field text that core read at Enter, because a dead key or an autocorrect can change the typed text), line breaks, and then no letters, the session continues on the new line, and the settle asks the model for a suggestion there.
- Else the session ends.

### Read of the field when the input settles

- When the input settles without Enter, core reads the field again (`Engine::follow_field`).
- When the field shows the text that the user typed since the last read, or that text with the same words, each with at most 2 changed characters, the same word at the caret and the same last character, the session takes the text of the field.
- The word at the caret of a read counts as typed, because the app changes a word when the user finishes it.
- Thus an autocorrect of the app goes into the prompt and into the earlier writing.

## Keys while a completion runs

- While a completion runs, a typed key that continues the text that the completion added so far keeps its request, also when that text does not show yet.
- This does not occur when the earlier writing gives a phrase or next words for the new text.
- When the completion then gives no ghost text after the typed text, core asks again as after a typed key.

## Held keys

- A key that the OS repeats while the user holds it (`Press::Repeat`) changes the text of the session, but asks the model for no suggestion.
- The ghost text hides, and the input settle then asks one time for the suggestion at the new text (`Engine::suggest_after_hold`).
- A repeated key that continues the ghost text still takes it, and a key that the user presses still asks at once.

## Shown suggestions

- Each session keeps the last 16 suggestions that the ghost text showed, each with its text before the caret (`ShownSuggestions`).
- When the text before the caret is such a text and then a start of its suggestion that is shorter than the suggestion, core shows the rest at once, with no request.
- Thus a backspace back to a place where a suggestion showed, or a retype of the suggested letters after a wrong letter, needs no model.
- Only the text of a completion goes into this memory, never a fix or a phrase.
- An earlier suggestion comes before a phrase.
- A dead key and Enter keep the memory.

## Escape

- Escape forgets each kept suggestion that the text before the caret continues (`ShownSuggestions::forget_at`), so a suggestion that the user rejected does not show again without the model.
- Escape while the ghost text shows also pauses the suggestions in the field for 10 seconds (`Reaction::PauseField`, `Timer::FieldPause`, `FIELD_PAUSE_DELAY`).
  - The keys there still change the text of the session, but they get no request, no phrase and no earlier suggestion.
  - A key that ends the session keeps the pause, and a click or a focus change ends it.
  - The next key after the pause gets a suggestion again.
- Escape while no ghost text shows only goes to the app.

## Tab

- Tab types the next part of a suggestion: the next word with the whitespace before it.
- The closing punctuation, quotes and brackets at the end of a word (`CLOSING_SIGNS`) are not a part of that word. The next Tab types them alone, also after a fix ("Best| regards, Sam" takes " regards", then ",", then " Sam").
- A word of only signs is one part.
- Tab takes a suggestion only while the ghost text shows on the screen (`Desktop::is_ghost_shown`).
- After a Tab, the rest of the suggestion also counts as shown while it waits for the caret, so that a quick Tab types its next part.
- After the last part, core asks for the next suggestion as after a typed key (`AfterAccept::Next`). When the completion still runs, core asks when that completion ends without more text.
- Tab on a typo fix: `docs/core/suggestions.md`, "Typos".

### Tab hold

- Until the next suggestion shows, a quick Tab does not reach the app while a suggestion can still come (`TabHold`, at most 800 ms): after a request, a shown phrase or a completion that still runs (`App::has_offer`).
- Core swallows that Tab and keeps the session.
- The held Tab does not take the next suggestion, because Tab types only text that the user saw.
- A Tab while the next ghost text shows, any other key, a click, a focus change, a pause and a completion without a suggestion end the hold.
