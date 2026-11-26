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
- With "Fix typos" (on by default, shown while the rule is on), the request also asks for a fix. Then the runner also takes typed letters that are much less likely than a fix one edit away for a typo (`docs/llm.md`).
- A completion with a `Correction` shows the typed letters struck through and the fixed word after the caret (`Reaction::ShowFix`, `Ghost::Fix`), and Tab deletes the typed letters and types the fixed word (`Reaction::Replace`).
- A bad suggestion after the fixed letters hides the fix (`is_bad_suggestion`). A rest with no word, such as a period after the fixed word, is not bad there, because the fixed word comes before it.

### Space or sign after the typo

- A space or one of `, ; : ! ?` after the typed letters keeps the fix, with at most one sign and then one space (`fix_after_delimiter`).
- The typo and the fixed word then end with that space or sign. Thus the red line goes through it, and Tab deletes it and types it again after the fixed word.
- The rest of the suggestion keeps its text after that space or sign, and a rest that does not start with it goes.
- Another key asks again.

### Word before the typed space

- A request also asks the runner to check the finished word before the last typed space and to fix it (`finished_word_fix`, `docs/llm.md`).
- This occurs when only that space, or that space and the typed start of the next word, follow the word, with at most one sign before the space (`through_finished_word`). The word needs at least 3 characters, and the request must ask for fixes.
- A request does not check a word that a completed request checked (`Session::checked_word`), except while the fix of that word shows. Thus the next key after such a fix checks the word again, and the fix can show again with that key.
- The reason: the user often types the space and the start of the next word before a request after the space completes. Then no request checked the word.
- Core then shows the fix in the same way: the typo and the fixed word end with the space, the sign before it and the typed letters after it (`word_fix`). Tab deletes them and types them again after the fixed word.
- A key after the typed letters of such a fix asks again.
- When the dictionary knows that word, core asks again without the rule, and the fix does not show, also when the answer gives no suggestion.
- When core does not show such a fix, because the suggestion after it is bad or the dictionary does not know the fixed word, it also asks again without the rule. Thus the suggestion after the space still shows.

### Guesses of the dictionary

- After a stop for a typo without a fix, in 5 or more typed letters that the dictionary does not know as a word or as the start of a word, core asks the dictionaries of the language of the text for their guesses (`SpellChecker::guesses`). It then asks the model again once, with the guesses that are one edit away from the typed letters and keep their start, at most 3 (`typed_letters_fix`, `DictionaryGuesses`).
- Fewer letters have many words one edit away. With 3 or 4 letters, the guesses gave 2 right fixes ("atne" to "antemano", "pni" to "pino") and 1 wrong fix ("pge" through "pre" to "preterir").
- The model rates each guess, and takes the most likely one as the fix, with the same minimum and gap as the fix of a token (`docs/llm.md`).
- A typed space after a word that the dictionary does not know gives the model no guesses. The model then rated a regional word below a guess: the Chilean "polera" became "pollera".
- Measured with Gemma 4 E4B (Metal, macOS English and Spanish dictionaries), after the fix of the accents: 7 more right fixes in `typo_kinds` (74 to 81 of 96, none wrong), with the defaults and with long and without the sentence stop. 3 of them are in the 76 older cases ("goverment", "siguente" and "adress,"), and 4 in the `inner` cases, which come from stops without a fix. The eval marked no case as helped or harmed, because a stop and a fix both show no wrong suggestion. `typos` and, with a request after each character and word choice medium, the Spanish and Chilean cases of `varieties` and the Argentine cases of `regional` did not change.
- With the Hunspell dictionaries of English and of Spanish of Chile, `typo_kinds` got the same 7 more right fixes. But 38 Spanish and Chilean cases of `varieties` with a request after each character got 1 wrong fix: "polola," to "pololo". macOS does not know "polola" either, but it offers only "polo" in its place, so no guess is one edit away.
- A stop that gets a fix now waits for a second request and the decodes of the guesses: in `typo_kinds`, such keys took about 400 to 770 ms in place of about 120 ms without a suggestion.

### Accents of a finished word

