---
name: prompt-eval
description: Measure a change to the suggestions of TypeAlike with the prompt_eval example of typealike_llm (word scores, log probability, helped and harmed cases, sign test), or the time of the first key after a model load with the first_key example. Use it for a change to the prompt (prompt.rs), the screen text, the clipboard, the earlier writing, the word bias, the typo rule, the decode or the filters of a suggestion, and to pick the case set and the flags.
---

# Prompt eval

The examples are in `crates/typealike_llm/examples/`. The case sets are in `crates/typealike_llm/examples/prompt_eval/cases/`, and the baselines in `crates/typealike_llm/examples/prompt_eval/baselines/`.

## Steps

1. Pick the case sets for the change (see "Case sets").
2. Run each set with `--baseline` and the flags that the set needs.
3. Read the helped and the harmed cases, the sign test and the change of the log probability.
4. To read the prompt of a case, run again with `--prompts FILE`.

## Command

```sh
cargo run --release -p typealike_llm --example prompt_eval -- MODEL.gguf crates/typealike_llm/examples/prompt_eval/cases/tuning.jsonl --baseline crates/typealike_llm/examples/prompt_eval/baselines/<model>.tuning.jsonl
```

## Report

- It prints the word scores, the median time, the cases that a change helped or harmed, and the mean log probability of the next word with its change.
- The log probability moves for small changes that leave the word scores the same.
- The median time is the time on the first key, when the model did not decode the prompt before.
- The warm time is the time after the model decoded the prompt when the input settled.
- The median time and the warm time also show the 95th percentile (p95) and the most time (max).
- It also prints the median tokens of a prompt, the time for each token, and the median time by the tokens of a prompt (1 to 128, 129 to 256, 257 to 512, 513 or more).
- It also prints the right and the wrong words that show, a table for a minimum probability of the first word, and a table for each cut of the tail.
- With `--baseline`, the report also gives the p value of an exact two-sided sign test of the helped and the harmed cases (`sign test`), and the paired 95% interval of the change of the log probability. The unchanged cases do not count. For example, 1 helped and 7 harmed cases give p = 0.070, which is not significant at 5 percent.

## Flags

### Writer

- `--notes TEXT` sets the notes about the writer, and `--name TEXT` the name of the writer.

### Session

- `--session-start last-key|first-key` sets the key that starts the session of a case (default: last-key).
- With `last-key`, the field holds the text before the last key when the session starts. Thus the examples of the earlier writing match all of that text.
- With `first-key`, the field opens without that text, and the session starts at its first character, as in a new message. The eval then types the text: the screen read answers the first key, and the input settles after each line break and after the space after the end of a sentence. A newer key replaces the request of each key, so the model runs only at the last key.
- Run the sets of the earlier writing with both values for a change to the examples. The gap between them shows what the examples lose when the user types the whole text in one session.

### Backend and speed

- `--backend cpu|vulkan|cuda|metal` sets the backend (default: Metal on macOS, else the CPU).
- `--cpu-cores 4,6` gives the performance and efficiency cores, so that the CPU backend uses the threads of the app on Windows.
- `--batch TOKENS` sets the most tokens that one decode of a prompt takes on any backend (default: the batch of the backend, as in the app). Use it to measure another batch, and compare the median time of each prompt size.
- `--drafter on|off` runs the model with or without its drafter (default: on, as in the app). As in the models folder of the app, the folder of the model and its parent folder name the repository (for example `unsloth/gemma-4-E4B-it-qat-GGUF/`), and the drafter file must be in the folder of the model. Run the same cases with each value and `--save`: each suggestion must stay the same, and only the times change.
- `--preparation on-focus|first-key` sets when the app prepares the prompt (default: on-focus, as in the app). `first-key` is the switch "Prepare suggestions early" off. On the CPU, the screen text then gets 300 estimated tokens, not 500. With `first-key`, the warm time is mostly the time of a first key, because the app prepares no prompt when the input settles, except in a place where the earlier writing has a text.
- `TYPEALIKE_LOG=typealike_llm=debug` logs the guesses of the drafter for each completion to stderr (`guesses`, `right_guesses`).

