---
paths:
  - "crates/ghosttype_windows/**"
---
# Windows 11 UI

Look native: Fluent 2 / Windows 11 design. Principles: effortless, calm, personal, familiar, coherent. GhostType is a guest in other apps. Its UI recedes and never competes with host content.
Sources (checked 2026-10): learn.microsoft.com/windows/apps/design: `style/mica` (2026-07), `signature-experiences/layering` (2026-02), `basics/titlebar-design` (2026-07). Fluent 2: fluent2.microsoft.design. windows-reactor: github.com/microsoft/windows-rs, `docs/crates/windows-reactor.md` and `crates/libs/reactor/public-api.txt`.

## Two kinds of UI
- Settings window: WinUI 3 controls through windows-reactor (`settings.rs`, `settings/page.rs`), on its own UI thread. WinUI draws the controls and follows the system.
- Tray icon, tray menu, ghost text overlay and caret badge: Win32 on the main thread, next to the hooks. This crate draws them and follows the system by hand.

## Follow the system, never hardcode a look
- Settings window: WinUI follows the theme, the accent color, high contrast, transparency, animations, text scale and DPI live. Never set a theme, a color or a size that turns this off. Use `ThemeBrush` values, never a `Color` literal.
- Win32 parts: follow the apps theme for menus and `SystemUsesLightTheme` for the tray icon. Re-read on `WM_SETTINGCHANGE` ("ImmersiveColorSet", `SPI_SETHIGHCONTRAST`) and update live. No restart.
- Accent: the user accent color only, sparingly (focus, selected, primary action). No brand color for state.
- Per-monitor DPI aware v2 for the whole process. Physical px in the Win32 parts, effective px (DIPs) in reactor views.
- Core owns what is shown. This crate maps it to controls. Sizes and spacing are named consts, not literals in view code.

## Layering and elevation
- Two layers per window. Base layer = Mica, holds the title bar and the navigation. Content layer = the pages, as cards on top of the base layer.
- Card: `Border` with `ThemeBrush::CardBackground`, `ThemeBrush::CardStroke` at 1 px, radius 4, padding 16 x 12 px, at least 68 px high, 4 px apart. Section headings semibold, 28 px above the cards and 8 px over them.
- Shadows only to separate surfaces, never as decoration. WinUI and DWM draw window and flyout shadows.

## Geometry
- Corner radius: 8 px top-level/transient (windows, flyouts, menus, dialogs). 4 px controls, cards, bars, tooltips. WinUI controls bring their own radius. Do not change it.
- Spacing on a 4 px grid. Consistent gutters: the page margin is 56 px at the sides and 32 px at the top and bottom.
- Page width: the page fills the window up to 1000 px, at the leading edge, like the Settings app. Build it as a `ScrollViewer` with horizontal scrolling off, a `Border` with the page margin, and a one-column `Grid` (`GridLength::STAR.max(1000.0)`). Never give the direct child of a `ScrollViewer` a `max_width`. WinUI then moves it past the right edge when the window is wider than the limit.

## Materials
- Settings window: Mica through `WindowVisuals::backdrop(WindowBackdrop::Mica)`. Mica also shows in the title bar of `ViewContext::window_frame`. Keep every surface between the window and the Mica transparent, so the window looks like one surface.
- Never paint an opaque background on the root or the navigation. An opaque application background covers Mica.
- WinUI shows the solid fallback when transparency is off, battery saver is on, the window is inactive, or high contrast is on. The layout must look right on it too.
- Acrylic only for transient light-dismiss surfaces (flyouts, menus). WinUI flyouts and the `ComboBox` drop-down use it already.

## Controls
- One reactor control for each core row kind (`settings/page.rs`). A new row kind needs one renderer here, never a hand-drawn control.
  - Switch: `ToggleSwitch` at the trailing edge of a card.
  - Picker: `ComboBox` at the trailing edge, at least 200 px wide.
  - Text area: multi-line `TextBox` under the label, at least 160 px high, with its button below.
  - Text line: `TextBox` at the trailing edge of a card, at least 200 px wide. Each change goes to the core.
  - Text field: `TextBox` and an accent `Button` that submits. The return key also submits.
  - Search field: `AutoSuggestBox` with the `Find` symbol. The return key submits.
  - Item: title and subtitle at the leading edge, the detail and the accessory at the trailing edge. Download progress is a `ProgressBar` with a cancel `Button`.
  - Loading: a small `ProgressRing` and the text. Message: the text and an optional retry `Button`.
  - Navigation: `NavigationView` with the pane at the left, 280 px, no toggle button, no back button, no settings item. One item for each page, with its symbol.
