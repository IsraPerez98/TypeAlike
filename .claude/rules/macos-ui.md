---
paths:
  - "crates/typealike_macos/**"
---
# macOS UI

Look native: Apple Human Interface Guidelines (HIG), macOS 26+ Liquid Glass. macOS 27 lowers default glass transparency and adds a user tint control. Principles: purpose, agency ("stay out of the way"), familiarity, simplicity, craft. TypeAlike is a guest in other apps. UI recedes, never competes with host content.
Source: developer.apple.com/design/human-interface-guidelines (design-principles, materials, color, dark-mode, typography, the-menu-bar, menus, settings, privacy, accessibility).

## Follow the system, never hardcode a look
- Appearance: follow `NSApp.effectiveAppearance` (Light, Dark, Auto can switch while running). Redraw in `viewDidChangeEffectiveAppearance`, no restart. No app-specific light/dark setting.
- Color: semantic `NSColor` only (`labelColor`, `secondaryLabelColor`, `separatorColor`, `windowBackgroundColor`). Never hardcode system color values. Never give a semantic color a second meaning (no separator color as text). A custom color needs light, dark, and increased-contrast variants.
- Accent: `controlAccentColor` only, sparingly (focus, selection, primary action). No brand color for state.
- Accessibility display options (`NSWorkspace`): Increase Contrast, Reduce Transparency, Reduce Motion, Differentiate Without Color. Re-read on `accessibilityDisplayOptionsDidChangeNotification` and redraw live.
- Units: points, not pixels. Retina via `backingScaleFactor`. Re-render on screen or scale change. Never bitmap-stretch. Keep content out of the camera housing (`NSScreen.safeAreaInsets`).
- Core owns what is shown. This crate maps system appearance to drawing. Appearance values = named consts/types, not literals in draw code.

## Geometry
- AppKit draws window frame, corners, shadow, traffic lights. No custom window chrome, never imitate it.
- Standard controls carry their own radii. A custom shape nests concentric with the corners of its container.
- Control size: default 28×28 pt, min 20×20 pt. Padding about 12 pt around bezeled elements, about 24 pt around borderless ones.
- No critical controls or info at the bottom edge of a window. People often move it offscreen.

## Materials
- Liquid Glass = functional layer only (bars, sidebars, menus, popovers). Standard AppKit components adopt it automatically. Never in the content layer, never stacked.
- Custom glass (`NSGlassEffectView`, style `.regular`): only on the most important custom control, if any. `.clear` is for rich media backgrounds only. TypeAlike has none, so do not use it.
- Content-layer backgrounds: standard `NSVisualEffectView` material picked by purpose (`.menu`, `.popover`, `.hudWindow`, `.sidebar`, `.windowBackground`), never by the color it gives. Vibrant system colors on top.
- Glass look changes with the user tint setting, Reduce Transparency, and Increase Contrast. Layout must also look right on the opaque fallback.

## Typography
- System font only: SF Pro via `NSFont.systemFont` or text styles. Dynamic optical size picks Text/Display. SF Mono for code. Never bundle SF or request it by family name. Icons: SF Symbols.
- macOS text styles (size/line, pt): Large Title 26/32, Title 1 22/26, Title 2 17/22, Title 3 15/20, Headline 13/16 bold, Body 13/16, Callout 12/15, Subheadline 11/14, Footnote and Caption 10/13.
- Default 13 pt, min 10 pt. Weights Regular, Medium, Semibold, Bold. No Ultralight, Thin, Light.
- To match system controls, use role fonts: `menuFont`, `controlContentFont`, `labelFont`, `toolTipsFont`.
- macOS has no Dynamic Type. Layout still grows with longer (localized) strings. No fixed-width text boxes.
- Title case: menu items, buttons, window and pane titles. Sentence case: checkbox and switch labels, descriptions, alert body, permission purpose strings.
- Core labels are sentence case (Windows style). Title-case them at this crate's boundary in a pure, unit-tested fn (`labels::menu_label`), only in a language with title case (`UiLanguage::has_title_case`, from the `language` of the view).
- Menu item labels: no articles. Add an ellipsis (…) when the item needs more input before it completes (`Settings…`).

