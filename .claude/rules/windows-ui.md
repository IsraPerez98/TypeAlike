---
paths:
  - "crates/typealike_windows/**"
---
# Windows 11 UI

Look native: Fluent 2 / Windows 11 design. Principles: effortless, calm, personal, familiar, coherent. TypeAlike is a guest in other apps. Its UI recedes and never competes with host content.
Sources (checked 2026-10): learn.microsoft.com/windows/apps/design: `style/mica` (2026-07), `signature-experiences/layering` (2026-02), `basics/titlebar-design` (2026-07). Fluent 2: fluent2.microsoft.design. windows-reactor: github.com/microsoft/windows-rs, `docs/crates/windows-reactor.md` and `crates/libs/reactor/public-api.txt`.

## Two kinds of UI
- Settings window: WinUI 3 controls through windows-reactor (`settings.rs`, `settings/page.rs`), on its own UI thread. WinUI draws the controls and follows the system.
- Tray icon, tray menu, ghost text overlay and caret badge: Win32 on the main thread, next to the hooks. This crate draws them and follows the system by hand.

## Follow the system, never hardcode a look
- Settings window: WinUI follows the theme, the accent color, high contrast, transparency, animations, text scale and DPI live. Never set a theme, a color or a size that turns this off. Use `ThemeBrush` values, never a `Color` literal. The color icons of the pages and the setup steps are the only colored pictures, and high contrast replaces them. The app logo on the welcome step of the setup keeps its colors, as the app icons of Windows do.
- Win32 parts: follow the apps theme for menus and `SystemUsesLightTheme` for the tray icon. Re-read on `WM_SETTINGCHANGE` ("ImmersiveColorSet", `SPI_SETHIGHCONTRAST`) and update live. No restart.
- Accent: the user accent color only, sparingly (focus, selected, primary action). No brand color for state.
- Per-monitor DPI aware v2 for the whole process. Physical px in the Win32 parts, effective px (DIPs) in reactor views.
- Core owns what is shown. This crate maps it to controls. Sizes and spacing are named consts, not literals in view code.

## Layering and elevation
- Two layers per window. Base layer = Mica, holds the title bar and the navigation. Content layer = the pages, as cards on top of the base layer.
- Card: `Border` with `ThemeBrush::CardBackground`, `ThemeBrush::CardStroke` at 1 px, radius 4, padding 16 x 12 px, at least 68 px high, 4 px apart. Section headings semibold, 28 px above the cards and 8 px over them.
- A card of a setting starts with its icon in a 24 px column, as a card of the Settings app. The texts start 16 px after that column, so the texts of all the cards line up. The icon is the 20 px glyph of the `RowIcon` (`settings/icons.rs`) in the text color, or the 24 px icon of an app.
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
  - Switch: `ToggleSwitch` at the trailing edge of a card, at its own width (`MinWidth` 0). WinUI makes it 154 px wide, which leaves a gap after its text, so it does not end where the menus end.
  - Picker: `ComboBox` at the trailing edge, 200 to 320 px wide.
  - Text area: multi-line `TextBox` under the header of the card, in line with the label, at least 160 px high, with its button below.
  - Text line: `TextBox` at the trailing edge of a card, at least 200 px wide. Each change goes to the core.
  - Text field: `TextBox` and an accent `Button` that submits. The return key also submits.
  - Search field: `AutoSuggestBox` with the `Find` symbol. The return key submits.
  - Item: title and subtitle at the leading edge, the detail and the accessory at the trailing edge. Download progress is a `ProgressBar` with a cancel `Button`.
  - Loading: a small `ProgressRing` and the text. Message: the text and an optional retry `Button`.
  - Link: a `HyperlinkButton` at the leading edge, with no card, for an option that most users do not need.
  - Navigation: `NavigationView` with the pane at the left, 280 px, no toggle button, no back button, no settings item. One item for each page, with its color icon, as in the Settings app: a Fluent UI System Icon in the color style (`assets/fluent`, MIT license), which fills the 16 px icon box. In high contrast, an item shows the symbol of its page, because a color icon does not follow the colors of the theme. The main thread sends each change of the contrast to the window.
  - Each page is a child with a key of its own, so that a page opens at its top and not at the scroll position of the last page.
