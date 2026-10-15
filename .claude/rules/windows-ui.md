---
paths:
  - "crates/ghosttype_windows/**"
---
# Windows 11 UI

Look native: Fluent 2 / Windows 11 design. Principles: effortless, calm, personal, familiar, coherent. GhostType is a guest in other apps. Its UI recedes and never competes with host content.
Sources (checked 2026-10): learn.microsoft.com/windows/apps/design: `style/mica` (2026-07), `signature-experiences/layering` (2026-02), `basics/titlebar-design` (2026-07), `desktop/modernize/ui/apply-mica-win32` (2026-07). Fluent 2: fluent2.microsoft.design. Color and control values: the WinUI 3 source, github.com/microsoft/microsoft-ui-xaml, `controls/dev/CommonStyles/Common_themeresources_any.xaml` and the control `*_themeresources.xaml` files.

## Follow the system, never hardcode a look
- Theme: follow user light/dark (apps mode for windows, `SystemUsesLightTheme` for taskbar/tray). Re-read on `WM_SETTINGCHANGE` ("ImmersiveColorSet") and repaint live. No restart.
- Accent: user accent color only, sparingly (focus, selected, primary action). No brand color for state.
- High contrast: on, use system colors (`GetSysColor`) only. Drop materials and custom tints. Re-check on `WM_SETTINGCHANGE`.
- Respect transparency off, battery saver, animations off (`SPI_GETCLIENTAREAANIMATION`): solid fallback, no motion.
- Text scale and DPI: per-monitor DPI aware v2, design in effective px, rescale on `WM_DPICHANGED`. Never bitmap-stretch.
- Core owns what is shown. This crate maps system appearance to drawing. Appearance values = named consts/types, not literals in paint code.

## Layering and elevation
- Two layers per window. Base layer = Mica, holds title bar, navigation and commands. Content layer = the main task, on top of the base layer.
- Content layer patterns: standard (one contiguous surface, `LayerFillColorDefault`) or card (separate cards). Settings uses cards.
- Card colors on Mica: light fill `#FFFFFF` at 70 %, dark fill `#FFFFFF` at 5 %, 1 px stroke (light `#000000` at 6 %, dark `#000000` at 10 %). When the surface must be opaque, use the blend over the Mica fallback (light `#FBFBFB` / `#E5E5E5`, dark `#2B2B2B` / `#1D1D1D`).
- Elevation (shadow value, 1 px stroke): window and dialog 128, flyout 32, tooltip 16, card 8, control 2 (pressed 1), layer 1. Shadows only to separate surfaces, never as decoration. Let DWM draw window shadows.

## Geometry
- Corner radius: 8 px top-level/transient (windows, flyouts, menus, dialogs). 4 px controls, cards, bars, tooltips. 0 px where straight edges meet or the window is snapped/maximized.
- Top-level windows: `DWMWA_WINDOW_CORNER_PREFERENCE` (`DWMWCP_ROUND`, `DWMWCP_ROUNDSMALL` for tooltip-sized popups). Let DWM draw corners, border, shadow.
- Spacing on 4 px grid. Consistent gutters.

## Materials
- Mica: opaque, wallpaper-tinted base layer of long-lived windows (settings). Apply once per window, behind all content. It must also show in the title bar, so the window looks like one surface.
- Every layer between the window and a spot where Mica must show is transparent. Never apply a material to a single UI element. It does not show there.
- Mica Alt (`DWMSBT_TABBEDWINDOW`): stronger tint, only for a tabbed title bar or a commanding layer that needs more contrast. Not used now.
- Acrylic (translucent blur): transient light-dismiss surfaces only (flyouts, context menus): `DWMSBT_TRANSIENTWINDOW`.
- Smoke: only behind modal dialogs. Never stack materials.
- The system replaces Mica with a solid color (`SolidBackgroundFillColorBase`: light `#F3F3F3`, dark `#202020`) when transparency is off, battery saver is on, hardware is low-end, the window is inactive, or the build is older than 22000. High contrast shows the user's background color. The layout must look right on the solid fallback too.