- When the user types a space after a word, with one of `, ; : ! ?` before it or none, and the dictionaries of the language of the text do not know that word, core asks them for the words that they offer in its place (`SpellChecker::guesses`, `accent_fix`).
- When exactly one of these words differs from the typed word only in more marks on its letters, that word is the fix, with the capital letters of the typed word. Examples: "comí" for "comi", "también" for "tambien" and "pequeño" for "pequeno". The letters compare after Unicode NFD without the combining marks.
- The request then has the fixed word in its prompt and no typo rule. Its completion shows as the fix of a typo (`Ghost::Fix`), and its parts do not show.
- The model does not rate this fix. It often finds the word without the accent likely, because the first token of the word agrees with the typed letters, and the runner then finds no fix (`docs/llm.md`).
- A typed word with a mark gets no such fix. A voseo form that the dictionary does not know, such as "tenís" or "sabés", would else lose its accent, because the dictionary offers "tenis" and "sabes".
- The dictionaries of macOS know most names without their accents, such as "Jose", "Ramon" or "Lopez", so a name seldom gets such a fix there. The Hunspell dictionary of Spanish of Chile knows none of them, and offers only "Ramón", "Andrés" and "López" with the same letters. Thus in a Spanish text, such a name of 5 or more letters gets the accent. A shorter name, such as "Jose" or "Raul", gets none, because the English dictionary knows it.
- Core asks for the guesses while it handles the space, which runs in the keyboard hook of the OS. On macOS, a request took about 5 ms (median) and at most 10 to 20 ms for each language. With Hunspell without its guesses of letter groups, it took 0.2 ms for English and 2 ms for Spanish of Chile (median), and at most 57 ms. Core keeps each answer, and asks only after a finished word that the dictionary does not know.
- A word that the dictionary of the other language knows gets no fix, such as "camion" (an English word) after "Hola Carla, El". A word before a period or a line break also gets no fix.
- A space that shows the rest of a suggestion, or that the running completion continues, asks no request, so the word gets no fix. Thus after "comi", a suggestion " con mis primos" keeps "comi".
- Measured with Gemma 4 E4B (Metal, macOS English and Spanish dictionaries): 4 more right fixes in `typo_kinds` (70 to 74 of 96, none wrong): "comí", "acá", "según" and "fiancé", all in the `end` cases. The 76 older cases did not change, and the eval gave 3 helped cases, none harmed (sign test p = 0.25). With a request after each character and word choice medium, the Spanish and Chilean cases of `varieties` (75) and the Argentine cases of `regional` (21) did not change. With the Hunspell dictionaries of English and of Spanish of Chile, `typo_kinds` also got 4 more right fixes.

### Autocorrect of the app

- At Tab on such a fix, core reads the field.
- When the text before the caret does not end with the typo, the app changed the word (an autocorrect). Then Tab replaces nothing (`Reaction::Swallow`) and the session ends.
- A read that ends with less of the space, the sign or the typed letters after the space, or no field text, allows the replace.

### Dictionary of the OS

- "The dictionary" here means the dictionaries of the languages that the user writes together (see "Languages and dictionaries").
- When a completion stops for a typo in typed letters that the dictionary of the OS knows as a word or as the start of a word (`SpellChecker::is_word_start`), or that it does not check (fewer than 3 letters, a digit or a sign inside), core asks again without the rule (`TypoCheck::Off`).
- A fix of letters that the dictionary knows as a word also asks again.
- Thus a correct word that the model did not expect, such as "Friday" after "before", gets a suggestion.
- With a dictionary, a last word of fewer than 3 characters asks without the rule at once (`typo_check_at_caret`). A stop for a typo there always asks again, and the runner gives no fix for so few letters. Thus the answer is the same, without a second request.
- Other letters that the dictionary does not check keep the rule, because the runner can fix them, such as "WEdnesday" to "Wednesday".
- Measured with Gemma 4 E4B (Metal, long, macOS English and Spanish dictionaries): no fix changed in 808 such places of `varieties`, 178 of `chat` and in `typo_kinds`, and no suggestion changed because of the rule.
- A last word that a dictionary that fits the text knows as a word also asks without the rule at once (`needs_no_typo_check`). A request also does not check such a finished word before a typed space. A stop for a typo or a fix there always asked again, so the answer is the same without a second request.
  - A dictionary fits the text when it knows at least half of the last 30 checked words that the writer finished before the caret, and they are at least 3. With fewer words, or when no dictionary fits, the word keeps the rule.
  - A short word of another language keeps the rule, such as "como" in an English text, unless the dictionary of that language also fits the text.
