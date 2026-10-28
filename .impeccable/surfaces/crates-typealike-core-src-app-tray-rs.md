---
version: 1
slug: "crates-typealike-core-src-app-tray-rs"
primary_target: "crates/typealike_core/src/app/tray.rs"
related_targets: ["crates/typealike_core/src/app/badge.rs","crates/typealike_windows/src/tray.rs","crates/typealike_windows/src/menu.rs","crates/typealike_macos/src/menu_bar.rs","crates/typealike_macos/src/badge_menu.rs","crates/typealike_macos/src/main_menu.rs","crates/typealike_macos/src/labels.rs"]
---

# Menus

## Scope

The tray icon and its menu on Windows, the menu bar extra and its menu on macOS, and the badge menu at the caret. Core gives the entries (`app/tray.rs`, `app/badge.rs`). The platforms show them as native menus.

## Mode

Operate. The user picks one entry and goes back to the work.

## Audience and task

The user pauses or resumes suggestions, turns suggestions off in the current app or on the current website, opens the settings or quits.

## Constraints

- `.claude/rules/windows-ui.md`, section "Tray & menus". Sentence case, native popup menus that follow the theme.
- `.claude/rules/macos-ui.md`, section "Menu bar extra & menus". Title case at the boundary of the macOS crate in English (sentence case in the other languages), an ellipsis for an entry that needs more input, keyboard equivalents.
- The status shows as a change of the glyph and of the tooltip, never of the color only.
- The label of the pause entry names the action that it does.
- `.claude/rules/ui-text.md` for the text of each entry and of the tooltip.

## Direction

Keep the native menus. No custom drawn menu, no popover.

## Open decisions

None.
