# Prompt and screen text

What the model reads for each request (`typealike_core`: `prompt`, `place`, `screen`). A change to the strings of the prompt (`crates/typealike_core/src/prompt.rs`) needs a `prompt_eval` run (the `prompt-eval` skill).

## Sections of the prompt

The prompt is in the first person. It has these sections:

- A fixed frame that the user cannot edit.
- The notes about the writer from the Personalization pane.
  - When the settings file has no notes, the default notes in the first person, from the Fluent files in the language of the UI.
  - The menu "Start from an example" (`NotesExample`) puts the notes of an example for a kind of writing into the field, and shows "Your own notes" for notes that are no example.
- The name of the writer.
- The place line.
- The text around the caret.
- The clipboard.
- The screen text.
- A heading just before the text before the caret.

Each section has a budget of estimated tokens. The whole preface has at most 2000 (`PREFACE_TOKENS`), and the text before the caret can use the rest of the context. The sections take the budget in this order, each up to its own cap:

- The frame.
- The notes: 300.
- The name: 20.
- The text after the caret: 300 in an email app or web mail, 100 in other places. In an email reply, the quoted message that the user answers is in the field after the caret. A platform reads at most 900 characters after the caret (`MAX_CHARS_AFTER_CARET`).
- The place line: 40 for the window title, 40 for the name of the field and 40 for its hint.
- The screen text: 800 on a graphics card (`Settings::screen_tokens`). On the CPU, 500 when TypeAlike prepares the prompt before the first key (the switch "Prepare suggestions early" on), else 300. More screen text is too slow to decode on the CPU at the first key. In a chat or an email app, the lines of the last prompt can use 25 percent more ("Order of the lines").
- The last line above the field one more time, only when the whole line fits.
- The earlier writing: 320.
- The clipboard: 170.

The examples of the earlier writing are in `docs/core/earlier-writing.md`. The frame lines (the lines that a suggestion must not copy) are in `docs/core/suggestions.md`, "Suggestions that do not show".

## Text before the caret

- A platform reads at most 9000 characters before the caret (`MAX_CHARS_BEFORE_CARET`). The backend sets how much of that text a session keeps (`Settings::text_limits`).
- On the CPU, a session keeps up to 4000 characters. At 4000, it drops the oldest characters and keeps the last 3000.
- On a graphics card (Vulkan, CUDA, Metal), a session keeps up to 9000 characters, and then the last 7000. Thus a fact further back in a long document or email can reach the prompt.
- The text drops its oldest characters in large steps, so that the start of the prompt stays the same for many keys and the model runner can reuse its decode.
- The text must also fit in the context of the backend (`Backend::context_tokens`) after the preface, with room for the longest suggestion. Else the session keeps the end that fits in three quarters of that room.
- The estimate counts one token for each Chinese or Japanese character. Thus a graphics card keeps about 4500 to 6000 of these characters after a full preface.
- A new prompt of 9000 characters takes about 2 s to decode with Gemma 4 E4B on Metal (M5), and the next keys reuse its start. The CPU keeps less text, because each token of a new prompt takes about 6 ms there.

## Place

- `place` holds the format of known apps and web pages.

## Screen text that the prompt keeps

`screen` decides:

- The scope of the lines (`LineScope`).
- The noise: many signs, or a digit between two lowercase letters, as text recognition makes them. A code of capital letters and digits, such as a booking code, stays.
- The copies: a line with the same text and frame as another line shows once, because the tree of an Electron app gives each line twice.
- The lines without a letter beside the field, such as the line numbers of an editor.
- The times of chat headers and of chat messages.
- The distance to the caret.
- The budget.
- The lines of the last prompt in a chat or an email app.
- The lines above and below the caret.

### Scope of the lines

- In a chat or an email app, the scope is the column of the field: the span of the field from left to right, with each line that reaches into that span, such as a chat message that starts left of the input box.
- A line wholly beside the column there is the list of the other conversations, so it goes and gives no topic words to the examples.
- In other places, a panel beside the field, such as the help of a form, can tell what the field needs, so all the lines of the window stay, and the lines beside the field rank last. An OCR there reads only the content of the field ("Screen reads").

