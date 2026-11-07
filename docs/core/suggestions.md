# Suggestions

The length of the suggestions, the typo rule and fix, and the rules in `typealike_core` that cut or hide a suggestion of the model. The decode itself (the cut of the tail, the sentence stop, the phrase ends) is in `docs/llm.md`.

## Length

- The suggestion lengths go from very short (3 tokens) to very long (22 tokens), with medium (10 tokens) as the default.
- Each length also gives the cut of the tail of its requests: about 2 percent up to medium, and a lower cut for long and very long, so that each token can be as unsure as at medium.
- On the CPU backend, the General pane offers only very short and short, and a longer saved length works as short.
- A request inside a line has no cut of the tail and twice the tokens (`INSIDE_LINE_TOKEN_FACTOR`), because the end that fits the text after the caret ends the suggestion, and a sentence that the writer inserts can then finish.

## Typos

### Rule and fix

- The rule for typos (the General pane, on by default) gives each request a minimum for the typed letters of the last word (`TypoRule::typed_letters_cut`, sent as `TypoCheck`).
- With "Fix typos" (on by default, shown while the rule is on), the request also asks for a fix.
- A completion with a `Correction` shows the typed letters struck through and the fixed word after the caret (`Reaction::ShowFix`, `Ghost::Fix`), and Tab deletes the typed letters and types the fixed word (`Reaction::Replace`).

### Space or sign after the typo

- A space or one of `, ; : ! ?` after the typed letters keeps the fix, with at most one sign and then one space (`fix_after_delimiter`).
- The typo and the fixed word then end with that space or sign. Thus the red line goes through it, and Tab deletes it and types it again after the fixed word.
- The rest of the suggestion keeps its text after that space or sign, and a rest that does not start with it goes.
- Another key asks again.

### Autocorrect of the app

- At Tab on such a fix, core reads the field.
- When the text before the caret does not end with the typo, the app changed the word (an autocorrect). Then Tab replaces nothing (`Reaction::Swallow`) and the session ends.
- A read that ends with less of the space or the sign, or no field text, allows the replace.

### Dictionary of the OS

- When a completion stops for a typo in typed letters that the dictionary of the OS knows as a word or as the start of a word (`SpellChecker::is_word_start`), or that it does not check (fewer than 3 letters, a digit or a sign inside), core asks again without the rule (`TypoCheck::Off`).
- A fix of letters that the dictionary knows as a word also asks again.
- Thus a correct word that the model did not expect, such as "Friday" after "before", gets a suggestion.
- Core asks the dictionary only after such a stop, because a query of the start of a word costs some milliseconds on macOS.

## Where a suggestion stops

### Text after the caret

- A suggestion also stops where it writes the text after the caret on its line again (`SHARED_UNITS`: from a word, or from the closing punctuation that starts that line, the words agree in at least 2 words and signs in a row, or up to the end of the shorter).

### Unknown words

- Each suggestion of the model also stops before its first word that the dictionary of the OS (`SpellChecker`, a service of the bin) does not know, unless the prompt, the text after the caret or the word bias of the pool holds that word in any case (`engine/spelling.rs`).
- A word with fewer than 3 letters, a digit or a sign inside, or a capital letter inside (code) needs no dictionary.
- The first word counts with the typed letters before it, so an unknown first word hides the suggestion.
- The engine keeps each answer of the dictionary, because each answer costs a call to the OS.
- The phrases of the earlier writing and the shown suggestions need no dictionary.
- `language::closest_languages` gives the dictionary of each preferred language of the user.

## Suggestions that do not show

- A suggestion of the model does not show when it copies the frame of its prompt (`copies_frame_line`): it holds 5 words in a row of a frame line, or the line of the caret starts as a frame line starts (its first 5 words, or all the words of a shorter line of at least 3).
- The frame lines are the lines of the preface outside its quoted texts, with only the label of the place line (a window title is text that the writer can type), and the heading (`Preface::frame_lines`), so a new label needs no other change.
- The prompt keeps the frame lines when it writes the preface, so a quote sign without its pair in a quoted text (a clipboard text, a screen text, an earlier text or the notes) does not change them. Thus the labels and the name of the writer stay, and no quoted text becomes a frame line.
- A suggestion that opens a bracket just after the end of a sentence (`opens_note_after_sentence`) also does not show.
- A suggestion that makes a word of the name of the writer the second word of the text after a first word that is not a part of that name (`greets_the_writer`, as after "Querida ") also does not show. A sign-off and a field that starts with the name keep the name.
