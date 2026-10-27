---
version: 1
slug: "crates-typealike-core-src-controller-ghost-view-rs"
primary_target: "crates/typealike_core/src/controller/ghost_view.rs"
related_targets: ["crates/typealike_windows/src/overlay.rs","crates/typealike_windows/src/badge.rs","crates/typealike_windows/src/gdi/text_bitmap.rs","crates/typealike_windows/src/gdi/font.rs","crates/typealike_windows/src/system_theme.rs","crates/typealike_macos/src/overlay.rs","crates/typealike_macos/src/mirror.rs","crates/typealike_macos/src/badge.rs","crates/typealike_macos/src/panel.rs"]
---

# Overlay at the caret

## Scope

What TypeAlike draws in the apps of the user: the suggestion after the caret, the card below the caret, the typo fix, the mirror next to a field without a caret (macOS) and the badge above the caret. Core decides what shows and where (`controller/ghost_view.rs`). The platforms draw it.

## Mode

Operate, as a guest. The user works in another app. The overlay must look like a part of that app and must never draw attention to itself.

## Audience and task

The user types in any app. A suggestion that the user can read at a glance, and that Tab takes, is the whole task.

## Constraints

- `.claude/rules/windows-ui.md`, sections "Ghost text overlay" and "Caret badge".
- `.claude/rules/macos-ui.md`, section "Ghost text overlay".
- Never take the focus or a click (except the click on the badge). Hide at once when the caret moves or the focus changes.
- No animation beyond a short fade, and none when the OS reduces motion.
- The colors come from the text of the field or from the system. Check the contrast on light and dark host apps, and in high contrast.

## Direction

Keep the incumbent look: inline text in the font of the host, at half the opacity of the host text. No brand color, no material, no shadow on the suggestion.

## Open decisions

- A tip at the first suggestion that tells the user that Tab takes it. It shows one time.
