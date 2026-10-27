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

Planned text change (decided, not done yet): the backend menu of the General pane says "Run the model on", with the choices "Processor (CPU)", "Graphics card (Vulkan)" and "NVIDIA graphics card (CUDA)".

## Open decisions

- The word "caret" in the description of "Prepare before you type".
- The word "tokens" in the detail of a Playground output.
- "Loads" and "unloads the model" in the Energy saving pane and in the description of the suggestion model. The Spanish text "descarga el modelo" also means "downloads the model".
- The first run for a new user: what shows before the first model download, and which model TypeAlike recommends for the computer.
