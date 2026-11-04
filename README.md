<p align="center"><img src="assets/logo.svg" alt="" width="160"></p>

# TypeAlike

Inline suggestions as you type, in any app on Windows and macOS. The model runs on your computer.

[Download](https://github.com/IsraPerez98/TypeAlike/releases) ·
[Behavior docs](docs/) ·
[Report a bug](https://github.com/IsraPerez98/TypeAlike/issues) ·
[Contributing](CONTRIBUTING.md)

**Status:** early release, version 0.4.0. Expect bugs. TypeAlike runs on Windows 10 or 11 (x64) and on macOS 15 or
later (Apple silicon). It is free and open source. Inspired by [Cotypist](https://cotypist.app/) and
[Cotabby](https://github.com/FuJacob/cotabby).

## How it works

1. Type in a text field of any app, such as an email, a chat or a document.
2. A suggestion shows in gray where you type. If you type in the middle of a line, the suggestion shows on a small
   card below the line.
3. Press **Tab** to type the next word of the suggestion. Press **Tab** again for the word after it.
4. To ignore the suggestion, type as usual. **Esc** hides it and stops suggestions in that field for 10 seconds.

What TypeAlike does:

- Learns from your earlier writing, so suggestions use your words and your style. Each app can share its text with
  all apps, keep it for itself, or keep nothing.
- Reads your name, a few notes about you, the text that you copy, and the text near the field, such as the last
  messages of a chat.
- Fixes typos. The suggestion shows the typo with a line through it, then the fixed word. **Tab** types the fix.
- Works only where you want it. A badge at the caret pauses suggestions, or turns them off in one app or on one
  website. Password fields and search boxes get no suggestions.
- Saves energy. Suggestions stop and the model frees its memory during games, in full screen, in energy saver or
  Low Power Mode, and after 30 minutes without use.
- Shows its settings in English, Spanish, Brazilian Portuguese, French, German and Italian.

A suggestion is the model's guess, and it can be wrong. A smaller model is faster but wrong more often.

## Privacy

- The model runs on your computer: on the processor or the graphics card on Windows, and with Metal on macOS.
- TypeAlike uses the internet only to search for models and to download them from Hugging Face (`huggingface.co`), and to check for updates on GitHub (`api.github.com`) when you click "Check for updates" on the About page.
  It has no account, no telemetry and no update check. The text that you type never leaves your computer.
- Earlier writing, copied text and screen text are on by default. You can turn each one off in the setup or in
  **Settings**.
- TypeAlike keeps the earlier writing in `history.bin`, encrypted with ChaCha20-Poly1305. The key is in the login
  keychain on macOS, and in `history.key` (locked with the Windows Data Protection API) on Windows.
- Before TypeAlike saves any text, it removes private keys, API tokens, long strings of letters and digits, and card
  numbers.
- TypeAlike does not save copied text, screen text or anything that you type in a password field.
- When earlier writing is on, TypeAlike keeps at most 10 words of each chat or email message that you answer.
- Earlier writing is off by default in password managers and terminals.
- To delete the earlier writing, open **Settings** > **Personalization** and press **Delete all earlier writing**
  two times.

## Install

> [!WARNING]
> The installers are not signed yet, so Windows and macOS show a warning the first time you open TypeAlike. Download
> TypeAlike only from the [Releases](https://github.com/IsraPerez98/TypeAlike/releases) page.

To check a download, compare the SHA-256 digest of the file on the release page with the output of
`Get-FileHash <file>` (Windows) or `shasum -a 256 <file>` (macOS).

### Windows

You need Windows 10 version 1809 or later, or Windows 11, on x64. To read screen text from an image of a window,
TypeAlike needs Windows 11.

| File | Use it if |
|---|---|
| `TypeAlike-X.Y.Z-windows-x64-vulkan-setup.exe` | You are not sure. It runs the model on the processor, or on a graphics card through Vulkan. |
| `TypeAlike-X.Y.Z-windows-x64-cuda-setup.exe` | You have an NVIDIA graphics card with a recent driver. It also has Vulkan, and it is much larger. |

1. Run the installer. It installs TypeAlike for your user account only, in `%LOCALAPPDATA%\Programs\TypeAlike`.
2. If SmartScreen shows "Windows protected your PC", click **More info**, then click **Run anyway**.
3. Smart App Control can block programs without a signature, and it shows no "Run anyway" button. If it blocks
   TypeAlike, the only fix is to turn off Smart App Control (**Windows Security** > **App & browser control** >
   **Smart App Control settings**).

On Windows, TypeAlike needs no permissions. To update, run the new installer. It keeps your settings, models and
earlier writing. To switch between Vulkan and CUDA, run the other installer.

### macOS

You need macOS 15 or later on a Mac with Apple silicon. Intel Macs are not supported.

1. Open `TypeAlike-X.Y.Z-macos-arm64.dmg` and drag TypeAlike to Applications.
2. Open TypeAlike. macOS blocks it and shows a warning. Close the warning.
3. Open **System Settings** > **Privacy & Security**, then click **Open Anyway** next to the message about TypeAlike.
4. Confirm in the next window. On macOS 15 and later, Control-click > **Open** does not skip this check.
5. Give the permissions when the setup asks for them.

To update, quit TypeAlike and replace the app in Applications. If macOS blocks it again, repeat steps 2 to 4. Then
give the permissions again (see below).

## Permissions on macOS

| Permission | Why TypeAlike needs it | Needed |
|---|---|---|
| Accessibility | To read the text and the caret where you type, and to type a suggestion when you press **Tab**. | Required |
| Screen & System Audio Recording | To read the text of an app that does not give it, from an image of the window. | Optional |

Each build has a different ad-hoc signature, so macOS does not keep the permissions after an update. To give them
again:

1. Open TypeAlike. Without the Accessibility permission, it opens **Settings** > **Permissions**.
2. Click **Give access** for each permission. TypeAlike removes the old entry, and macOS asks again.
3. If System Settings asks you to quit and reopen TypeAlike, do it.

If this does not help, open **System Settings** > **Privacy & Security** > **Accessibility**. Select TypeAlike, click
the minus button, then open TypeAlike again.

## First start and models

The setup opens the first time you start TypeAlike. It asks for the language, your name and notes, what suggestions
can use, the permissions on macOS, and the model. It also turns on **Open at login**, and you can turn it off there.

The setup recommends a model for your computer, and you can download it with one click. On the processor, this is
Qwen3.5 0.8B. On a graphics card or a Mac, it is the largest of Gemma 4 E4B, Gemma 4 E2B and Qwen3.5 2B that fits
in the free graphics memory.

| Model | Download size | Developer |
|---|---|---|
| Qwen3.5 0.8B | 0.5 GB | Alibaba |
| Qwen3.5 2B | 1.2 GB | Alibaba |
| Gemma 4 E2B | 2.6 GB | Google |
| Qwen3.5 4B | 2.6 GB | Alibaba |
| Gemma 4 E4B | 4.2 GB | Google |
| Qwen3.5 9B | 5.4 GB | Alibaba |

**Settings** > **Models** lists these models, and also searches Hugging Face for other GGUF models. On the processor,
suggestions are short or very short, so that they show sooner.

## Uninstall

**Windows:** in Windows Settings, open **Apps** and uninstall TypeAlike. The uninstaller also removes the
**Open at login** entry. To remove your data, delete `%LOCALAPPDATA%\TypeAlike`. It holds the settings, the
models, the earlier writing and its key.

**macOS:** quit TypeAlike and move it from Applications to the Trash. Then remove the data, the login item, the key
that encrypts your earlier writing, and the permissions:

```sh
rm -rf ~/Library/Application\ Support/TypeAlike
rm -f ~/Library/LaunchAgents/io.github.israperez98.typealike.plist
security delete-generic-password -s io.github.israperez98.typealike -a history-key
tccutil reset Accessibility io.github.israperez98.typealike
tccutil reset ScreenCapture io.github.israperez98.typealike
```

## Troubleshooting

- **No suggestion shows in an app.** Click the badge at the caret, or open **Settings** > **Apps and websites**. Make
  sure that suggestions are on for that app or website. On macOS, also check the Accessibility permission.
- **Suggestions stopped.** **Settings** > **Models** tells the reason. If you paused suggestions, select
  **Resume suggestions** in the tray menu, the menu bar menu or the badge menu.
- **Suggestions are slow.** Select a smaller model in **Settings** > **Models**. On Windows, you can also run the
  model on the graphics card.
- **The log.** TypeAlike writes `TypeAlike.log` in the temp folder: `%TEMP%` on Windows, `$TMPDIR` on macOS. Add
  the lines about the problem to your bug report.

## Build from source

You need [rustup](https://rustup.rs/) and [CMake](https://cmake.org/). On Windows, also install the MSVC build tools,
LLVM (for libclang) and the Windows App Runtime 2. On macOS, also install the Xcode Command Line Tools.

```sh
git clone https://github.com/IsraPerez98/TypeAlike.git
cd TypeAlike
cargo run --release
```

On Windows, this build runs the model only on the processor. [CONTRIBUTING.md](CONTRIBUTING.md) gives the GPU builds,
the local build settings and the checks.

## Contributing

To report a bug or ask a question, open an [issue](https://github.com/IsraPerez98/TypeAlike/issues). Before you open
a pull request, read [CONTRIBUTING.md](CONTRIBUTING.md).

## License

TypeAlike is free software under the GNU General Public License, version 3 or later (GPL-3.0-or-later). See
[LICENSE](LICENSE). It runs the models with [llama.cpp](https://github.com/ggml-org/llama.cpp), and its Windows
parts use [windows-rs](https://github.com/microsoft/windows-rs).