### Earlier writing

- `--word-choice off|light|medium|strong` sets the word bias (default: off).
- `--word-bias off` turns off only the word bias, and keeps the next-word bias and the phrases.
- `--shared-history on|off` gives each app of a case the scope "All apps" (default) or "Only this app".
- `--reply-words on|off` turns "Learn from the messages that you answer" on or off (default: on).

### Length and stops

- `--tail-cut off` turns off the cut of the app: use it to choose a cut.
- `--length very_short|short|medium|long|very_long` sets the suggestion length, with its token limit and its cut (default: medium). The CPU backend allows at most short, so measure a longer length on a GPU backend.
- `--sentence-stop on|off` stops each suggestion at the end of a sentence (default: on, as in the app).

### Replay

- `--replay word|character` also asks for a suggestion at the start of each word or after each character of `next` (default: off).
- The input always follows `next`, also after a wrong suggestion.
- The report then also gives the score of each replayed text.
- Use `character` for a change to the token healing, because each case has only one caret inside a word.

### Typos and dictionary

- `--typo-rule on|off` sets the rule for typos (default: on, as in the app).
- The report then gives a table for each cut of the typed letters: the typo cases that it catches and the other cases that it hides. Run it with `--typo-rule off` to choose a cut.
- `--dictionary FILE` gives the app the answers of the dictionary of the OS: each line has a text, a tab, and `word`, `start` or `none`. A text that the file does not hold is a known word and no start of a word. Without it, the app of the eval has no dictionary.
- `--dictionary macos` and `--dictionary windows` give the live dictionary of that OS, for the preferred languages of the user. Use `windows` on a computer with only English to measure Spanish cases with a dictionary that does not fit them.
- `--dictionary windows:es-CL,en-US` gives the dictionaries of Windows for those languages. It fails when Windows has no dictionary for any of them.
- `--dictionary hunspell:PATH` gives the Hunspell dictionary in `PATH.aff` and `PATH.dic` (UTF-8, for example from `wooorm/dictionaries`).
- A `+` joins live dictionaries, which then know a word when any of them knows it: `windows+hunspell:PATH`.
- The app gets each part as the dictionary of one language, as the app opens the dictionary of each language on its own. Thus give each language its own part, as in `windows:en-US+hunspell:PATH`, for a change that uses the language of a text.

## Spelling eval

```sh
cargo run --release -p typealike_llm --example spelling_eval -- --dictionary windows --dictionary windows+hunspell:PATH --words tuning=crates/typealike_llm/examples/prompt_eval/cases/tuning.jsonl --words chat=chat.txt
```

- It needs no model. Each `--dictionary` takes the value of `--dictionary` of `prompt_eval`.
- For each dictionary and each set of words, it gives the part of the words that the dictionary knows, and the most frequent unknown words, which the cut removes although the writer uses them.
- It also gives the part of the made-up words that the dictionary knows (the cut keeps them), the time to read the dictionary, and the time of a check.
- A `.jsonl` file of cases gives a set for each language of the case ids. Another file gives one set.

### Sections of the prompt

- `--screen on|off`, `--clipboard on|off` and `--history on|off` turn the screen text, the clipboard and the earlier writing on or off in the settings, as the switches of the app do (default: on).
- With an off value, the prompt has no such section, and the `words` of a case give no word bias.
- To measure the gain of one section, run the same cases with that switch off, and compare the run with the run with all on (`--baseline`).

### Prompts

- `--prompts FILE` writes the prompt of each case. Use it to read the examples of a case.

## Case sets

