# Fluent color icons

These are the page icons of the settings window on Windows. They come from the Fluent UI System Icons of Microsoft (https://github.com/microsoft/fluentui-system-icons), in the color style at 20 px:

| File | Icon | Page icon |
| --- | --- | --- |
| `settings.svg` | `ic_fluent_settings_20_color` | `PageIcon::Gear` |
| `scan_type.svg` | `ic_fluent_scan_type_20_color` | `PageIcon::TextViewfinder` |
| `contact_card.svg` | `ic_fluent_contact_card_20_color` | `PageIcon::PersonText` |
| `bot.svg` | `ic_fluent_bot_20_color` | `PageIcon::Cube` |
| `edit.svg` | `ic_fluent_edit_20_color` | `PageIcon::TextCursor` |
| `apps.svg` | `ic_fluent_apps_20_color` | `PageIcon::AppGrid` |
| `lightbulb.svg` | `ic_fluent_lightbulb_20_color` | `PageIcon::Lightbulb` |

The icons are the same as in the source, but each view box is cut to the live area of the icon (`2 2 16 16`), and the size is 16 px. Thus each icon fills the 16 px icon box of the navigation at scale 1. The teeth of `settings.svg` go 0.5 px past the live area, so its view box is `1.5 1.5 17 17`.

The icons have the MIT license in `LICENSE`.

The pictures above the title of each step of the setup window are in `setup/`. They come from the icons of the source in the color style, at the largest size that the source has in color. They are PNG files of 224 x 224 px, four times the size of the picture (56 DIPs), so they stay sharp up to a display scale of 400 %. WinUI draws an SVG without anti-aliasing when it scales the SVG, and the source has these icons only at 24 or 48 px. A bitmap gets smooth edges when WinUI makes it smaller.

| File | Icon | Setup step |
| --- | --- | --- |
| `setup/text_edit_style.png` | `ic_fluent_text_edit_style_24_color` | `SetupStep::Welcome` |
| `setup/contact_card.png` | `ic_fluent_contact_card_48_color` | `SetupStep::About` |
| `setup/scan_type.png` | `ic_fluent_scan_type_24_color` | `SetupStep::Sources` |
| `setup/shield_checkmark.png` | `ic_fluent_shield_checkmark_48_color` | `SetupStep::Permission` |
| `setup/bot.png` | `ic_fluent_bot_24_color` | `SetupStep::Model` |
| `setup/checkmark_circle.png` | `ic_fluent_checkmark_circle_48_color` | `SetupStep::Ready` |

To make a picture again:

1. Download the SVG of the icon from the source.
2. Change its `width` and `height` to `224`. Keep its view box.
3. Render it with Microsoft Edge: `msedge --headless --disable-gpu --hide-scrollbars --default-background-color=00000000 --window-size=224,224 --screenshot=NAME.png NAME.svg`.
