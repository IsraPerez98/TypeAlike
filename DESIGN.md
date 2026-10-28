# Design System: TypeAlike

## Overview

The visual system of TypeAlike is the system of the OS. TypeAlike has no brand palette, no own font and no own controls. This file gives the decisions. The rule files give the details, and they win when this file is different:

- Windows: `.claude/rules/windows-ui.md`. Fluent 2, with the Settings app of Windows 11 as the model.
- macOS: `.claude/rules/macos-ui.md`. The Apple Human Interface Guidelines, with System Settings as the model.

There are two kinds of UI:

1. **The settings window and the menus.** The OS draws them with its own controls: WinUI 3 on Windows, AppKit on macOS. They follow the theme, the accent color, the contrast settings and the text size of the OS while they are open.
2. **The overlay at the caret.** TypeAlike draws it: the suggestion, the card below the caret, the typo fix, the mirror (macOS) and the badge. It must look like a part of the host app, not like a part of TypeAlike.

## Colors

- No brand color. The accent color of the user shows only for the focus, the selection and the primary action.
- Settings window: theme brushes on Windows (`ThemeBrush`), semantic `NSColor` values on macOS. Never a color literal.
- Suggestion after the caret: the color of the text of the field at half opacity (`GHOST_TEXT_ALPHA`), as a placeholder. When TypeAlike cannot read that color, a neutral gray that shows on a light and on a dark background (`NEUTRAL_GRAY` on Windows, `systemGrayColor` on macOS).
- Card below the caret and mirror: the typed word in the primary text color, the suggestion in a lighter text color with a contrast of at least 4.5:1.
- Typo fix: a red line through the typed letters, and the fixed word in green (the Fluent colors on Windows, `systemGreen` on macOS).
- High contrast (Windows) and Increase Contrast (macOS) replace these colors with the colors of the system.

## Typography

- Windows: Segoe UI Variable, and Segoe Fluent Icons for symbols. The type ramp of Fluent: caption, body, body strong, subtitle, title.
- macOS: the system font (SF Pro) and its text styles, and SF Symbols.
- Regular for text, semibold for titles and emphasis. Sentence case in core. In English, macOS changes menus, buttons and window titles to title case.
- Overlay: the font face of the text before the caret when the OS gives it and it is installed, at the height of the caret. Else Segoe UI on Windows and the system font on macOS.

## Layout

- Settings window on Windows: navigation at the left (280 px), pages as cards on a 4 px grid, a page margin of 56 px at the sides, a page width of at most 1000 px at the leading edge.
- Settings window on macOS: a sidebar of panes with colored symbol tiles, a fixed window width, text that wraps.
- Each setting is one card or row: icon, label, description, and the control at the trailing edge.
- Text wraps and never cuts. A row must also look right with a translated text that is longer than the English text.
- Overlay: the suggestion starts at the caret. When text follows the caret on its line, the suggestion goes on a card below the line, under the caret.

## Elevation & Depth

- Windows: two layers. Mica is the base layer with the title bar and the navigation. The pages are cards on top of it. WinUI and the window manager draw the only shadows.
- macOS: Liquid Glass only on the functional layer that AppKit gives (sidebar, toolbar, menus). No custom glass.
- Overlay: no shadow, no border and no material on the suggestion after the caret. On Windows, the card below the caret and the badge use the fill and the border of the Fluent tooltip. On macOS, they are small rounded panels.

## Shapes

- Windows: a radius of 8 px for windows, menus and flyouts, and 4 px for controls, cards, the card below the caret and the badge.
- macOS: the radii of the standard controls. A custom shape is concentric with the corners of its container.
- Badge: a 24 x 24 square above the caret, at its left (DIPs on Windows, points on macOS).

## Components

- **Setup window.** A window of its own for the first start: the picture of the step (a Fluent color icon on Windows, an SF Symbol on macOS), the title, the body, the rows of the settings window and a footer with a dot for each step, Back and Next. A step slides in with a short fade, and only fades when the OS reduces motion. A demo shows a suggestion as it shows at the caret. No logo and no brand color.
- **Rows of the settings window.** Core gives each page as sections and rows (`settings_window::view`). Each platform has one renderer for each kind of row: switch, picker, text area, text line, text field, item, note, loading text, message and link. A link is a quiet button with no card, for an option that most users do not need. A new setting needs no platform code.
- **Suggestion.** Inline text after the caret. It never takes the focus or a click, and it hides at once when the caret moves or the focus changes.
- **Card below the caret.** The end of the typed word, then the suggestion, when text follows the caret on its line.
- **Typo fix.** The typed letters with a red line through them, then the fixed word in green.
- **Badge.** A small button above the caret with the glyph of the status. A click opens the badge menu, a native popup menu.
- **Tray icon and menu bar extra.** A monochrome glyph that follows the theme of the taskbar or the menu bar. The status shows as a change of the glyph, never of the color only.

## Do's and Don'ts

### Do:

- Use the controls of the OS for each part of the settings window.
- Follow the theme, the contrast, the text size and the motion settings of the OS while the app runs.
- Check each UI change in light, dark and high contrast, at 100, 150 and 200 percent scale, and with a screen reader and the keyboard only.

### Don't:

- Do not add a brand color, a gradient, a custom font or a custom control.
- Do not animate the overlay beyond a short fade, and never when the OS reduces motion.
- Do not draw a window frame, a title bar or a shadow of your own.
- Do not put UI text or UI state in a platform crate. Core owns them.