- `cases/tuning.jsonl`: the set of the command above.
- `cases/chat.jsonl` has one or two characters in chats, emails and documents. Run it also with `--notes`.
- `cases/new_line.jsonl` has cases that end with Enter (a case with a line break at the end of `before`, or with `before_enter`). Run it with `--name "Sara Novak"`.
- `cases/mid_line.jsonl` has cases with text after the caret on its line. There `next` is all the text up to that text, and each word of a suggestion after `next` is wrong. Run it for a change to the text after the caret or to the filters of a suggestion.
- `cases/recall.jsonl` has cases in English and Spanish with a fact on the screen or on the clipboard.
  - In the `use` cases, the next word needs that fact (a name, a time, a place, a code or a number).
  - In the `ignore` cases, an unrelated screen line or copy holds a fact that does not belong in the text.
  - Some chats show the messages of the writer with her name, so run it with `--name "Sara Novak"`.
  - Run it for a change to the screen text, the clipboard or the name of the writer.
- `cases/leaks.jsonl` has leak cases in a new TextEdit document.
  - The `leaks` of a case are texts that no suggestion must show. A leak case matches when its suggestion holds none of them, and its words do not count.
  - The report gives them in the line `no leak:`.
  - Run it with `--name "Sara Novak"` and `--word-choice medium` for a change to the prompt or to the filters of a suggestion.
- `cases/typos.jsonl`: a case matches when no suggestion finishes the word with the typo.
  - Each typo case has `fixed`, the text before the caret without the typo, and the report counts the fixes that write the meant word (`typo fixes`) and the fixes of words without a typo (`other fixes`).
  - A change to the prompt or to the context must show a gain here before it ships.
- `cases/typo_kinds.jsonl` has typo cases in English and Spanish, most with the caret just after the whole word with the typo. Its groups: `swap` (two letters in the wrong order), `drop` (a dropped letter), `accent` (a missing accent, also on the first letter and inside the word) and `other` (a word that the dictionary of the other language knows, such as "reunion" in a Spanish text).
  - Run it with the dictionaries of the user, as `varieties.jsonl`, for a change to the typo rule or to the fix.
- `cases/varieties.jsonl` has the same kinds of cases in English (`en-`), standard Spanish without regional words (`es-`) and Chilean Spanish (`cl-`): common phrases and idioms, slang, voseo ("estái", "tenís") and Chilean office phrases ("junto con saludar").
  - The kind follows the language in the id: `chat`, `email`, `doc`, `form`, `word` (the caret inside a word of an idiom or of slang), `mem` (the earlier writing holds the next word) and `typo`.
  - The report has no score for each language. Save the run with `--save`, then split it by the prefix of the ids.
  - Run it with `--word-choice medium --replay word` and the dictionaries of the user, for example `--dictionary windows:en-US+hunspell:PATH` with the downloaded `es-CL` dictionary, for a change to the dictionary cut, the typo rule or the length of the suggestions.
  - Run it also without `--dictionary`: the cases that match only without it show the slang that the cut of unknown words hides.
- `cases/regional.jsonl` has 165 cases in 8 regional varieties: British (`engb-`) and Australian English (`enau-`), the Spanish of Mexico (`esmx-`), Argentina (`esar-`) and Spain (`eses-`), the Portuguese of Portugal (`ptpt-`), the French of Quebec (`frca-`) and the German of Switzerland (`dech-`).
  - Most cases need a word of the region, such as "flat", "alberca", "courriel" or "Grüsse". The `ctl` cases need a word that all regions of the language use, so they show the harm of a change.
  - Run it without `--name` and `--dictionary` for a change that gives the model the region of the writer. Save the run with `--save`, then compare the score of each prefix.
  - A line of the prompt with the region of each spelling language helped 51 cases and harmed 22 here. It did not ship, because for an en-GB writer it made more leaks and harmed the general sets.
- `cases/terminal.jsonl` has 50 command lines of PowerShell and bash in Windows Terminal. In each case, `next` holds options (`opt-`), options with a value (`value-`), paths and file names (`path-`) or subcommands (`sub-`). In the `word-` cases, the caret is inside an option or a path.
  - Run it with `--word-choice medium --replay word` and the dictionaries of the user for a change to the cut of unknown words or to the suggestions in a terminal.
  - Run it also without `--dictionary`: the cases that match only without it show the terminal text that the cut hides.
  - The rule that a run that starts with `-`, `.`, `/`, `~` or `*` needs no dictionary helped 7 cases here and harmed none.
