---
paths:
  - "crates/typealike_core/locales/**"
  - "crates/typealike_core/src/localization.rs"
  - "crates/typealike_core/src/settings_window.rs"
  - "crates/typealike_core/src/settings_window/**"
  - "crates/typealike_core/src/app.rs"
  - "crates/typealike_core/src/app/**"
  - "crates/typealike_core/src/test_support/writing_rules.rs"
  - "crates/typealike_macos/src/labels.rs"
---
# UI text

What each text that the user sees must say. The users are not technical (`PRODUCT.md`, "Users"). The grammar and the words come from `ste100.md`, and the terms from `PRODUCT.md`, "Terms". The casing and the menu conventions of each OS are in `windows-ui.md` and `macos-ui.md`.
Sources (checked 2026-10): Microsoft "Writing style" (learn.microsoft.com/windows/apps/design/style/writing-style), Apple HIG "Writing" and "Generative AI", NN/g "Error-message guidelines", Google PAIR "Mental models" and "Explainability + Trust", Microsoft "Guidelines for Human-AI Interaction", Project Fluent "Good practices for developers".

## Voice
- The voice of `PRODUCT.md` wins over the voice of a platform guide. Microsoft asks for contractions and "we". TypeAlike uses no contractions, and says "TypeAlike", not "we" or "I".
- Read each text aloud. Rewrite a text that sounds like a machine: "Suggestions are off while the computer saves energy.", not "The computer saves energy now, so suggestions are off."

## Plain words for the engine
- Tell the gain for the user, not the technology: "A shorter suggestion shows sooner."
- Do not show the words of the table "Not in the UI" of `PRODUCT.md`. Use its UI text.
- A technical word can show only in a place for experts (the Hugging Face search), or in parentheses after a plain word: "Graphics card (Vulkan)".

## Labels
- A label names a thing or an action. No period at the end.
- A switch label tells what is on: "Fix typos". Do not write a negative switch label.
- A button starts with a verb and names the result: "Download", "Delete earlier writing". Do not use "OK", "Yes", "Submit" or "Click here".
- A menu entry that changes a state names the action that it does: "Pause suggestions".

## Descriptions
- Tell what happens when the setting is on. Users can guess the off state.
- Then tell the cost, if there is one: "The first suggestion then shows sooner, but this uses more power."
- Do not repeat the label. Each sentence adds a fact. Keep the description short, because users scan.

## Errors
- Tell what failed, then what the user can do: "TypeAlike cannot connect to Hugging Face. Check your internet connection."
- When TypeAlike can do the fix, give a button that does it. Do not tell the path to a setting.
- Do not blame the user. No "Oops" and no error codes. The code goes to the log.
- Keep the text that the user typed.

## States
- Loading: name the operation. Show the progress when the wait is long. Never "Processing…".
- Empty: tell why the list is empty, and give the next action as a button.
- Stopped: tell why suggestions are off and how they start again.

## Expectations
- Tell what a suggestion can do, and that it can be wrong. Use the facts of `PRODUCT.md`, "What suggestions can do". Do not promise a result.
- Next to each switch that keeps data, tell what TypeAlike keeps, where it keeps it, and how to delete it.
- A privacy text states a fact of `PRODUCT.md`, "Positioning". Do not add a claim.
- Show a feature where the user needs it, not in a long tour at the start.

## Translation
- Each text is one Fluent message: a full sentence with named arguments. Do not join messages, and do not give a message as an argument of another message.
- Give each UI case its own message. Use a select expression only for grammar, such as a plural.
- A comment above each group tells the translator what each argument holds.
- ASD-STE100 applies to English only. A translation keeps the meaning of the English text, uses sentence case and the terms of its language in `PRODUCT.md`, and reads as a native UI text, not as a word-for-word copy of English.
- The form of address: Spanish "tú", Brazilian Portuguese "você" (with "o TypeAlike"), French "vous", German "du", Italian "tu".
- A text about the user stays neutral in gender ("Sou estudante", "Je fais des études", "Lehrkräfte").
- A name of the OS, such as a permission, a pane of System Settings or the notification area, is the name that the OS shows in that language.
- The quotes and the spaces follow the language: “ ” in Portuguese and Italian, « » in Spanish, « » with a no-break space inside in French, „ “ in German. French also puts a no-break space before %, a unit and "minutes".
- Each row must hold a translated text that is longer than the English text (`DESIGN.md`, "Layout").
- A number with a fraction goes into its message as `MessageArgument::Decimal`, so that it gets the decimal separator of the language. A unit, such as "MB", belongs in the message.
- Title case is a rule of English. macOS gives title case only to a language with `UiLanguage::has_title_case`.

## Check
- `test_support/writing_rules.rs` checks the rules that a test can check. Add each new rule of this kind there, test first (`tdd.md`).
- The test also rejects the code words of the table "Not in the UI" (`CODE_WORDS`). A quantization name such as Q4_K_M can show only in the Hugging Face search.
- Tests make sure that each language has each message of English, with the same arguments.
- Use `/impeccable clarify` to review a full flow, not one string at a time.
