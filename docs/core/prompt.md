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

Each section has a budget of estimated tokens.

The examples of the earlier writing are in `docs/core/earlier-writing.md`. The frame lines (the lines that a suggestion must not copy) are in `docs/core/suggestions.md`, "Suggestions that do not show".

## Place

- `place` holds the format of known apps and web pages.

## Screen text that the prompt keeps

`screen` decides:

- The scope of the lines (`LineScope`).
- The noise: many signs, or a digit between two lowercase letters, as text recognition makes them. A code of capital letters and digits, such as a booking code, stays.
- The copies: a line with the same text and frame as another line shows once, because the tree of an Electron app gives each line twice.
- The times of chat headers.
- The distance to the caret.
- The budget.
- The lines above and below the caret.

### Scope of the lines

- In a chat or an email app, the scope is the column of the field: the span of the field from left to right, with each line that reaches into that span, such as a chat message that starts left of the input box.
- A line wholly beside the column there is the list of the other conversations, so it goes and gives no topic words to the examples.
- In other places, a panel beside the field, such as the help of a form, can tell what the field needs, so all the lines of the window stay, and the lines beside the field rank last.

### Order of the lines

- The lines below come first, then the lines above, then the last line above the field one more time (`LAST_LINE_LABEL`), because that line is often the message that the user answers.
- The text after the caret comes last, next to the heading, so that the model writes text that leads into it.

## Screen reads

- A screen read starts at the first key of a session and when the input settles, unless the session has its final screen text.
- A session first uses the stored screen text. The answer of its own read replaces that text one time, when the text changed.
- When the lines of the accessibility tree that the prompt can keep hold fewer than 40 words with a letter (`MIN_TREE_WORDS`) and the platform can do an OCR, core asks for an OCR read. The tree of an app that draws its text itself, such as the editor of VS Code, often gives only the labels of the app.
- When the OCR gives no lines, the lines of the tree stay.
- The level of the Copied and screen text pane (fast or accurate) shows only when the OCR of the platform has levels (`OcrLevels`).

## Preparation of the prompt

- With the preparation at the first key (the switch "Prepare suggestions early" off), TypeAlike still prepares the prompt when the input settles in a place where the earlier writing has a text.
