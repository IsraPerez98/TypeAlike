# Fluent color icons

These are the page icons of the settings window on Windows and the pictures of the setup window. They come from the Fluent UI System Icons of Microsoft (https://github.com/microsoft/fluentui-system-icons), in the color style. The icons have the MIT license in `LICENSE`.

Each file is a PNG file larger than the size that it shows at. WinUI draws an SVG without anti-aliasing when it scales the SVG: an SVG has smooth edges only at its own size and a display scale of 100 %. A bitmap gets smooth edges when WinUI makes it smaller, but the two kinds of image make it smaller in different ways:

- A setup picture is an `Image`, which decodes its bitmap at the size on the screen. Its file is four times its size, so it stays sharp up to a display scale of 400 %.
- A page icon is an `ImageIcon`, which makes its bitmap smaller on the graphics card. That gives smooth edges only for at most about half the size, and gives jagged edges for a quarter. Its file is two times its size: half size at 100 %, the same size at 200 %.

## Page icons

The page icons show in the 16 px icon box of the navigation. They are PNG files of 32 x 32 px, made from the icons at 20 px:

| File | Icon | Page icon |
| --- | --- | --- |
| `settings.png` | `ic_fluent_settings_20_color` | `PageIcon::Gear` |
| `globe.png` | `ic_fluent_globe_20_color` | `PageIcon::Languages` |
| `scan_type.png` | `ic_fluent_scan_type_20_color` | `PageIcon::TextViewfinder` |
| `contact_card.png` | `ic_fluent_contact_card_20_color` | `PageIcon::PersonText` |
| `bot.png` | `ic_fluent_bot_20_color` | `PageIcon::Cube` |
| `edit.png` | `ic_fluent_edit_20_color` | `PageIcon::TextCursor` |
| `apps.png` | `ic_fluent_apps_20_color` | `PageIcon::AppGrid` |
| `lightbulb.png` | `ic_fluent_lightbulb_20_color` | `PageIcon::Lightbulb` |
| `question_circle.png` | `ic_fluent_question_circle_20_color` | `PageIcon::Info` |

The view box of each SVG is cut to the live area of the icon (`2 2 16 16`) before the render, so each icon fills the icon box. The teeth of the settings icon go 0.5 px past the live area, so its view box is `1.5 1.5 17 17`.

## Setup pictures

The pictures above the title of each step of the setup window after the welcome step are in `setup/`. The welcome step shows the app logo (`../logo.png`, `docs/icons.md`). They show at 56 DIPs. They are PNG files of 224 x 224 px, made from the icons at the largest size that the source has in color:

| File | Icon | Setup step |
| --- | --- | --- |
| `setup/globe.png` | `ic_fluent_globe_24_color` | `SetupStep::Languages` |
| `setup/contact_card.png` | `ic_fluent_contact_card_48_color` | `SetupStep::About` |
| `setup/scan_type.png` | `ic_fluent_scan_type_24_color` | `SetupStep::Sources` |
| `setup/shield_checkmark.png` | `ic_fluent_shield_checkmark_48_color` | `SetupStep::Permission` |
| `setup/bot.png` | `ic_fluent_bot_24_color` | `SetupStep::Model` |
| `setup/checkmark_circle.png` | `ic_fluent_checkmark_circle_48_color` | `SetupStep::Ready` |

## Make a file again

1. Download the SVG of the icon from the source.
2. For a page icon, change its view box as above.
3. Change its `width` and `height` to the size of the PNG file: `32` for a page icon, `224` for a setup picture.
4. Render it with Microsoft Edge, with `SIZE` the same size: `msedge --headless --disable-gpu --hide-scrollbars --default-background-color=00000000 --window-size=SIZE,SIZE --screenshot=NAME.png NAME.svg`.