### Times of chat messages

- A chat header loses its time ("Laura Méndez 11:02" gives "Laura Méndez"). A line with only a time goes.
- Some chats show the time of each message at the end of its bubble, as Telegram and WhatsApp do. Text recognition reads that time as a line of its own when a gap divides it from the message, else as the end of the last line of the message.
- In a chat or an email app, a line with only a time shows that the app gives these times. Then a time at the end of another line is the time of its message, and it goes. Thus the model does not copy these times into a suggestion.
- A line keeps the time at its end when a line with only a time is on its row. That line is the time of the message, so the time at the end is a part of the message, as in "see you at 8:40".
- When the last line of a message fills its bubble, the time of the app is on the row below. Thus a message that ends with a time there loses its own time.
- Without a line with only a time, each message keeps its times, because a time is often what a message tells.
- The model can still write a time, for example a time that a message tells.

### Field that hides its text

- A field with unreadable text whose read gives no text hides its text (`Surroundings::hides_field_text`), as the editor of VS Code does while its screen reader mode is off, or a terminal that gives no caret.
- Its lines on the screen are its text, so they stay and count as above the caret. In VS Code, that is the line of the caret.
- A line of such a field that shows a part of the text that the user typed in the session goes, because the prompt has that text already (`FieldContent::shows_typed_text`). The signs at the start of the line do not count, such as the prompt mark of an agent in a terminal ("❯ ").
- When the platform does not know the caret, the last such line is the line of the caret. The lines nearest to the caret fill the budget first.
- The lines of the field below the row of the caret, such as the status lines of an agent, are below the caret. The other lines of the field count as above it, also a line on the row of the caret that does not match the typed text, such as text that the user typed before the session.
- The lines of the field come after the other lines above the caret, so that they lead to the caret. In the terminal of VS Code, the side bar has lines between the last answer of the terminal and the caret row.
- On 12 synthetic prompts to coding agents in a terminal (Gemma 4 E4B), these two rules gave the right next word in 11 cases, against 6 with the whole screen after the caret and 6 with all the lines above the caret. With all the lines above, the model copied the status lines.
- It gives no labels, because its label tells a screen reader how to read the text.
- Its screen read always asks for an OCR, because the tree misses its text.

### Order of the lines

- The lines below come first, then the lines above, then the last line above the field one more time (`LAST_LINE_LABEL`), because that line is often the message that the user answers.
- The text after the caret comes last, next to the heading, so that the model writes text that leads into it.
- In a chat or an email app, the prompt keeps the lines above the field of the last prompt in the field at the start. It keeps them while each of them is still on the screen in the same order. The nearest lines after them come next.
- Thus a new message at the bottom does not change the start of the prompt, and the model decodes only the new text again (`docs/llm.md`, "Cache and context").
- A line counts as the same line when it has the same text, because the lines of a chat move when it scrolls.
- The lines of the last prompt and the new lines can use 25 percent more than the budget of the screen text (`KEPT_LINES_SLACK_PERCENT`). When they need more, the prompt chooses the nearest lines again, as at a first read.
- The prompt also chooses the nearest lines again when the lines of the last prompt leave out a near line. Examples are a new message between them, or an older line that fits after a scroll. A near line is a line that the nearest lines keep before their first line that does not fit.
- The engine forgets the lines of the last prompt when the focus moves to another field or window. In other places, the prompt chooses the nearest lines at each read.

## Screen reads

