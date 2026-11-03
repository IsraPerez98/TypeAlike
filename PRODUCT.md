# Product

<!-- impeccable:product-schema 1 -->

## Platform

adaptive

## Platform References

TypeAlike is a desktop app for Windows and macOS. It uses a different design language on each OS, so the platform is `adaptive`. But Impeccable's platform references are for phones. For this project, these files replace them:

- Windows: `.claude/rules/windows-ui.md` (Fluent 2, WinUI 3 through windows-reactor, Win32 for the overlay, the badge and the tray).
- macOS: `.claude/rules/macos-ui.md` (Apple Human Interface Guidelines, macOS 26 and later, AppKit).

Do not apply `ios.md` or `android.md`. Their rules for touch targets, tab bars, safe areas, Dynamic Type, Material 3 and the simulator do not apply to a desktop app. Their general rules (follow the system, use the controls of the platform, support both appearances) agree with the two files above.

The settings window, the menus and the overlay get their colors, fonts and materials from the OS. `DESIGN.md` lists the parts that TypeAlike draws itself.

## Users

People who write a lot every day: emails, chats and documents, in many apps. They want to type faster. They are not necessarily technical.

TypeAlike asks the user to select a model and, on Windows, the hardware that runs it. The UI must explain these choices in plain words and must not expect knowledge of language models.

## Product Purpose

TypeAlike gives system-wide inline suggestions. While the user types in any app, a suggestion shows at the caret as light text. Tab types the next part of the suggestion. Any other key works as usual.