- When the completion without the rule continues known letters but gives no suggestion, the fix of the completion with the rule shows (`KnownWord`). For example, after "a day trip to Snit", the completion "ra" makes "Snitra", which the dictionary does not know, so the fix "Sintra" shows.
  - With the rule first, core keeps its fix of the known letters, asks again without the rule, and shows the kept fix.
  - Without the rule first, core asks again with the rule, and shows its fix. This request asks nothing again.
  - Thus the same fixes show in both orders, and only these rare answers need a second request.
- Core asks for the start of a word only after such a stop, because that query costs some milliseconds on macOS. It asks for the last word as a whole word before each request, and keeps each answer. A new word costs about 0.16 ms on macOS with English and Spanish (`spelling_eval`).
- For a fix, only the dictionaries of the language of the text count (`Dictionary::knows_typed_word`).
  - The dictionary of a language fits the text when it knows at least half of the last 30 checked words that the writer finished before the caret. With fewer than 3 such words, or when no dictionary fits, the dictionary of each language counts.
  - Thus "reunion" in a Spanish text, which only the English dictionary knows, keeps its fix "reunión".
  - Fewer than 5 typed letters that another language knows as a word are often the start of a longer word of the text, such as "hart" for "harto" in a Spanish text. For them, the dictionary of each language counts.
  - The rule needs no list of words: the dictionaries of the languages of the user tell the language of the text.
  - Measured with Gemma 4 E4B (CUDA, Windows en-US and Hunspell es-CL, long, word choice medium): the rule gave 7 more right fixes in `typo_kinds` (35 to 42 of 52, none lost). It gave no wrong fix in 2341 places of `varieties` and 463 of `chat` with a request after each character. The other sets did not change.
  - Without the minimum of 5 letters, it gave 9 more right fixes, but also 2 wrong fixes of the start of a word: "coo" to "contratado" and "hart" to "hasta".

## Where a suggestion stops

### Text after the caret

- A suggestion also stops where it writes the text after the caret on its line again (`SHARED_UNITS`: from a word, or from the closing punctuation that starts that line, the words agree in at least 2 words and signs in a row, or up to the end of the shorter).

### Name of the writer

- A suggestion stops before the name of the writer at its end when the name signs off a message on the line of the text (`without_sign_off`). Then the end of a sentence comes just before the name ("…your calendar. Thanks! Sara"). Or the first one or two words of a sentence and a comma come before it, after the end of an earlier sentence ("…your calendar. Thanks, Sara").
- A writer signs on a line of their own. Thus the name stays at the start of its line, after a sign-off at the start of the line ("Best regards, Sara") and inside a sentence ("I am Sara Novak."). A name that the user started to type also stays.
- The name is the first name, the last name or the full name of the setting, in any case. Only the signs tell where a sentence ends: a period, "!", "?", "…", "。", "！" or "？", before any closing quotes and brackets. A period after one letter or after a number ends no sentence, because it ends an initial ("J. Novak") or a list number. Thus the rule is the same for each language.
- With Gemma 4 E4B (CUDA, long, without the sentence stop, word replay), the rule made each leak case of `leaks.jsonl` pass (143 of 143 in place of 141). It helped 2 cases and harmed none of `new_line`, `recall`, `conversation`, `chat`, `tuning`, `mid_line` and the variety sets, and it removed up to 32 wrong words in a set.

### Note after a sentence

- A suggestion stops before a bracket that opens a note after the end of a sentence inside the suggestion (`before_note`), as in "Thanks. (This is the part that I…". Such a note is about the text, often a translation, and it shows mostly when a long suggestion without the sentence stop goes on after the end of a sentence.
- The end of a sentence is the same as for the name of the writer, and the letters before the caret count for it. Thus "seve" and "n. (This is…" stop after "n.", but "5 p.m. (Pacific time)" and "step 2. (the form)" keep their brackets.
- A real aside after the end of a sentence also goes, but the sentence before it stays.
- A bracket just after the end of a sentence before the caret hides the whole suggestion (see "Suggestions that do not show").
- Measured with Gemma 4 E4B (Metal, long, without the sentence stop): on the 28 `note-` cases of `leaks.jsonl` (`--session-start first-key`, character replay), the cut removed the note in the 2 cases that showed one (9 places, 370 to 379 of 379 without a leak) and harmed none.
- In 60 cases of `tuning` (word replay), it cut one note, a translation after a Spanish chat message, and changed no score. In `chat`, no suggestion had such a bracket. It also cut the translations after 2 Spanish prompts to an agent in the `clip-mid-` cases.