- `cases/left_out.jsonl` has cases from the stores of the writers, each with one text of its store left out of the history (`left_out`). Run it with each `--word-choice` for a change to the word bias, and compare it with the run with `--word-choice off`.
- `cases/repeats.jsonl` has the stores `writers/NAME_repeats.jsonl`, where the writer sent 3 messages 3 or 4 times, and one of them one more time with a typo. Run it with `--word-choice medium` and `off` for a change to the examples or to the word bias.
- `cases/drift.jsonl` has the stores `writers/NAME_drift.jsonl`, where the writer had a long conversation about one topic two days before, and writes about another topic in the same app today.
  - The `phrase`, `word` and `example` cases are on the topic of today, and the `same_day` cases continue the long conversation on its own day.
  - Run it with `--word-choice medium`, and also with `--screen off`, for a change to the earlier writing.
- `cases/replies.jsonl` has the stores `writers/NAME_replies.jsonl`, where each chat and email text has the message that it answers (`reply_to`).
  - Its groups: `help` (a message like an earlier one, and the answer of that time, which shares no rare word with the message), `harm` (a message like an earlier one, and a new answer), `none` (a new message) and `elsewhere` (a notes app).
  - Run it with `--reply-words on` and `off` for a change to the words of the answered messages.
- `cases/register.jsonl` has cases from the chat and email texts of the stores, each with its text left out, where the next word is a word that the writer uses mostly in one kind of place (80 percent of its counts in chats or in emails).
  - In the `own` cases, the word is of the kind of the place of the case (for example a sign-off of an email), and in the `cross` cases, of the other kind.
  - Run it with `--word-choice medium` for a change to how the texts of one app help the suggestions in another app.
- `cases/context.jsonl` has the store `writers/nora_context.jsonl`: chats with 4 contacts and 3 mail threads that talk about the same events, each text with its app and its window title (its typing context).
  - Its groups: `greet` (the name that the writer uses for each person), `close` (sign-offs), `habit` (words of one contact) and `control` (the right word is only in a text of another conversation).
  - Run it with `--word-choice medium`, also with `--backend cpu --length short` (one example), for a change to the typing context or to the pick of the examples.
- `cases/new_words.jsonl` has 0, 1 or 2 texts of today (`c0`, `c1`, `c2`) with a word that the stores of the writers never hold, and then that word next (`next`), after its typed start (`start`) or in another app (`other`).
  - Run it with `--word-choice medium --dictionary crates/typealike_llm/examples/prompt_eval/cases/new_words.dictionary.tsv`, the answers of the macOS dictionary for these words.
- `cases/today_harm.jsonl` has the cases of `stores_tuning`, `stores_held_out`, `replies` and `drift`, each with one text of today that holds an invented word which does not come next. Run it with `new_words.jsonl` for a change that uses the texts of today: a line with the new word of today helped 9 cases of `new_words` and harmed none, but here it helped 3 and harmed 18, because the model wrote the invented word in place of the right word.
- `cases/conversation.jsonl` has chats and emails of real size: up to 866 estimated tokens of screen text, bubble chats with the messages of the writer on the right, app lines such as times and "Today", and quoted emails after the caret.
  - Its groups: `far` (the fact is more than 300 tokens above the caret), `near` (the fact is near the caret), `turns` (the next word depends on who wrote each message), `answer` (the fact is in the first line of the last message), `live` (older texts of the same app pull another word), `header` (WhatsApp with the contact only in the chat header, store `writers/sara_conversation.jsonl`) and `email` (the fact is more than 100 tokens into the quoted message after the caret).
  - Run it with `--name "Sara Novak" --word-choice medium`, also with `--backend cpu --length short` for a change to the budget of the screen text.
  - `cases/conversation_gen.py` writes the set again, and `--check` makes sure of the distances of each group.