It is open source (GPL-3.0-or-later). Windows comes first, and macOS is also supported. Inspired by Cotypist (https://cotypist.app/) and Cotabby (https://github.com/FuJacob/cotabby).

### What suggestions can do

The UI uses these facts to tell the user what to expect (`.claude/rules/ui-text.md`, "Expectations").

- A suggestion is the model's guess. It can be wrong. The user decides with Tab, and any other key works as usual.
- A smaller model is faster and needs less memory, but its suggestions are wrong more often.
- A graphics card runs the model faster than the processor, but the whole model must fit in its memory.
- On the processor, suggestions are short or very short, so that they show sooner.
- Suggestions learn from the earlier writing only while "Use earlier writing" is on.

## Positioning

- The language model runs on the user's computer (llama.cpp, GGUF models, on the CPU or a GPU). The text that the user types never leaves the computer.
- Suggestions learn from the user's earlier writing. TypeAlike keeps it encrypted on the computer, and only while the user allows it.
- TypeAlike does not save screen text, except some words of the messages that the user answers in a chat or an email. It keeps these words with the earlier writing.
- TypeAlike does not save the text of a field that the app marks as a password field.
- It is open source and free.

## Operating Context

TypeAlike is a guest in the user's apps. It runs in the background with a tray icon on Windows and a menu bar extra on macOS. The user sees it in four places:

- The suggestion at the caret, inside the text field of another app. When text follows the caret on its line, the suggestion shows on a small card below the line.
- The badge at the caret. A click on it opens the badge menu: pause, and on/off switches for the current app and website.
- The tray menu or the menu bar menu: pause or resume, settings, quit.
- The settings window. It is the only window of TypeAlike.

The user meets TypeAlike in the middle of a task in another app. Each part must stay out of the way of that task.

## Capabilities and Constraints

- Panes of the settings window: General, Apps and websites, Copied and screen text, Personalization, Energy saving, Models, and Permissions on macOS. Only a build with the `playground` feature shows the Playground pane, a tool for developers.
- `typealike_core` owns all the text that the user sees, and the state of each control. The platform crates only show it and send back what the user did. Thus a change to the UI text goes into core, and it changes both platforms.
- Each language of the UI has a Fluent file for its texts (`crates/typealike_core/locales/<tag>/typealike.ftl`): English, with the source texts, Spanish, Brazilian Portuguese, French, German and Italian. The settings window, the menus and the badge take all their texts from these files, in the user's first preferred language that has a file, or else in English. The language menu in the General pane and in the first setup step overrides this choice. `.claude/rules/ui-text.md` gives the rules for the texts and their translations.
- On macOS, the platform crate changes the labels of menus, buttons and window titles to title case in English. The other languages keep sentence case. Core labels stay in sentence case.
- Before the first model download, the General pane recommends one model for the computer: Qwen3.5 0.8B on the processor, and on a graphics card the largest of Gemma 4 E4B, Gemma 4 E2B and Qwen3.5 2B that fits in its free memory. A button downloads the recommended model. When TypeAlike cannot read the graphics card, it recommends Gemma 4 E2B with a warning. The first model that the user downloads becomes the suggestion model.
- The setup shows when TypeAlike starts for the first time and after "Reset all settings", until the user finishes it. It is short, and the steps about the user come first: a demo of what TypeAlike does, the name and the notes with their examples, what suggestions can use (earlier writing, copied text, screen text), the Accessibility permission on macOS, the model download, and "Open at login". Any other setting keeps its default and appears where the user needs it.
- A test checks the UI text against the writing rules: `crates/typealike_core/src/test_support/writing_rules.rs`. The tests of the settings window, the Models and Playground panes and the badge menu use it.
- The strings of the prompt (`crates/typealike_core/src/prompt.rs`) are text for the model, not UI text. A change to them needs a `prompt_eval` run (see `.claude/skills/prompt-eval/SKILL.md`).

### Terms

Use one term for one thing. The writing-rules test rejects the words in the "Not" column.

| Term | Spanish | Portuguese (Brazil) | French | German | Italian | Meaning | Not |
|---|---|---|---|---|---|---|---|
| suggestion | sugerencia | sugestão | suggestion | Vorschlag | suggerimento | The text that TypeAlike offers at the caret. | completion, prediction, autocomplete |
| earlier writing | escritura anterior | textos anteriores | textes précédents | frühere Texte | testi precedenti | The texts that the user wrote before, which TypeAlike keeps to learn from. | history |
| screen text | texto de la pantalla | texto da tela | texte de l’écran | Bildschirmtext | testo dello schermo | The text near the field that TypeAlike reads on the screen. | |
| model | modelo | modelo | modèle | Modell | modello | The language model that writes the suggestions. | LLM |
| pause, resume | pausar, reanudar | pausar, retomar | suspendre, reprendre | pausieren, fortsetzen | sospendere, riprendere | The user stops or starts suggestions in all apps. | |
| typo fix | corrección de erratas | correção de erros de digitação | correction des fautes de frappe | Tippfehlerkorrektur | correzione degli errori di battitura | A suggestion that replaces a word with a typo. | |
| word choice | elección de palabras | escolha de palavras | choix des mots | Wortwahl | scelta delle parole | How much the earlier writing changes the words of a suggestion. | |
| full screen | pantalla completa | tela cheia | plein écran | Vollbild | schermo intero | An app whose window fills the whole screen, such as a video or a game. | fullscreen |

When a language gets its first translated message, add its term to each row.

### Not in the UI

These words belong to the code and the docs. The UI uses the text of the second column. The writing-rules test rejects the words of the first column in the English UI text. Only the Hugging Face search can show a quantization name such as Q4_K_M.

| Code and docs | UI text | Note |
|---|---|---|
| backend, inference backend | "Run the model on", with the choices "Processor (CPU)", "Graphics card (Vulkan)" and "NVIDIA graphics card (CUDA)" | The backend is the way that the model runs: on the CPU, or on a GPU through Vulkan, CUDA or Metal. macOS has no menu for it, because it always uses Metal. |
| ghost text | suggestion | |
| caret | where you type, the text field | |
| load, unload the model | The effect for the user: "frees memory", "Suggestions start again when you type in a text field." | In Spanish, "descargar" means "download" and also "unload". |
| token | Do not show. Tell the length or the time. | |
| prompt | Do not show. Name what the model reads: the text in the field, the copied text or the screen text. | |
| inference | Do not show. | |
| GGUF, quantization names such as Q4_K_M | The file size, and "faster" or "better suggestions" | The Hugging Face search is for experts, and can show these words. |

## Brand Commitments

- Name: TypeAlike.
- Voice: ASD-STE100 Simplified Technical English, for all the text that the user sees (`.claude/rules/ste100.md`). Short sentences, active voice, no contractions, no em dashes, no semicolons, no marketing words.
- Sentence case for all UI text in core.
- Personality: calm and quiet. TypeAlike is a guest, and its UI never competes with the content of the host app.
- Logo: a robot with a visor and two happy eyes, next to a text cursor (`assets/logo.svg`). The app icons, the tray icon and the menu bar extra come from it (`docs/icons.md`). The settings window, the setup window, the overlay and the badge do not show it.

## Evidence on Hand

- `README.md`: features, privacy, install steps, permissions and uninstall. The status is "early release".
- No user numbers, no testimonials, no benchmarks for marketing, no press. Do not invent them.
- The release builds have no Developer ID signature and no Authenticode signature yet.

## Product Principles

1. **A guest in other apps.** TypeAlike never takes the focus and never covers the text of the user. Its UI stays small and quiet.
2. **The text stays on the computer.** Models run on the computer. Only model search, model downloads and the check for updates that the user starts use the network. No cloud AI provider, no telemetry.
3. **Native on each OS.** Each part looks and acts as a part of Windows or macOS. TypeAlike uses no brand colors, custom fonts or custom controls.
4. **Plain words.** Each text tells what happens and what the user can do, in Simplified Technical English, with one term for one thing. It names what the user gets, and does not explain how TypeAlike works inside.
5. **Few settings, good defaults.** Add a setting only when users need different values. Never copy a setting of the OS.

## Accessibility & Inclusion

- Contrast: 4.5:1 for text and 3:1 for parts of the UI, in the light and the dark appearance. Never use color as the only signal.
- Windows: high contrast themes, text scaling, 100 to 200 percent display scale, Narrator, keyboard only.
- macOS: Increase Contrast, Reduce Transparency, Reduce Motion, Differentiate Without Color, VoiceOver, Full Keyboard Access.
- Each control has an accessible name. The order of the keyboard focus is the visual order.
- The two rule files list the manual check for each UI change.