## Drawing over Mica in Win32
- Microsoft documents Mica for Win32 only through the Windows App SDK `MicaController`. We do not depend on it. We use DWM directly: `DWMWA_SYSTEMBACKDROP_TYPE` = `DWMSBT_MAINWINDOW`, `DWMWA_USE_IMMERSIVE_DARK_MODE` matching the theme, and `DwmExtendFrameIntoClientArea` with margins `-1`, so Mica fills the client area.
- The extended frame blends client pixels with Mica by their alpha. GDI writes no alpha, so GDI output over the frame is see-through and broken.
- Draw every surface with Direct2D and DirectWrite (`settings/canvas.rs`): a software DC render target with premultiplied alpha at 96 DPI, so one unit is one physical pixel. Draw into a 32-bit memory bitmap, then copy it to the window with `BitBlt`. The copy keeps the alpha, and DWM blends it with Mica.
- Transparent pixels show Mica. Translucent Fluent fills (cards, controls) blend with it as WinUI does. Never draw an opaque copy of a translucent token.
- A child window replaces the pixels under it. A control on a card first fills the card color, then draws itself. The two layers then blend the same as the card around them.
- Text on alpha surfaces uses grayscale antialiasing, never ClearType.
- The top-level window and every page use `WS_CLIPCHILDREN`, so a parent never paints over its controls.
- Text fields stay `Edit` controls for typing, selection, IME and UIA. Each one is a layered child window (`WS_EX_LAYERED`) with `LWA_COLORKEY`. Its background is a key color one blue step off the field color, so only its text and caret show. The page draws the field frame and the placeholder under it.
- Layered child windows need the Windows 10 `supportedOS` entry in the app manifest (`crates/ghosttype/compatibility.manifest`).
- High contrast: `DWMSBT_NONE` and margins `0`. Every surface clears to the solid window color before it draws.

## Fluent colors
- One palette type (`settings/palette.rs`) holds the WinUI tokens for dark, light and high contrast. Paint code takes colors only from it, never literals.
- Tokens use straight alpha (`#AARRGGBB`), as in the WinUI source. Main dark / light values:
  - Text primary `#FFFFFF` / `#E4000000`, secondary `#C5FFFFFF` / `#9E000000`, disabled `#5DFFFFFF` / `#5C000000`.
  - Control fill rest `#0FFFFFFF` / `#B3FFFFFF`, hover `#15FFFFFF` / `#80F9F9F9`, pressed `#08FFFFFF` / `#4DF9F9F9`.
  - Control stroke `#12FFFFFF` / `#0F000000`, bottom edge `#18FFFFFF` / `#29000000`. Text field bottom edge `#8BFFFFFF` / `#72000000`.
  - Card fill `#0DFFFFFF` / `#B3FFFFFF`, card stroke `#19000000` / `#0F000000`.
  - Subtle fill (navigation hover, selected) `#0FFFFFFF` / `#09000000`, pressed `#0AFFFFFF` / `#06000000`.
- Accent fill: dark theme uses `SystemAccentColorLight2`, light theme uses `SystemAccentColorDark1`. Hover is 90 % opacity, pressed is 80 %. Read the shades from the `AccentPalette` registry value (HKCU `...\Explorer\Accent`): eight `R G B _` entries, Light3 to Dark3. Text on the accent is black in dark theme and white in light theme.
- High contrast maps tokens to system colors as WinUI does: text `COLOR_WINDOWTEXT`, controls `COLOR_BTNFACE` with `COLOR_BTNTEXT` borders, accent `COLOR_HIGHLIGHT`, disabled text `COLOR_GRAYTEXT`, surfaces `COLOR_WINDOW`.

## Controls
- Keep a standard window per control (`Static`, `Button`, `Edit`, `msctls_progress32`) for its UIA role, name, focus and keys. Draw it with owner draw (`SS_OWNERDRAW`, `BS_OWNERDRAW`). Store how it looks in its `GWLP_USERDATA`, so drawing needs only the window and the appearance.
- Owner-drawn statics report `ODS_DISABLED` in their draw state. Labels ignore the draw state and take their color from their tone.
- Control ids are never `IDOK` or `IDCANCEL`, so dialog keys never click a control.
- Track hover with a comctl32 subclass (`SetWindowSubclass`) and `TrackMouseEvent`. Remove the subclass on `WM_NCDESTROY`.
- Button: 32 px high, radius 4, padding 11 px, control fill per state, control stroke with a stronger 1 px bottom edge at rest and hover. Pressed text is secondary. Disabled text is disabled.
- Primary button: the accent fill, the on-accent stroke, and on-accent text. Use it for one main action per page.
- Text field: 32 px high, radius 4, control fill at rest and hover, `ControlFillColorInputActive` with focus. A 1 px strong bottom edge at rest, a 2 px accent bottom edge with focus. Text starts 11 px from the left. Placeholder in secondary text.
- Menu button (a combo box): looks like a button with leading text, 12 px padding, and a 12 px `E70D` chevron in secondary text, 14 px from the right edge. A click, Space or Enter opens a menu of the choices with the current one checked.
- Navigation item: 36 px high, radius 4, 4 px apart. Subtle fill on hover. Selected items get the subtle fill and a 3 x 16 px accent pill at the left edge. A 16 px glyph 12 px from the left, then the label 16 px after it.
- Progress bar: a 1 px track in the strong stroke color and a 3 px accent pill as the indicator.
- Scroll bar: a 12 px gutter at the right edge of the page. The thumb is a 2 px pill, 3 px from the edge, that grows to 6 px on hover and drag. The minimum thumb length is 24 px. A press in the gutter scrolls one page.
- Card (settings row): radius 4, card fill, 1 px card stroke, 16 px padding, at least 68 px high, 4 px apart. Section headings in body strong, 28 px above the cards and 8 px over them.
- Keyboard focus visual: a 2 px outer ring (`FocusStrokeColorOuter`) and a 1 px inner ring (`FocusStrokeColorInner`), only while the window shows keyboard cues (`ODS_NOFOCUSRECT` clear). Hide the cues when the window opens (`WM_CHANGEUISTATE`).

