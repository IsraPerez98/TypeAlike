# Earlier writing

The earlier writing of the user (`history` in `typealike_core`, the file in `typealike_history`). The examples, the words of the answered messages and the word tables all come from it.

## Texts

- The store keeps the texts of finished sessions without their secrets and without the text that the user did not write.
- The store keeps no text that the user deleted with a shortcut, or with a selection and a key, at the end of a session. It keeps only the text that the field still holds after the key that ended the session (`docs/core/typing.md`, Session).
- The store keeps these secrets as "[redacted]" (`redaction`):
  - private keys
  - the passwords of web addresses (`postgres://user:password@host`)
  - the credentials after "Authorization:"
  - the value after the name of a secret and ":" or "=", as in `DB_PASSWORD=value`, `password: value` and `token = value`. A name of a secret is `password`, `passwd`, `pwd`, `pass`, `secret`, `token` or `apikey`, or a longer name whose last part after "_" or "-" is one of them or `key`, such as `API_KEY`. The value is the next word on the same line, or the text between quotes.
  - the value after the name of a secret and "is", as in "my password is hunter2". This value must have a character that is not a letter, a dash or an apostrophe, because "the password is wrong" names no secret.
  - the passwords of database commands: the value next to `-p` after `mysql` or `mariadb`, the word after `-a` after `redis-cli`, and the last word of a Redis `AUTH` with one or two words after it
  - JSON web tokens: three parts or more between dots, and the first part starts with `eyJ`
  - API tokens with a known start (`sk-`, `ghp_`, `AKIA` and others)
  - runs of 20 or more letters and digits
  - card numbers
- A run whose parts between dashes and underscores each have only letters or only digits is a name, such as the file name "Informe_Gestion_2024", not a secret.
- The store keeps no text of a field that the platform cannot find (`SessionField::can_keep_text`). The keys can then go to any other place, for example a remote desktop, or an app that draws its own text, such as Zed.
- The store also keeps no text of a terminal panel in an app, such as the terminal of VS Code. A terminal can ask for a password that it does not show, and the keys still get to TypeAlike.
- The place of a text is its web page in any browser, else its app.
- A text that the user typed in full is a new text, also when an older text is the same, so a repeated message counts each time.
- Each text keeps the time when the user started to write it (`HistoryEntry::written_at`, from `Desktop::now`).
- Each text also keeps its typing context (`HistoryEntry::typing_context`, `history::typing_context`): the window title of its session in lowercase, without the parts that name the app, counts such as "(3)", reply marks such as "Re:" and file extensions. For example, "#team-web - Acme - Slack" gives "#team-web | acme", and "Re: Garden party" gives "garden party". Thus it names the contact, the channel, the thread or the document. The typing context also has no secrets, because a title can show a web address with a key, as in Postman. A text of an older file has none.
- The store keeps at most 1,000,000 characters of text and at most 10,000 texts. Above either limit, the oldest texts of the store go, in any place.
- The core also has the `WritingHistory` and `KeyVault` traits.

## Pools

- Each app of the Personalization pane has a scope: Off, Only this app or All apps.
- A password manager and a terminal app (`known_apps`: Terminal, Ghostty, Termius and others) start with Off. The other apps start with All apps.
- A session uses only the texts of its pool: the texts of its own app with "Only this app", else the texts of all the apps with "All apps".
- A text never replaces or continues a text of another pool.

## Examples of the prompt

- The examples are the texts of the same place that match the topic of the field: one on the CPU, two on a GPU.
- A session picks its examples at its first key, often in an empty field, and again at the answer of its first screen read.
- When the input settles, the session picks its examples again with the text of the field. It does this when the text before the caret reaches 16 estimated tokens, about 8 words (`EXAMPLE_STEP_TOKENS`), and each time that text doubles (`Engine::pick_examples_again`).
  - Thus the examples follow the topic of a long text that the user types in one session.
  - A new pick decodes the prompt again from the examples on. The doubling keeps these decodes rare: a text of 1000 tokens gets 6 picks.
  - When the new pick changes the prompt and TypeAlike prepares prompts before a key ("Prepare suggestions early" on, or a place with earlier writing), the settle prepares the new prompt. Thus the next key does not wait for that decode.
  - A new pick also takes the newest screen text.
  - Before this rule, a new email without screen text or a window title got no examples for its whole session. On 298 cases of the `register` and `stores_*` sets with a session that starts at the first character (`--session-start first-key`, Gemma 4 E4B, Metal), the rule helped 54 cases and harmed 8: 98 matches before it, 144 with it. With the whole text in the field at the first key, these sets match 167 cases. On the CPU (one example), it helped 5 of the 38 cases of `stores_held_out` and harmed 1.
- A word of the window title matches only a text of the same typing context or of none. A text to a contact seldom holds the name of the contact, but a text about that contact to another person does. Thus in a chat with "Mamá", a text to another contact that names "mamá" is no example for that word.
- When no text of the place matches, the newest text of the typing context of the window is the example.
- On 38 cases of chats and mail threads (Gemma 4 E4B), these two rules helped 3 cases and harmed none with two examples (Metal), and helped 4 and harmed none with one example (CPU). When all the texts of a place have one title, such as "Untitled", the second rule gives the newest text of the place: on 208 cases of the history sets with such a title, it helped 2 cases and harmed none.
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
- It holds each text with its app, its web page, its time, the words of the message that it answers and its typing context.
- It is sealed with ChaCha20-Poly1305.
- The key is a generic password in the login keychain on macOS (service `io.github.israperez98.typealike`, account `history-key`), and `history.key` (locked with DPAPI) next to `history.bin` on Windows.
- To delete both resets the history.
- The read skips a line that is not an entry, for example a line of a newer version, and keeps the other entries.
- When the read cannot use all of `history.bin` (a skipped line, a missing key, another key or a damaged file), the next save first copies the file to `history.bin.backup`.
  - A save after that keeps the copy. A newer copy replaces an older one.
  - "Delete all earlier writing" also deletes `history.bin.backup`.
- Each save writes `history.bin.new`, waits until its bytes are on the disk, then replaces `history.bin` with it. Thus a power loss cannot leave an empty file.
