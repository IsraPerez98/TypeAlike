---
version: 1
slug: "crates-typealike-core-src-settings-window-rs"
primary_target: "crates/typealike_core/src/settings_window.rs"
related_targets: ["crates/typealike_core/src/settings_window/view.rs","crates/typealike_core/src/settings_window/controls.rs","crates/typealike_core/src/settings_window/general.rs","crates/typealike_core/src/settings_window/suggestion_places.rs","crates/typealike_core/src/settings_window/screen_text.rs","crates/typealike_core/src/settings_window/personalization.rs","crates/typealike_core/src/settings_window/energy_saving.rs","crates/typealike_core/src/settings_window/models.rs","crates/typealike_core/src/settings_window/models/page.rs","crates/typealike_core/src/settings_window/models/catalog.rs","crates/typealike_core/src/settings_window/playground.rs","crates/typealike_windows/src/settings.rs","crates/typealike_windows/src/settings/page.rs","crates/typealike_windows/src/settings/layout.rs","crates/typealike_windows/src/settings/icons.rs","crates/typealike_windows/src/settings/app_icon.rs","crates/typealike_windows/src/settings/text_area_box.rs","crates/typealike_macos/src/settings.rs","crates/typealike_macos/src/settings/page.rs","crates/typealike_macos/src/settings/layout.rs","crates/typealike_macos/src/settings/controls.rs","crates/typealike_macos/src/settings/sidebar.rs","crates/typealike_macos/src/settings/rows.rs","crates/typealike_macos/src/settings/rows/app.rs","crates/typealike_macos/src/settings/rows/item.rs","crates/typealike_macos/src/settings/rows/text.rs"]
---

# Settings window

## Scope

The settings window of TypeAlike on Windows and macOS: the navigation of the panes and each pane (General, Apps and websites, Screen text, Personalization, Energy saving, Models, Playground). Core gives each page as sections and rows (`settings_window::view`). Each platform shows the rows with its own controls.

## Mode

Operate. The user changes a setting and goes back to the work in another app. Scanability, consistency and the expectations of the OS come before expression.

## Audience and task

People who write a lot and are not necessarily technical. They open the window to turn suggestions on or off in an app, to pick a model, or to change how suggestions show. Each label and description must make sense without knowledge of language models.

## Constraints

- Windows: the Settings app of Windows 11 (`.claude/rules/windows-ui.md`, sections "Layering and elevation", "Geometry", "Materials", "Controls", "Typography").
- macOS: System Settings (`.claude/rules/macos-ui.md`, section "Windows (settings, onboarding)").
- All text and state come from core. A new setting is a new row in core, never a control in a platform crate.
- The text follows ASD-STE100, `.claude/rules/ui-text.md` and the terms of `PRODUCT.md`. The test of the writing rules checks the view.
- The Hugging Face search of the Models pane is the only place for experts. Only there can a text show a technical word without a plain word before it.

## Direction

Keep the incumbent native look. No visual redesign. Improvements are in the text, the order of the settings, the states (loading, error, empty) and accessibility.

All the texts come from the Fluent files (`crates/typealike_core/locales`), in English and Spanish. The backend menu of the General pane says "Run the model on", with the choices "Processor (CPU)", "Graphics card (Vulkan)" and "NVIDIA graphics card (CUDA)".

## Decisions

Closed on 2026-10-27, from `.claude/rules/ui-text.md` and the table "Not in the UI" of `PRODUCT.md`.

- **"Caret" in "Prepare before you type".** The UI does not show the word. The switch is now "Prepare suggestions early", and its description says "When you go to a text field, the model reads the text in it at once. The first suggestion then shows sooner, but this uses more power." The sentence about the off state went, because a description tells what happens when the switch is on.
- **"Tokens" in the Playground output.** The UI does not show them. The detail tells only the time, "Written in 0.42 seconds", with the decimal separator of the language ("0,42" in Spanish).
- **"Loads" and "unloads the model".** The UI does not show these words. Each text tells the effect for the user: "suggestions stop to free memory" and "They start again when you type in a text field." The note of a load says "Starting the model…". The Spanish texts say "detener" and "iniciar", so "descargar" means only "download". The test of the writing rules rejects the load words in English.
- **The first run.** Before the first download, the model menu of the General and Playground panes shows the note "TypeAlike needs a model to write suggestions. Download the recommended model below.", and below the menu the row of the recommended model with its size and a "Download" button. The row shows the progress, the cancel button and the errors of the download, as in the Models pane. TypeAlike recommends Qwen3.5 0.8B (516 MB) on the processor, because suggestions on the processor must be short to show soon, and Gemma 4 E2B (2.6 GB) on a graphics card, because it fits in the memory of most graphics cards. The Models pane marks the same model "Recommended for this computer". When the first download finishes and no suggestion model is saved, that model starts as the suggestion model, so that suggestions work with no other step.

## Open decisions

None.
