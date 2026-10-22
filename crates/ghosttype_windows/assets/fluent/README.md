# Fluent color icons

These are the page icons of the settings window on Windows. They come from the Fluent UI System Icons of Microsoft (https://github.com/microsoft/fluentui-system-icons), in the color style at 20 px:

| File | Icon | Page icon |
| --- | --- | --- |
| `settings.svg` | `ic_fluent_settings_20_color` | `PageIcon::Gear` |
| `scan_type.svg` | `ic_fluent_scan_type_20_color` | `PageIcon::TextViewfinder` |
| `contact_card.svg` | `ic_fluent_contact_card_20_color` | `PageIcon::PersonText` |
| `bot.svg` | `ic_fluent_bot_20_color` | `PageIcon::Cube` |
| `edit.svg` | `ic_fluent_edit_20_color` | `PageIcon::TextCursor` |

The icons are the same as in the source, but each view box is cut to the live area of the icon (`2 2 16 16`), and the size is 16 px. Thus each icon fills the 16 px icon box of the navigation at scale 1. The teeth of `settings.svg` go 0.5 px past the live area, so its view box is `1.5 1.5 17 17`.

The icons have the MIT license in `LICENSE`.