## Ghost text overlay
- Inline, not a popup: no chrome, no border, no shadow, no material. Reads as the host's own text.
- Match host font family, size, and baseline when known (AX attributes). Else system font at caret height.
- Color: host text color at reduced emphasis (like `secondaryLabelColor` or `tertiaryLabelColor`). Must stay legible on the host background. Increase Contrast on: never lighter than secondary.
- Never takes focus or input: borderless `NSPanel` with `.nonactivatingPanel`, `ignoresMouseEvents = true`, `canBecomeKey` and `canBecomeMain` false, `hasShadow = false`, clear background, level `.floating` or higher. Collection behavior: `.canJoinAllSpaces`, `.fullScreenAuxiliary`, `.ignoresCycle`, `.stationary`.
- App runs as `.accessory`: no Dock icon, not in Command-Tab.
- No animation beyond a subtle fade. None when Reduce Motion is on. Hide instantly on caret move, focus change, Space switch, or secure input.

## Menu bar extra & menus
- `NSStatusItem` icon: the robot of the logo as a template image (`isTemplate = true`), black and clear only, 16 pt with 1x and 2x pixels on the 16 px grid. The system tints it for light/dark menu bar and selection. Menu bar height is 24 pt.
- State (paused, error) by glyph change, never color alone.
- Click shows a menu, not a popover. Items: `Pause Suggestions` / `Resume Suggestions` (one item, label shows the action), separator, `Settings…` ⌘,, `Quit TypeAlike` ⌘Q.
- The system can hide the extra (crowded menu bar, notch, user turns it off in System Settings > Menu Bar). Never rely on it.
- Menu icons: sparingly. Give icons to all items in a group or to none. Show keyboard equivalents. Never repurpose standard shortcuts.

## App icon
- The logo on a dark rounded rectangle of the macOS icon grid: 824 px on a 1024 px canvas, continuous corners, a soft shadow. macOS 26+ puts an icon without this shape on a gray plate.
- At 16 and 32 pt, only the head of the robot.
- `docs/icons.md` lists the sources and the steps to rebuild the files.

## Windows (settings, onboarding)
- Settings: like System Settings. A sidebar at the leading edge (`NSSplitViewItem` sidebar, source list `NSTableView`) with a row for each pane, and the selected pane at its right. Each row shows the SF Symbol of its pane in white on a rounded tile, then the pane name. The tile has a system color (`systemBlueColor` and others) that names the pane, never a state. Title = pane name, in a unified toolbar after the sidebar (`NSToolbarSidebarTrackingSeparatorItemIdentifier`). Minimize and zoom buttons dimmed. Restore the last pane. ⌘, opens, ⌘W closes.
- Settings width: fixed. Text never makes the window wider. A wrapping label has a low horizontal compression resistance, and a pop-up menu has a maximum width.
- Few settings, good defaults. Never copy a system setting (appearance, accessibility). Link to the System Settings pane instead.
- Single instance. Reopen focuses the existing window and brings the app forward.
- Permissions (Accessibility, Input Monitoring): ask at first launch only because the core feature needs them. Explain why in one clear sentence first. Pre-prompt window: one button that opens the system prompt or System Settings pane. No cancel button, no "Allow" label. Show live status and recover when the user grants access later.

## Accessibility
- Every control: AX label, role, keyboard reachable (Full Keyboard Access), visible focus ring (`keyboardFocusIndicatorColor`). Tab order = visual order.
- Contrast: 4.5:1 for text up to 17 pt, 3:1 for larger or bold text. Aim for 7:1 for small custom text. Check both appearances. Never color as sole signal.
- Reduce Motion: no zoom, scale, or slide. Fades only. No timed auto-dismiss for content people must read.
- Manual check per UI PR: light, dark, Auto switch while running, Increase Contrast, Reduce Transparency, Reduce Motion, both ends of the glass tint control, Retina and non-Retina display, notch display, full-screen Space, VoiceOver, Full Keyboard Access.