- A screen read starts at the first key of a session and when the input settles, unless the session has its final screen text.
- It starts only in a field that holds text: a field whose text the platform reads, or a field with unreadable text (`Surroundings::holds_text`). A terminal that shows its text only on the screen gives no field text, but it still gets screen reads.
- A field that gets no suggestions gets no screen reads (`place::is_excluded_field`): a search box, or an address or the subject of an email. Thus TypeAlike does no OCR while the user types an address into the To field of an email.
- A session first uses the stored screen text. The answer of its own read replaces that text one time, when the text changed. A new pick of the examples also takes the newest screen text (`docs/core/earlier-writing.md`, "Examples of the prompt").
- When that answer changes the prompt of the suggestion that runs or shows, and the user pressed no key after its request, TypeAlike asks for the suggestion again with the new prompt. Thus the first suggestion of a reply also uses a chat message that came while the field was idle. When no suggestion runs or shows, TypeAlike asks for none.
- A scroll moves the text on the screen, so the next key of the session reads the screen again, and its answer replaces the text of the session one more time.
- In a text field of a chat or an email app where the screen text is on, the stop of a scroll also reads the screen:
  - A scroll stops 300 ms after its last scroll event (`Timer::ScrollSettle`, `SCROLL_SETTLE_DELAY`). Each stop starts at most one read.
  - The read uses the accessibility tree, and an OCR only when the tree gives too few words, as at a key.
  - Its answer goes into the session, so the next key does not read the screen again. Thus the kept chat lines also get the view that the user scrolled to and read.
- In a chat or an email app, the prompt also shows the chat lines that the user scrolled past (`ConversationLines`):
  - TypeAlike keeps the lines above the caret of each read in memory, never on disk, for 10 minutes after the newest read that showed them.
  - A new read continues the conversation when it shares at least 2 lines of 20 or more characters with the kept lines. The window title does not count, because WhatsApp shows only "WhatsApp" for each chat.
  - A read that shares fewer lines shows another conversation, so the kept lines go. A short line such as "ok" or "Today" shows in many chats.
  - A focus change drops the kept lines, as it drops the screen text. Without a clock, TypeAlike keeps no lines.
  - The kept lines that the newest read does not show go above all its lines, in their old order, as if the chat were taller. They are farther from the caret, so they fill the budget of the screen text after the lines on the screen.
  - After a single read, the prompt shows only the lines of that read. In other places, it shows only the newest read.
- When the lines of the accessibility tree that the prompt can keep hold fewer than 40 words with a letter (`MIN_TREE_WORDS`) and the platform can do an OCR, core asks for an OCR read. The tree of an app that draws its text itself, such as the editor of VS Code, often gives only the labels of the app.
- When the OCR gives no lines, the lines of the tree stay.
- An OCR reads the whole window only in a chat or an email app (`OcrArea::Window`). Elsewhere, and in a browser also for a chat or an email page, it reads the content that holds the field (`OcrArea::Content`, `screen::ocr_area`):
  - The window of a browser also shows its tabs, its address bar and its bookmarks bar, and other apps show a toolbar, a sidebar or a list of other notes beside the content. These lines are noise for the model, and they put private names into the prompt.
  - In a chat or an email app, core keeps the column of the field itself ("Scope of the lines"). The name in the header of a chat or the address of an email can be outside of the content of the field there, for example in a toolbar.
  - The platform finds the content (`docs/macos.md`, "Screen text"). When it finds none, it reads the whole window.
  - Thus the OCR of a native app leaves out the other panes of a split view, such as a sidebar, but a panel inside a web page stays.
- The cost of these lines, measured on `cases/window_chrome.jsonl` (23 cases of `recall` and `tuning` with made-up lines of tabs, an address, 10 bookmarks, a ribbon or a sidebar, Gemma 4 E4B on Metal, 2026-11-08):
  - No case changed its match (0 helped, 0 harmed). The log probability of the next word changed by +0.04 (95% interval −0.11 to +0.18), and 6 suggestions changed their words.
  - The prompts got 49 more tokens (173 to 222), and the median time of the first key went from 267 ms to 379 ms. On the CPU with the short length, it went from 1030 ms to 1477 ms, also with no change of a match.
  - In the Notion cases, a sidebar item became the last line above the field.
- The level of the Copied and screen text pane (fast or accurate) shows only when the OCR of the platform has levels (`OcrLevels`).
- When the user picks a level, the controller tells the shell (`Shell::warm_up_ocr`), so that the platform can load the OCR of that level before its first read.

## Preparation of the prompt

- With the preparation at the first key (the switch "Prepare suggestions early" off), TypeAlike still prepares the prompt when the input settles in a place where the earlier writing has a text.