### Closing quote at the end

- A suggestion loses a closing quote ” at its end when no opening quote “ comes before it on the line of the caret (`without_lone_closing_quote`). The preface puts its texts in these quotes, so the model can close the typed text as a text of the preface.
- A quote that the writer opened on the line stays, as in "the sign says “closed”".
- Measured with Gemma 4 E4B (CUDA, 2026-11-26, `--name "Sara Novak" --notes TEXT --word-choice medium --length long --sentence-stop off`): no case changed in `chat_end` (no closing quote showed there before), `message_times` and `quotes_needed` (word replay). With `--name "Sara Novak" --word-choice medium`, `leaks`, `leaks_adversarial`, `tuning`, `chat`, `recall` and `new_line` did not change. With the first key as the start of the session, the long length without the sentence stop and the character replay, it cut the quote at 5 places of one case of `leaks_adversarial` ("…paste it quickly”"), and changed no match. `terminal` (word replay) did not change.

### Unknown words

- Each suggestion of the model also stops before its first word that no dictionary of the languages of the user knows (`SpellChecker`), unless the prompt, the text after the caret, the word bias of the pool or the earlier writing holds that word in any case (`engine/spelling.rs`).
- A word of the earlier writing counts with any word choice, because the writer wrote it, so it is no made-up word. With the dictionaries of Windows and of Spanish of Chile, this helped 2 cases of the `new_words` set ("porfa" after a text of today with it) and harmed none. The tuning, held-out, today-harm and language-switch sets did not change. Slang that the writer never wrote still stops a suggestion.
- A word with fewer than 3 letters, a digit or a sign inside, or a capital letter inside (code) needs no dictionary.
- A run of text that starts with a sign of a command option or a path (`-`, `.`, `/`, `~` or `*`) needs no dictionary. Examples: `--oneline`, `-Recurse`, `.gitignore`, `/tmp`, `~/.ssh` and `*.json`. The letters after these signs are often in no dictionary, and few words of a sentence start with them.
- Before this rule, the cut hid such options and paths in a terminal. On the `terminal` set of the prompt eval, the rule helped 7 cases and harmed none (p = 0.016). In the tuning, chat, mid_line, typos, varieties, regional and leaks sets, no suggestion changed.
- Other terminal text needs no rule. A path or an option with a sign inside, such as `src/main.rs` or `--format=json`, has no checked word. The text before the caret often holds the names of the commands and the files, so these names count as written. A command name that the text does not hold, such as `tsc` after `npx`, still stops a suggestion.
- The first word counts with the typed letters before it, so an unknown first word hides the suggestion.
- The fixed word of a typo fix also needs the dictionary, so an unknown fixed word hides the fix. Thus a fix never shows only a part of a word, such as "saba" for "sabado", which the Spanish dictionary of macOS knows only as "sábado".
- The engine keeps each answer of the dictionary, because each answer costs a call to the OS.
- The phrases of the earlier writing and the shown suggestions need no dictionary.
- The cut has no switch: it removes only a word that no dictionary knows and that the user never wrote, such as a word that the model made up.

### Languages and dictionaries

- The user picks the languages that they write in the Languages and typos pane and in the second step of the setup (`LanguagesPane`, `SpellingSettings::languages`). Until the user changes the list, it follows the languages of the user in the settings of the OS that the OS has a dictionary for.
- Each language is a tag of `WRITING_LANGUAGES`, with its name in that language.
- The controller opens the dictionary of each language on its own, and the engine keeps them together (`Dictionary`): a word is known when any of them knows it. Thus a change of language between two messages, or in one sentence, needs no other step.
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

