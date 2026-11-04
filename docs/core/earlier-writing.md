# Earlier writing

The earlier writing of the user (`history` in `typealike_core`, the file in `typealike_history`). The examples, the words of the answered messages and the word tables all come from it.

## Texts

- The store keeps the texts of finished sessions without their secrets and without the text that the user did not write.
- The place of a text is its web page in any browser, else its app.
- A text that the user typed in full is a new text, also when an older text is the same, so a repeated message counts each time.
- Each text keeps the time when the user started to write it (`HistoryEntry::written_at`, from `Desktop::now`).
- The store keeps at most 1,000,000 characters of text and at most 10,000 texts. Above either limit, the oldest texts of the store go, in any place.
- The core also has the `WritingHistory` and `KeyVault` traits.

## Pools

- Each app of the Personalization pane has a scope: Off, Only this app or All apps.
- A session uses only the texts of its pool: the texts of its own app with "Only this app", else the texts of all the apps with "All apps".
- A text never replaces or continues a text of another pool.

## Examples of the prompt

- The examples are the texts of the same place that match the topic of the field: one on the CPU, two on a GPU.
- In a place with at most 3 texts, its newest texts fill up to two examples.
- The texts of other places of the pool that share at least 3 topic words with the field fill the free examples.
- A place without texts gets the newest text of another place with the same format.
- Among texts that match the same, the text with more copies in its place comes first.
- Each example keeps at most 160 estimated tokens: a longer text keeps its start (its greeting) and its end, with "…" between them.
- All the examples together keep at most 320 estimated tokens (`HISTORY_TOKENS`), so two long examples both keep their greeting and their names. When they do not fit, the older examples keep only their end. On 327 cases with earlier writing (Gemma 4 E4B), 320 instead of 200 tokens helped 9 cases and harmed 1, and the time on the first key went from 296 to 302 ms (median).
- Each example with a time starts with its age ("(today)", "(yesterday)", "(4 days ago)", "(2 weeks ago)"), from the clock of the desktop.
- Each example is in curly quotes, with a line break before its closing quote. A quote just after the text makes the model end the typed text with a quote, which then shows at the end of the suggestion.
- A near copy of a picked example is no example: two texts that differ in at most 2 inserted, deleted or changed characters, or in at most 10 percent of the characters of the shorter text. Thus the spelling that the writer uses most stays.

## Words of the answered messages

`HistoryEntry::reply_to`, `history/reply.rs`:

- In a chat or an email app, each kept text also keeps at most 10 distinctive words of the message that it answers, nearest the reply first, without secrets.
- That message is the lines of the screen text of the session prompt above the field in the column of the field, without chat headers, after the last line that is an earlier text of the user in the same place.
- An earlier text matches the topic of the field with its own words and with these words, so the answer to a similar message is an example also when it shares no word with that message.
- Core keeps these words only while "Learn from the messages that you answer" of the Personalization pane is on (`ReplyWords`, on by default, shown while "Use earlier writing" is on).
- While it is off, the engine drops these words from the earlier writing and saves it.

## Word tables

- The word bias of each pool with its spelling: a word with a count of at least 3, with the full strength at 10.
- The phrases that the writer repeats: when the word choice is on, a word that came after the same 3 words with a count of at least 3, and at least 60 percent of the counts, shows without the model while the line after the caret is empty.
- The next-word bias: at most 8 words that came after the last 3 words before the caret, else the last 2, each with the strength times its share of the counts. The runs with a count of 1 are left out.
- The phrases and the next words come from the texts of the place when these texts have the last words, else from all the texts of the pool.

### Counts

- In these counts, each text of the last 24 hours adds its times, at most 2 (`MAX_COUNT_PER_GROUP`).
- Each earlier UTC day adds the times of all its texts, also at most 2, and the texts without a time count as one day.
- Thus one long conversation of an earlier day is no habit of the writer by itself, but its phrases show for a day, and a new user gets phrases on the first day.
- The engine reads the clock at the start of each session and builds these tables again when they are one hour old (`Engine::follow_clock`).

## File and key

- The file is `history.bin` in the folder of `settings.json`, only while "Use earlier writing" is on.
- It is sealed with ChaCha20-Poly1305.
- The key is a generic password in the login keychain on macOS (service `io.github.israperez98.typealike`, account `history-key`), and `history.key` (locked with DPAPI) next to `history.bin` on Windows.
- To delete both resets the history.