- Primary action: `ButtonStyle::Accent`, at most one for each card. A back button in a section heading is `ButtonStyle::Subtle`.
- Secondary text: reactor has no `TextFillColorSecondaryBrush` yet. Use the caption size (12 px) with 75 % opacity, and check its contrast. Error notes use `ThemeBrush::SystemCritical` at full opacity.
- Controlled fields: declare the text of each field from state and send each change back as a message. The page keeps the text that the user types in each field, and declares it in place of the text of the view. A view from the main thread can come after newer keys, and its older text then moves the caret.
- Give each child a stable key (`keyed`): the page index, the section index, the row index and the control id. Reactor then keeps the focus and the caret of a control across renders.
- `AutoSuggestBox` has no query event in reactor yet. A `Border` with `on_key_up` around the field turns the return key into a submit.
- Reactor sets the text of a new `TextBox` before `AcceptsReturn`, and a single-line WinUI text box keeps only the first line of its text. A text area gets its text one render after its `TextBox` exists (`TextAreaBox`).

## Typography
- WinUI default fonts: Segoe UI Variable, and Segoe Fluent Icons for symbols. No bundled fonts.
- Type ramp (size/line, epx): caption 12/16, body 14/20, body strong 14/20 semibold, subtitle 20/28 semibold, title 28/36 semibold. Page titles use title, section headings use body strong.
- Regular for text, Semibold for titles and emphasis. No Bold, no italic.
- Sentence case everywhere, titles and menu items included. Left aligned. Wrap long text, never cut it.

## Title bar
- `ViewContext::window_frame` gives the standard title bar: the title, the system caption buttons, the drag region and snap layouts. Do not draw a title bar of your own.
- The title bar sits on the base layer and blends with the rest of the window (Mica shows through).

## Ghost text overlay
- Inline, not a popup: no chrome, no border, no shadow, no material. Reads as the host's own text.
- Match host font family/size/baseline when known. Else use the system UI font at caret height.
- Color: host text color at reduced emphasis (Fluent secondary/tertiary text). It must stay legible on the host background. High contrast: `COLOR_GRAYTEXT`.
- Never takes focus or input: `WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST`. Excluded from taskbar and Alt+Tab.
- No animation beyond a subtle fade, and none when animations are off. Hide instantly on caret move, focus change, or fullscreen exclusive app.

## Caret badge
- A small button above the caret, on its left, where the ghost text never goes. A click pauses or resumes suggestions (`BadgeView`). The glyph is the tray glyph of the status.
- 20 x 20 DIPs, 4 DIPs above the caret, radius 4, a 1 DIP border, a 12 DIP glyph. Scale with the DPI of the monitor at the caret.
- Colors of the Fluent tooltip for the apps theme: the fill, the surface stroke and the secondary text. High contrast: `COLOR_WINDOW` fill, `COLOR_WINDOWTEXT` border and glyph.
- Same window styles as the ghost text overlay. The mouse hook swallows a left click on the badge and posts it to the main window. The app under the badge never gets the click.

## Tray & menus
- Tray icon: theme-aware glyph (light variant on dark taskbar, dark on light), swap on theme change. Multi-resolution `.ico` (16/20/24/32 px), crisp at every DPI. State (paused, error) by glyph change + tooltip, never color alone.
- Left click = primary action/flyout. Right click = context menu. Tooltip names app + state.
- Menus: native look: rounded 8 px, acrylic, dark-mode aware, Segoe Fluent Icons glyphs optional, sentence case, keyboard accelerators.
- Dark menus: Win32 has no documented call. uxtheme exports `SetPreferredAppMode` (ordinal 135) and `FlushMenuThemes` (ordinal 136) since Windows 10 1903. Resolve them at run time with `GetProcAddress` and skip them when they are missing. Set force dark or force light from the apps theme at start and on each theme change.
- A menu runs a message loop of its own. Show it outside any borrow of the runtime: post a message, then show it from the window procedure.

## Windows (settings, dialogs)
- Settings-app style: Mica base layer across the whole window and the title bar, the navigation at the left, content as cards.
- Single instance. Reopen activates the open window. Close drops the window and its state. The reactor app keeps running for the next open.
- The window opens at 1000 x 720 DIPs, at least 640 x 480 DIPs (`WindowPolicy`).

## Accessibility
- WinUI controls bring their UIA role, keyboard focus, focus visuals and Tab order. Never replace a control with drawing only.
- Give each control without a visible label of its own an `automation_name` (switches, menus, fields).
- Tab order = visual order, so build the view in reading order.
- Contrast: text 4.5:1, UI parts 3:1, both themes, on Mica and on its solid fallback. Never color as sole signal.
- Manual check per UI PR: light, dark, high contrast, 100/150/200 % scale, transparency off, inactive window, Narrator, keyboard only, typing and IME in text fields, and the tray menu in both themes.
