---
paths:
  - "crates/ghosttype_windows/**"
---
# Windows 11 UI

Look native: Fluent 2 / Windows 11 design. Principles: effortless, calm, personal, familiar, coherent. GhostType is a guest in other apps; UI recedes, never competes with host content.
Source: learn.microsoft.com/windows/apps/design (signature-experiences: materials, geometry, typography, color, layering).

## Follow the system, never hardcode a look
- Theme: follow user light/dark (apps mode for windows, `SystemUsesLightTheme` for taskbar/tray). Re-read on `WM_SETTINGCHANGE` ("ImmersiveColorSet") and repaint live; no restart.
- Accent: user accent color only, sparingly (focus, selected, primary action). No brand color for state.
- High contrast: on, use system colors (`GetSysColor`) only, drop materials and custom tints. Re-check on `WM_SETTINGCHANGE`.
- Respect transparency off, battery saver, animations off (`SPI_GETCLIENTAREAANIMATION`): solid fallback, no motion.
- Text scale and DPI: per-monitor DPI aware v2, design in effective px, rescale on `WM_DPICHANGED`. Never bitmap-stretch.
- Core owns what is shown; this crate maps system appearance to drawing. Appearance values = named consts/types, not literals in paint code.

## Geometry
- Corner radius: 8 px top-level/transient (windows, flyouts, menus, dialogs); 4 px controls, bars, tooltips; 0 px where straight edges meet or window snapped/maximized.
- Top-level windows: `DWMWA_WINDOW_CORNER_PREFERENCE` (`DWMWCP_ROUND`; `DWMWCP_ROUNDSMALL` for tooltip-sized popups). Let DWM draw corners, border, shadow.
- Spacing on 4 px grid. Consistent gutters.

## Materials
- Mica: base layer of long-lived windows (settings). Once per window, behind all content; content layers transparent so it shows through. Win32: `DWMWA_SYSTEMBACKDROP_TYPE` = `DWMSBT_MAINWINDOW`, plus `DWMWA_USE_IMMERSIVE_DARK_MODE` matching theme.
- Acrylic: transient light-dismiss surfaces only (flyouts, context menus): `DWMSBT_TRANSIENTWINDOW`.
- Smoke: only behind modal dialogs. Never stack materials or put one on a single element.
- Falls back to solid automatically (inactive window, transparency off, older build); our layout must look right on the solid fallback too.

## Typography
- Segoe UI Variable (Text optical size for UI, Display for titles); fallback Segoe UI. Icons: Segoe Fluent Icons, fallback Segoe MDL2 Assets. No bundled fonts.
- Type ramp (size/line, epx): caption 12/16, body 14/20, body strong 14/20 semibold, subtitle 20/28 semibold, title 28/36 semibold.
- Regular for text, Semibold for titles/emphasis. No Bold, no italic. Min 12 px regular, 14 px semibold.
- Sentence case everywhere, titles and menu items included. Left aligned. Truncate with ellipsis.
- Non-Latin scripts: system UI font per language, not Segoe UI Variable.

## Ghost text overlay
- Inline, not a popup: no chrome, no border, no shadow, no material. Reads as the host's own text.
- Match host font family/size/baseline when known; else system UI font at caret height.
- Color: host text color at reduced emphasis (Fluent secondary/tertiary text); must stay legible on host background. High contrast: `COLOR_GRAYTEXT`.
- Never takes focus or input: `WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST`; excluded from taskbar and Alt+Tab.
- No animation beyond a subtle fade; none when animations off. Hide instantly on caret move, focus change, or fullscreen exclusive app.

## Tray & menus
- Tray icon: theme-aware glyph (light variant on dark taskbar, dark on light), swap on theme change. Multi-resolution `.ico` (16/20/24/32 px), crisp at every DPI. State (paused, error) by glyph change + tooltip, never color alone.
- Left click = primary action/flyout; right click = context menu. Tooltip names app + state.
- Menus: native look: rounded 8 px, acrylic, dark-mode aware, Segoe Fluent Icons glyphs optional, sentence case, keyboard accelerators.

## Windows (settings, dialogs)
- Settings-app style: Mica base, card layout (`LayerFillColorDefault`-like low-opacity cards), left nav if more than one page.
- Standard caption buttons and title bar; if extending into title bar, keep snap layouts (`HTMAXBUTTON`) and drag region working.
- Single-instance; reopen focuses existing window.

## Accessibility
- Every control: UIA name, role, keyboard reachable, visible focus rect (2 px accent-aware). Tab order = visual order.
- Contrast: text 4.5:1, UI parts 3:1, both themes. Never color as sole signal.
- Manual check per UI PR: light, dark, high contrast, 100/150/200 % scale, transparency off, Narrator.