- A suggestion of the model does not show when it copies the frame of its prompt (`copies_frame_line`): it holds a run of a frame line, or the line of the caret starts as a frame line starts (its first 5 words, or all the words of a shorter line of at least 3).
- A run is 5 words in a row of a frame line, or all the words of a label of 3 or 4 words, such as "Text on my clipboard:" (`copy_run_words`). Thus "…based on the text on my clipboard" does not show.
- A label ends with a colon. The other short frame lines hold text that the writer can type: the name line with a name of one word ("My name is Sara.") and the age of an earlier text ("(2 days ago)"). Thus "I sent it 2 days ago" shows.
- Measured with Gemma 4 E4B (Metal, character replay): on the 12 `clip-mid-` cases of `leaks.jsonl` with `--history off --screen off`, the rule hid the clipboard label in 3 cases (9 places, 191 to 200 of 228 without a leak) and harmed none.
- The result was the same with the long length without the sentence stop, also with the earlier writing. The rule hid no suggestion in the 34 `start-` and `clip-` cases, in 60 cases of `tuning` (medium and long) and in `chat` (long).
- Other echoes of the clipboard, such as "the environment variables in the clipboard", still showed. The next rule hides them.
- When the preface has a clipboard text, a suggestion that holds the last word of the clipboard label, "clipboard", does not show (`names_the_clipboard`). The exception: a text that the writer wrote or saw holds that word, that is the text before or after the caret, the clipboard text, the screen text, the earlier writing, the notes or the place line. The model then names the label and not the copied text.
- Measured with Gemma 4 E4B (CUDA, 2026-11-26, `--name "Sara Novak" --word-choice medium`):
  - With the first key as the start of the session, the long length without the sentence stop and the character replay, the rule hid the echo at 47 places of 6 `clip-` cases of `leaks.jsonl` (1994 to 2041 of 2058 without a leak) and harmed none.
  - With the defaults, it hid the echo in 2 cases of `leaks.jsonl` (108 to 110 of 110).
  - It harmed the 4 `clipboard-word` cases of `leaks_adversarial.jsonl` where the model wrote the right word "clipboard" (with the replay, at 4 places). These made-up cases need the word while a copied text is in the prompt and no text holds the word, so they show the worst case.
  - `tuning`, `chat`, `recall`, `new_line` and `terminal` (word replay) did not change.
  - With the long length without the sentence stop and the word replay, it helped 10 places of `leaks` (381 to 391 of 394 without a leak). It harmed 1 place of `tuning`, where a Spanish chat message ended with "…que tengo en mi clipboard." after a right first word, and the 4 `clipboard-word` cases. `new_line`, `recall`, `chat`, `mid_line`, `conversation`, `varieties` and `regional` did not change.
