# Suggestions

The length of the suggestions, the typo rule and fix, the languages and their dictionaries, and the rules in `typealike_core` that cut or hide a suggestion of the model. The decode itself (the cut of the tail, the sentence stop, the phrase ends) is in `docs/llm.md`.

## Length

- The suggestion lengths go from very short (3 tokens) to very long (22 tokens), with medium (10 tokens) as the default.
- Each length also gives the cut of the tail of its requests: about 2 percent up to medium, and a lower cut for long and very long, so that each token can be as unsure as at medium.
- On the CPU backend, the General pane offers only very short and short, and a longer saved length works as short.
- A request inside a line has no cut of the tail and twice the tokens (`INSIDE_LINE_TOKEN_FACTOR`), because the end that fits the text after the caret ends the suggestion, and a sentence that the writer inserts can then finish.

## Typos

### Rule and fix

- The rule for typos (the Languages and typos pane, on by default) gives each request a minimum for the typed letters of the last word (`TypoRule::typed_letters_cut`, sent as `TypoCheck`).
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

- "The dictionary" here means the dictionaries of the languages that the user writes together (see "Languages and dictionaries").
- When a completion stops for a typo in typed letters that the dictionary of the OS knows as a word or as the start of a word (`SpellChecker::is_word_start`), or that it does not check (fewer than 3 letters, a digit or a sign inside), core asks again without the rule (`TypoCheck::Off`).
- A fix of letters that the dictionary knows as a word also asks again.
- Thus a correct word that the model did not expect, such as "Friday" after "before", gets a suggestion.
- When the completion without the rule continues those letters but gives no suggestion, the fix of the first completion shows. For example, after "a day trip to Snit", the completion "ra" makes "Snitra", which the dictionary does not know, so the fix "Sintra" shows.
- Core asks the dictionary only after such a stop, because a query of the start of a word costs some milliseconds on macOS.

## Where a suggestion stops

### Text after the caret

- A suggestion also stops where it writes the text after the caret on its line again (`SHARED_UNITS`: from a word, or from the closing punctuation that starts that line, the words agree in at least 2 words and signs in a row, or up to the end of the shorter).

### Unknown words

- Each suggestion of the model also stops before its first word that no dictionary of the languages of the user knows (`SpellChecker`), unless the prompt, the text after the caret, the word bias of the pool or the earlier writing holds that word in any case (`engine/spelling.rs`).
- A word of the earlier writing counts with any word choice, because the writer wrote it, so it is no made-up word. With the dictionaries of Windows and of Spanish of Chile, this helped 2 cases of the `new_words` set ("porfa" after a text of today with it) and harmed none. The tuning, held-out, today-harm and language-switch sets did not change. Slang that the writer never wrote still stops a suggestion.
- A word with fewer than 3 letters, a digit or a sign inside, or a capital letter inside (code) needs no dictionary.
- The first word counts with the typed letters before it, so an unknown first word hides the suggestion.
- The fixed word of a typo fix also needs the dictionary, so an unknown fixed word hides the fix. Thus a fix never shows only a part of a word, such as "saba" for "sabado", which the Spanish dictionary of macOS knows only as "sábado".
- The engine keeps each answer of the dictionary, because each answer costs a call to the OS.
- The phrases of the earlier writing and the shown suggestions need no dictionary.
- The cut has no switch: it removes only a word that no dictionary knows and that the user never wrote, such as a word that the model made up.

### Languages and dictionaries

- The user picks the languages that they write in the Languages and typos pane and in the second step of the setup (`LanguagesPane`, `SpellingSettings::languages`). Until the user changes the list, it follows the languages of the user in the settings of the OS that the OS has a dictionary for.
- Each language is a tag of `WRITING_LANGUAGES`, with its name in that language.
- The dictionaries of all the languages check a word together (`JoinedSpellCheckers`): a word is known when any of them knows it. Thus a change of language between two messages, or in one sentence, needs no other step.
- The dictionary of a language (`LanguagesPane::dictionaries`):
  1. The dictionary of the OS with the same tag (`OsDictionaries`). For the main entry of a language, such as "Español", the closest dictionary of the OS for that language also counts.
  2. Else a dictionary that the hub downloads (`DictionaryHub`), after the hub read it.
  3. Until then, and when no dictionary of the language exists, the closest dictionary of the OS for that language, in another region.
- A language that the user adds downloads its dictionary at once when it needs one. A language of the list whose dictionary is missing when TypeAlike starts downloads it after the hub listed its dictionaries.
- Remove deletes the downloaded dictionary of the language. A cancel of a download removes the language.
- Only Windows has a hub (`typealike_dictionaries`, `docs/windows.md`). macOS has a dictionary for many languages, so the menu there offers only the languages that macOS can check.
- Loading many languages makes the cut weaker: with 9 more dictionaries, 16 percent of the made-up words of the tuning set passed in place of 9 percent. Thus TypeAlike loads only the languages of the list.

### Dictionary of another language

- The cut of an unknown word applies only while the dictionaries fit the language of the writer (`Dictionary::fits_writer`): it knows at least half of the last 30 checked words that the writer finished before the caret.
- With fewer than 3 such words, the answer for the last text with enough words counts. Before any such text, the dictionary fits. Thus a new chat message in Spanish keeps the answer of the last message.
- A dictionary that does not fit cuts no word, as without a dictionary.
- The reason: Windows has dictionaries only for the languages that the user added in the settings of Windows. A Spanish writer with only English (United States), who did not add Spanish in the Languages and typos pane, has no Spanish dictionary, and the cut then hid most suggestions in Spanish.
- With the dictionary of each language of the writer, the rule seldom applies. On cases where a paragraph in English comes before a text in Spanish in the same field, the rule alone left 54 of 300 Spanish carets without a suggestion. With the Spanish dictionary added, 22 cases got better and none got worse.
- On the tuning set with the dictionary of such a Windows computer (Gemma 4 E4B, long, without the sentence stop, word replay), the rule gave Spanish carets 6.13 words for each caret in place of 1.73, and 23 carets without a suggestion in place of 89. It helped 35 cases and harmed none. The English carets did not change.

## Suggestions that do not show

- A suggestion of the model does not show when it copies the frame of its prompt (`copies_frame_line`): it holds 5 words in a row of a frame line, or the line of the caret starts as a frame line starts (its first 5 words, or all the words of a shorter line of at least 3).
- The frame lines are the lines of the preface outside its quoted texts, with only the label of the place line (a window title is text that the writer can type), and the heading (`Preface::frame_lines`), so a new label needs no other change.
- The prompt keeps the frame lines when it writes the preface, so a quote sign without its pair in a quoted text (a clipboard text, a screen text, an earlier text or the notes) does not change them. Thus the labels and the name of the writer stay, and no quoted text becomes a frame line.
- A suggestion that opens a bracket just after the end of a sentence (`opens_note_after_sentence`) also does not show.
- A suggestion that makes a word of the name of the writer the second word of the text after a first word that is not a part of that name (`greets_the_writer`, as after "Querida ") also does not show. A sign-off and a field that starts with the name keep the name.