## Typography
- Segoe UI Variable (Text optical size for UI, Display for titles), fallback Segoe UI. Icons: Segoe Fluent Icons, fallback Segoe MDL2 Assets. No bundled fonts.
- Type ramp (size/line, epx): caption 12/16, body 14/20, body strong 14/20 semibold, subtitle 20/28 semibold, title 28/36 semibold.
- Regular for text, Semibold for titles/emphasis. No Bold, no italic. Min 12 px regular, 14 px semibold.
- Sentence case everywhere, titles and menu items included. Left aligned. Truncate with ellipsis.
- Non-Latin scripts: system UI font per language, not Segoe UI Variable.

## Title bar
- Standard height 32 px. Use 48 px only with a search box or a person picture in the title bar.
- The title bar sits on the base layer and blends with the rest of the window (Mica shows through).
- Icon 16 × 16 px, 16 px from the left edge, centered vertically. Title 16 px after the icon, caption style, Segoe UI Variable, truncated with ellipsis. Caption buttons always fully visible.
- Inactive window: title bar elements show semi-transparent.
- Empty space and the title are draggable. Right-click shows the system menu. Double-click toggles maximize.
- Prefer the system caption buttons. Custom caption buttons use Segoe Fluent Icons `E921` minimize, `E922` maximize, `E923` restore, `E8BB` close, full-bleed backplates, and keep snap layouts (`HTMAXBUTTON`) working.

## Ghost text overlay
- Inline, not a popup: no chrome, no border, no shadow, no material. Reads as the host's own text.
- Match host font family/size/baseline when known. Else use the system UI font at caret height.
- Color: host text color at reduced emphasis (Fluent secondary/tertiary text). It must stay legible on the host background. High contrast: `COLOR_GRAYTEXT`.
- Never takes focus or input: `WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST`. Excluded from taskbar and Alt+Tab.
- No animation beyond a subtle fade, and none when animations are off. Hide instantly on caret move, focus change, or fullscreen exclusive app.

## Tray & menus
- Tray icon: theme-aware glyph (light variant on dark taskbar, dark on light), swap on theme change. Multi-resolution `.ico` (16/20/24/32 px), crisp at every DPI. State (paused, error) by glyph change + tooltip, never color alone.
- Left click = primary action/flyout. Right click = context menu. Tooltip names app + state.
- Menus: native look: rounded 8 px, acrylic, dark-mode aware, Segoe Fluent Icons glyphs optional, sentence case, keyboard accelerators.
- Dark menus: Win32 has no documented call. uxtheme exports `SetPreferredAppMode` (ordinal 135) and `FlushMenuThemes` (ordinal 136) since Windows 10 1903. Resolve them at run time with `GetProcAddress` and skip them when they are missing. Set force dark or force light from the apps theme at start and on each theme change.
- A menu runs a message loop of its own. Show it outside any borrow of the runtime: post a message, then show it from the window procedure.

## Windows (settings, dialogs)
- Settings-app style: Mica base layer across the whole window and the title bar, left navigation on Mica if more than one page, content as cards.
- Standard caption buttons and title bar. If extending into the title bar, keep snap layouts (`HTMAXBUTTON`) and the drag region working.
- Single-instance. Reopen focuses the existing window.

## Accessibility
- Every control: UIA name, role, keyboard reachable, visible focus rect (2 px accent-aware). Tab order = visual order.
- Owner draw keeps the standard window of each control, so its UIA role and name stay. Never replace a control with drawing only.
- Enter clicks the focused button, like Space. The page scroll bar is drawing only. Keyboard users scroll as focus moves, so the page shows the focused control.
- Contrast: text 4.5:1, UI parts 3:1, both themes, on Mica and on its solid fallback. Never color as sole signal.
- Manual check per UI PR: light, dark, high contrast, 100/150/200 % scale, transparency off, inactive window, Narrator, hover and press on each control, typing and IME in text fields, and the menus in both themes.