- Each color icon and color picture is a PNG file, because WinUI draws an SVG without anti-aliasing when it scales the SVG: an SVG has smooth edges only at its own size and a display scale of 100 %. A picture in an `Image` (`Image::source_data`, the step pictures of the setup) is four times its size, because `Image` decodes the bitmap at the size on the screen. An icon in an `ImageIcon` (`Icon::image_data`, the page icons) is two times its size, because `ImageIcon` makes the bitmap smaller on the graphics card, and a quarter of the size gives jagged edges (`assets/fluent/README.md`).
- Primary action: `ButtonStyle::Accent`, at most one for each card. A back button in a section heading is `ButtonStyle::Subtle`.
- A default button gets no `.style()`. Reactor gives `ButtonStyle::Default` a null style, and WinUI then drops its Fluent button style (the button gets square corners and no Fluent border).
- Secondary text: reactor has no `TextFillColorSecondaryBrush` yet. Use the caption size (12 px) with 75 % opacity, and check its contrast. Error notes use `ThemeBrush::SystemCritical` at full opacity.
- Controlled fields: declare the text of each field from state and send each change back as a message. The page keeps the text that the user types in each field, and declares it in place of the text of the view. A view from the main thread can come after newer keys, and its older text then moves the caret.
- Give each child a stable key (`keyed`): the page index, the section index, the row index and the control id. Reactor then keeps the focus and the caret of a control across renders.
- `AutoSuggestBox` has no query event in reactor yet. A `Border` with `on_key_up` around the field turns the return key into a submit.
- Reactor sets the text of a new `TextBox` before `AcceptsReturn`, and a single-line WinUI text box keeps only the first line of its text. A text area gets its text one render after its `TextBox` exists (`TextAreaBox`).

## Typography
- WinUI default fonts: Segoe UI Variable, and Segoe Fluent Icons for symbols. No bundled fonts. Windows 10 has only Segoe MDL2 Assets, so use only glyphs that both icon fonts have.
- Type ramp (size/line, epx): caption 12/16, body 14/20, body strong 14/20 semibold, subtitle 20/28 semibold, title 28/36 semibold. Page titles use title, section headings use body strong.
- Regular for text, Semibold for titles and emphasis. No Bold, no italic.
- Sentence case everywhere, titles and menu items included. Left aligned. Wrap long text, never cut it.
- Text next to an icon or a button goes in a `Grid` with an `Auto` and a `STAR` column. A horizontal `StackPanel` never wraps its text.

## Title bar
- `ViewContext::window_frame` gives the standard title bar: the title, the system caption buttons, the drag region and snap layouts. Do not draw a title bar of your own.
- The title bar sits on the base layer and blends with the rest of the window (Mica shows through).

## Ghost text overlay
- Inline, not a popup: no chrome, no border, no shadow, no material. Reads as the host's own text.
- Match host font family/size/baseline when known. Else use the system UI font at caret height.
- Color: host text color at reduced emphasis (Fluent secondary/tertiary text). It must stay legible on the host background. High contrast: `COLOR_GRAYTEXT`.
- Never takes focus or input: `WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST`. Excluded from taskbar and Alt+Tab.
- No animation beyond a subtle fade, and none when animations are off. Hide instantly on caret move, focus change, or fullscreen exclusive app.
- The ghost text after the caret ends 4 px before the badge at the end of the line of the caret, so it never covers the badge. Its wrapped lines keep the whole width of the field.
- Exception: when text follows the caret on its line, inline ghost text would cover that text. The overlay then shows the end of the typed word and the ghost text on a card below the caret line: the fill and the border of the Fluent tooltip (as the badge), radius 4, a 1 DIP border, 8 x 4 DIPs padding, 4 DIPs below the line. The card goes above the line when it does not fit in the work area. The typed word is in the Fluent primary text color and semibold. The ghost text is regular, in the lightest Fluent text color with 4.5:1 on the card: secondary in light mode, tertiary in dark mode. The ghost text starts under the caret.