- The notes of an example, such as the default notes, are also a frame line (`NotesOrigin`, `docs/core/prompt.md`), because the writer did not write them. Thus 5 words of them in a row, or a line of the caret that starts as they start, do not show, as "I write clear and friendly messages".
- Measured with Gemma 4 E4B (CUDA, 2026-11-26, `--name "Sara Novak" --word-choice medium`): with the English default notes, the rule hid the copy in 2 of the 8 `notes-default` cases of `leaks_adversarial.jsonl` (45 to 47 of 51 without a leak), and with the Spanish default notes also in 2 cases. With the first key as the start of the session, the long length without the sentence stop and the character replay, it helped 1 and 2 places. It harmed none. `tuning` and `varieties` (word replay) with the English default notes did not change.
- The frame lines are the lines of the preface outside its texts (the quoted texts and the screen text), with only the label of the place line (a window title is text that the writer can type), and the heading (`Preface::frame_lines`), so a new label needs no other change.
- The placeholder of the field is also a frame line when the place line shows it (`shown_placeholder`). The field shows it only while it is empty, so the writer does not type it. Thus a line of the caret that starts as the placeholder starts does not show, as "Write a reply" after "W".
- Measured with Gemma 4 E4B (CUDA, 2026-11-26, `--name "Sara Novak" --word-choice medium`): the rule hid the placeholder in 2 `placeholder` cases of `leaks_adversarial.jsonl` and harmed none. With the first key as the start of the session, the long length without the sentence stop and the character replay, it helped 2 places and harmed none. `leaks`, `chat`, `tuning`, `conversation`, `message_times` and `replies`, which have many placeholders such as "Type a message", did not change.
- The labels of the prompt are constants in `prompt.rs`, which the prompt and the filters both use. A test makes sure that each frame line of a preface with all its parts hides in four forms of a copy: at the start of the field, after "Thanks. ", in upper case and after its typed first word. The known exceptions are the ages "(today)" and "(yesterday)", which do not hide after their typed opening bracket, and the name line, which hides only at the start of the field. A new frame line of 1 or 2 words fails the test until it gets a rule or a place in that list.
- The prompt keeps the frame lines when it writes the preface, so a quote sign without its pair in a quoted text (a clipboard text, an earlier text or the notes) does not change them. Thus the labels and the name of the writer stay, and no text of the preface becomes a frame line.
- While the words of a completion come in, the last words wait when more words can make a copy of a frame line (`before_frame_copy_start`). These words start the line of the caret as a frame line starts, or they start a run of a frame line.
- Thus 1 to 4 words of a frame line do not show for a moment before the check can see the copy. A word that ends the run shows the words that waited, and so does the end of the completion. When the words make a copy, the whole suggestion hides.
- A word that waits often shows one word later, because most short words of the frame, such as "the" or "my", can start a run.
- The clipboard word (`names_the_clipboard`) and the colon of a label copy (`copies_frame_label`) get no wait, because no earlier word tells that they come next. While the words of a completion come in, the words before them can show for a moment. When the word or the colon comes, the whole suggestion hides, so the clipboard word never shows.
- A suggestion that adds a colon at the end of a clause with 3 words in a row of a label of the frame also does not show (`copies_frame_label`), as "The text I am typing is:". A label is a frame line that ends with a colon. The clause starts after the last end of a sentence before the colon on its line. Without the colon, 3 such words are often writing, as in "I am typing on my phone."
- The 3 words must hold a word of the topic of a label (`LABEL_TOPIC_WORDS`: typing, screen, clipboard, field, writing, notes). Other runs of a label often end a clause of a text before a colon, as in "The part of my plan:", "The last line of the poem:" and "Here is the text that I am sending:".
- The reason: a change of the prompt made the model write such copies of the labels in new documents ("field on my screen:"). The rule uses only the frame lines that the prompt has.
- Measured with Gemma 4 E4B (CUDA, 2026-11-26, `--name "Sara Novak" --word-choice medium --length long --sentence-stop off --replay word`): no case changed in `leaks`, `new_line`, `recall`, `chat`, `mid_line`, `conversation`, `tuning`, `varieties`, `regional` and `leaks_adversarial`, and in `leaks` and `leaks_adversarial` with the defaults. The rule stays as a guard, because it harms none.
- The name line ("My name is Sara Novak.") hides only when the field starts as it starts, in the first 5 words or all the words of a shorter line (`copies_name_line`). Its runs do not count. In a letter, an email or a chat with an earlier line, the writer types it, as in a letter that starts with "Dear Mr. Lee," and then "My name is Sara Novak" on a later line. In an empty document that starts with "My ", the model copies it.
- Measured with Gemma 4 E4B (CUDA, 2026-11-26, `--name "Sara Novak" --word-choice medium`): before, the frame check hid the right suggestion in 5 of the 9 `self-intro` cases of `leaks_adversarial.jsonl`. The change helped those 5 and harmed none. With the first key as the start of the session, the long length without the sentence stop and the character replay, it helped 81 places of 6 `self-intro` cases and harmed 1 place: a prompt to an AI chat page that ends with the name line after the end of a sentence ("…arod. My name is Sara Novak."), which the run of the name line hid before. The `label-my-` cases of `leaks.jsonl` (an empty document with "My ") still hide the name line, and `leaks`, `recall` and `new_line` did not change.
- A suggestion that holds the age of an earlier text in brackets, in any case, also does not show (`holds_age_frame`): "(today)", "(yesterday)" or "(4 days ago)", as the preface writes it before each earlier text (`is_age_label`). Without the brackets, the age is often writing, as in "I sent it 4 days ago". The check does not look across the caret, so a copy after a typed opening bracket still shows.
- Measured with Gemma 4 E4B (CUDA, 2026-11-26, `--name "Sara Novak" --word-choice medium`): the rule hid "(today)" in 2 `newline-after-label` cases of `leaks_adversarial.jsonl` (46 to 48 of 51 without a leak), and at 2 places with the first key as the start of the session, the long length without the sentence stop and the character replay. It harmed none. `leaks`, `tuning`, `chat`, `recall`, `new_line` and `terminal` (word replay) did not change.
- A rule for the frame of the place line did not ship: a suggestion with "Format: " and the name of a format, or with a part after another part ("; Title: "). No completion of these sets and of the sets of the rule before held such a frame, and the rule changed no case.
- A suggestion that opens a bracket just after the end of a sentence (`opens_note_after_sentence`) also does not show.
- A suggestion that makes a word of the name of the writer the second word of the text after a first word that is not a part of that name (`greets_the_writer`, as after "Querida ") also does not show. A sign-off and a field that starts with the name keep the name.