- `cases/scrolled.jsonl` has chats in a laptop window with a short chat area, where the app read the screen before the last read.
  - The optional field `earlier_screens` of a case holds those reads of the same window, oldest first, in the format of `screen`. The input settles once for each of them before it settles for `screen`, and a scroll follows each of them, so that the session reads the screen again.
  - The reads are 20 seconds apart, or `screen_gap_seconds` apart when the case gives it. A case without `earlier_screens` runs as before.
  - Its groups: `scrolled` (an earlier read showed the fact, and the last read shows the bottom of the chat without it, with at least 2 long lines in common), `switched` (the earlier read is another chat of the same app with a conflicting fact), `stale` (as `scrolled`, but 15 minutes before the last read) and `visible` (control, the fact is in the last read).
  - Run it with `--name "Sara Novak"` on a graphics card for a change to the screen reads or to the chat lines that the user scrolled past.
  - `cases/scrolled_gen.py` writes the set again, and `--check` makes sure of the reads of each group.
- `cases/message_times.jsonl` has the Telegram and WhatsApp chats of `conversation.jsonl` and `scrolled.jsonl`, where text recognition joins the time of a message to the end of its last line ("see you there 9:57"), as macOS does when a small gap divides them.
  - Its groups: `joined` (the same caret and next word, for the harm of a change) and `end` (the caret after the whole message, a leak case whose leaks are the joined times).
  - Run it with `--name "Sara Novak" --word-choice medium` for a change to the screen lines of a chat or to the times of messages.
  - `cases/message_times_gen.py` writes the set again from the two source sets, and `--check` makes sure that the set is the same as the sets give.
- `cases/window_chrome.jsonl` has the cases of `recall.jsonl` and `tuning.jsonl` with screen text in a browser, Word or Notion, with made-up lines of the app around the page, as an OCR of the whole window reads them.
  - Its groups: `browser` (the tabs, the address and 10 items of the bookmarks bar), `word` (the title bar and the ribbon) and `notion` (the sidebar and the bar above the page).
  - To measure the cost of these lines, write the same cases without them with `cases/window_chrome_gen.py --clean FILE`, run FILE with `--save`, then run `window_chrome.jsonl` with `--baseline`. Run both with `--name "Sara Novak"`.
  - `cases/window_chrome_gen.py` writes the set again, and `--check` makes sure that the set is the same as the source sets give.

### Stores of the writers

- The `shared` group of the stores files has cases where the next word comes from another app (`help`), where another app pushes a wrong word (`harm`), and where the place and another app give different next words (`priority`). Run them with `--word-choice medium` for a change to the scope of the history.
- Each text of the stores has the time when the writer wrote it (`written_at`, 4 texts a day, the last day 4 days before 2026-10-28 15:00 UTC).
- A case can give its time (`now`), and its history then leaves out the texts written later.
- A text of a store or of `history` can give the name of its app (`app`) and its window title (`window_title`), which give its typing context. Only the texts of `context.jsonl` have them.
- Without `now`, a case is at the time of the newest text of its history.
- A case with `"has_unreadable_text": true` has a field that hides its text when `before` and `after` are empty, as the editor of VS Code does while its screen reader mode is off. Its screen lines inside the field are then its text.

## First key

```sh
cargo run --release -p typealike_llm --features vulkan,cuda --example first_key -- MODEL.gguf --backend vulkan
```

- It loads the model 6 times (`--loads N`), and times the first completion after each load, a later key, and a whole other prompt after the first completion.
- The first request of a load goes with the load request (no warm-up) or after the answer of the load (with the warm-up), in turns.
- `--first queued|after-load` sets the way of the first load, which also starts the backend as at the start of the app. Run it several times with each value.
- `--drafter on|off` loads the model with or without its drafter, which `prompt_eval` finds in the same way (default: on).
- The NVIDIA Vulkan driver keeps the compiled shaders for each program, so the first run of a new build compiles them for some seconds.
- Close TypeAlike before a run.