## Caret badge
- A small button at the end of the line of the caret, inside the field. The line has no text there, and the ghost text ends before it. Core gives its frame (`BadgeView::frame`, `docs/core/ghost-text.md`, "Badge").
- A click opens the badge menu (`BadgeMenu`) below the badge, with the right edge of the menu at the right edge of the badge. Thus the menu opens toward the field.
- The glyph is the robot of the tray icon for the status (`robot_icon` in `tray.rs`), tinted in the glyph color.
- A square as high as the line of the caret, from 16 to 24 DIPs, 4 px inside the right edge of the field, in the middle of the line. Scale with the DPI of the monitor at the caret.
- Radius 4, a 1 DIP border, a glyph of at most 16 DIPs with 2 DIPs of room inside each edge.
- Never above or below the line of the caret: there it covers the text of the lines around the caret, or the menu bar above the first line.
- No badge while text can follow the caret on its line, or while the line has no room for it. A badge there covers text.
- Colors of the Fluent tooltip for the apps theme: the fill, the surface stroke and the secondary text. High contrast: `COLOR_WINDOW` fill, `COLOR_WINDOWTEXT` border and glyph.
- Same window styles as the ghost text overlay. The mouse hook swallows a left click on the badge and posts it to the main window. The app under the badge never gets the click.
- The badge menu is a native popup menu, as the tray menu, with a submenu and check marks. The window that had the focus gets it back after the menu closes.

## Tray & menus
- Tray icon: the robot of the logo, white on a dark taskbar and black on a light one, swapped on theme change. A multi-resolution `.ico` for each status and color (16 to 64 px, `assets/tray`), drawn on the pixel grids of 16, 20 and 24 px, so it is crisp at every DPI. State (paused, error) by glyph change + tooltip, never color alone. A stop of the Energy saving pane also makes the paused robot yellow, in the Fluent caution fill: amber `#9D5D00` on a light taskbar, yellow `#FCE100` on a dark one. The Windows guide reserves red, yellow and green for status.
- Left click = primary action/flyout. Right click = context menu. Tooltip names app + state.
- Menus: native look: rounded 8 px, acrylic, dark-mode aware, Segoe Fluent Icons glyphs optional, sentence case, keyboard accelerators.
- Dark menus: Win32 has no documented call. uxtheme exports `SetPreferredAppMode` (ordinal 135) and `FlushMenuThemes` (ordinal 136) since Windows 10 1903. Resolve them at run time with `GetProcAddress` and skip them when they are missing. Set force dark or force light from the apps theme at start and on each theme change.
- A menu runs a message loop of its own. Show it outside any borrow of the runtime: post a message, then show it from the window procedure.

## App icon
- The logo without a plate, as the app icons of Windows 11, in `TypeAlike.exe` (`TypeAlike.rc`). 16, 20, 24, 32, 40, 48, 64 and 256 px. At 24 px and below, only the head of the robot.
- Each window of TypeAlike shows it in the title bar and on the taskbar button (`settings/window_icon.rs`).
- `docs/icons.md` lists the sources and the steps to rebuild the files.

## Windows (settings, dialogs)
- Settings-app style: Mica base layer across the whole window and the title bar, the navigation at the left, content as cards.
- Single instance. Reopen activates the open window. Close drops the window and its state. The reactor app keeps running for the next open.
- The window opens at 1000 x 720 DIPs, at least 640 x 480 DIPs (`WindowPolicy`).
- The setup window has a fixed size in the center of the work area of the main monitor: no resize frame and no maximize button (`settings/fixed_window.rs`). Each step fits in it without a scroll bar, also in the longest translation. Measure a new or longer step in each language.

## Accessibility
- WinUI controls bring their UIA role, keyboard focus, focus visuals and Tab order. Never replace a control with drawing only.
- Give each control without a visible label of its own an `automation_name` (switches, menus, fields).
- Tab order = visual order, so build the view in reading order.
- Contrast: text 4.5:1, UI parts 3:1, both themes, on Mica and on its solid fallback. Never color as sole signal.
- Manual check per UI PR: light, dark, high contrast, 100/150/200 % scale, transparency off, inactive window, Narrator, keyboard only, typing and IME in text fields, and the tray menu in both themes.
