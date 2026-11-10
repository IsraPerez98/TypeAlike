# Logo and icons

The logo of TypeAlike and the icon files that come from it. The look of each OS is in `.claude/rules/windows-ui.md` and `.claude/rules/macos-ui.md`.

## Sources

- `assets/logo.svg` is the logo: a robot with a visor and two happy eyes, next to a text cursor. Each part has an `id` (`earL`, `earR`, `stem`, `ball`, `head`, `visor`, `eyes`, `letterI`).
- `assets/tray/tray-16.svg`, `tray-20.svg` and `tray-24.svg` are the one-color robot on the pixel grid of 16, 20 and 24 px. Each file has the face of both states: the group `active` (a filled visor with the eyes cut out) and the group `paused` (an empty visor with a pause sign).
- `assets/build_icons.py` makes all the icon files from these sources. The files that it makes are in git, so a build needs no extra tool.

## Rebuild the icons

1. Install resvg: `cargo install resvg` or `brew install resvg`.
2. On a Mac, run `python3 assets/build_icons.py`. The script uses `iconutil` and `tiffutil` of macOS.
3. Commit the changed files.

The script reads `RESVG` for the path of resvg when it is not on `PATH`.

## Files

| File | Use |
|---|---|
| `packaging/macos/TypeAlike.icns` | The app icon of `TypeAlike.app` (`CFBundleIconFile` in `Info.plist`), 16 to 1024 px |
| `crates/typealike_macos/assets/logo.png` | The picture of the welcome step of the setup window on macOS: the macOS app icon at 128 px, which shows at 64 pt |
| `crates/typealike/TypeAlike.ico` | The app icon in `TypeAlike.exe` (`TypeAlike.rc`, `build.rs`) and the icon of the Windows installer (`SetupIconFile`) |
| `crates/typealike_windows/assets/logo.png` | The picture of the welcome step of the setup window on Windows: the Windows app icon at 224 px, which shows at 56 DIPs |
| `crates/typealike_windows/assets/tray/*.ico` | The tray icon and the badge on Windows, for each status, in black and in white, and the paused robot in amber and in yellow |
| `crates/typealike_macos/assets/menu_bar/*.tiff` | The menu bar extra and the badge on macOS, for each status, at 1x and 2x |

## macOS app icon

- The logo is on a dark blue rounded rectangle of the macOS icon grid: 824 px on a canvas of 1024 px, with continuous corners and a soft shadow.
- macOS 26 and later put an icon without this shape on a gray plate. This icon has the shape, so it shows without the plate. On macOS 27, `NSWorkspace.icon(forFile:)` showed this icon without the plate. It showed the logo without a background on the plate.
- At 16 and 32 pt, the icon shows only the head of the robot, with thicker eyes, because the text cursor is too small to read there.
- TypeAlike runs as an accessory app, so the Dock never shows it. Finder, Launchpad, the login items and the privacy lists of System Settings show it.

## Windows app icon

- The logo has no plate, as the app icons of Windows 11. It fills the 44 px keyline of the 48 px grid of Fluent.
- The icon file has 16, 20, 24, 32, 40, 48, 64 and 256 px. At 16, 20 and 24 px, it shows only the head of the robot, with thicker eyes.
- The head has a darker outline than in the logo, so it shows on a light background.
- The settings and setup windows show the icon in the title bar and on the taskbar button. They load it from the resources of the program at the sizes of the DPI of the window (`settings/window_icon.rs`).

## Tray icon, menu bar extra and badge

- The robot shows the status by a change of its face, not of its color. Active: a filled visor with happy eyes. Paused: an empty visor with a pause sign.
- Stopped: while a stop of the Energy saving pane applies (energy saver, game, full screen), the paused robot is yellow. The face also changes, so the color is not the only sign.
- Windows: black on a light taskbar, white on a dark taskbar. The stopped robot uses the caution fill of Fluent: amber `#9D5D00` on a light taskbar, yellow `#FCE100` on a dark one. Pure yellow has too little contrast on a light taskbar. The icon file has 16, 20, 24, 32, 40, 48 and 64 px. Each size comes from the grid of 16, 20 or 24 px at 1x, 2x, 3x or 4x, so its edges stay sharp at 100 to 400 % scale. `tray.rs` takes the smallest image at least as wide as the small icon size of the system, or else the largest one.
- macOS: a template image of 16 pt, from the grid of 16 px at 1x and 2x. macOS gives it the color of the menu bar and of the selection.
- macOS, stopped: `in_yellow` draws the paused template image in `systemYellowColor`, in an image that draws itself again for each appearance. It is not a template image. On macOS 27, the menu bar showed a template image in black when the button had a `contentTintColor`.
- The badge shows the stopped robot in its own text color, not in yellow.
- The badge at the caret shows the same robot, 16 px on a plate of 24 px, in the text color of the badge.
- Windows: the badge tints the white tray icon (`robot_icon`). At 100, 125, 150, 200, 250, 300 and 400 % scale, the badge uses an image of the icon file without a change of size.
- macOS: the badge shows the template image of the menu bar extra (`status_glyph`), in the secondary label color.
