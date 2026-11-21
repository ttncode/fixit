# fixit

Select text in any Windows app, press a hotkey, and fixit replaces it with a grammar-fixed or rewritten version from Gemini. The style depends on the hotkey and on the app in front.

Design notes and the reasoning behind them: [docs/design.md](docs/design.md).

## Setup

1. Install Rust 1.85+ (edition 2024) on Windows. Without Visual Studio Build Tools, use the GNU toolchain in this folder: `rustup override set stable-x86_64-pc-windows-gnu`.
2. Build with `cargo build --release`. The program is `target\release\fixit.exe`.
3. Get a free Gemini API key in Google AI Studio. Set it as a user environment variable, then sign out and back in, or restart Explorer:
   ```
   setx GEMINI_API_KEY "<your key>"
   ```
   Optional: `GEMINI_MODEL` (default `gemini-flash-lite-latest`).
4. Run `fixit.exe`. An icon appears in the notification area. Right-click it for **Open config**, **Start with Windows** and **Quit**.

On the free tier, Google may use what you send to improve its products, and people may read it. Never select passwords, tokens or confidential text.

## Use

| Mode | How | Result |
|---|---|---|
| Background | `fixit` (no arguments) | Tray app; config hotkeys work in every app |
| One-shot | `fixit <style>`, e.g. `fixit fix` | One rewrite of the current selection, then exit |

One-shot exit codes: `0` text replaced, `2` nothing selected, `1` error (shown in a message box).

From a [Chord](https://github.com/ttncode/chord) chord file:

```toml
"/gr" = { name = "Fix grammar", shell = "fixit fix" }
```

`Ctrl+Z` in the app brings the original text back.

## Config

`%APPDATA%\fixit\config.toml`, created with these defaults on first start:

```toml
[hotkeys]
"ctrl+alt+g" = "fix"
"ctrl+alt+b" = "better"

[styles]
fix          = "Fix grammar and spelling only. Keep my words, tone and formatting. Return only the text."
better       = "Rewrite to sound natural and clear, like a native speaker. Keep the meaning. Return only the text."
casual       = "Rewrite as a short, friendly chat message. Return only the text."
professional = "Rewrite as a polite, professional work message. Return only the text."

# Per app: the same hotkey picks a different style for the app in front
[apps."slack.exe"]
"ctrl+alt+b" = "casual"
[apps."outlook.exe"]
"ctrl+alt+b" = "professional"
```

- Hotkeys: `ctrl`, `alt`, `shift`, `win`, plus one key (`a`–`z`, `0`–`9`, `f1`–`f24`, `space`, `enter`, `tab`, `insert`, `delete`, `home`, `end`, `pageup`, `pagedown`, `up`, `down`, `left`, `right`).
- An `[apps]` hotkey must also be in `[hotkeys]`. It only changes the style.
- Saved changes apply within 2 seconds, without a restart. If the file has an error, fixit keeps the last good config and shows the error.

## Behaviour and limits

- fixit is off in terminals (Windows Terminal, cmd, PowerShell, conhost). There, `Ctrl+C` with nothing selected would stop the running command.
- The result is pasted, never typed, so Vietnamese Telex and other IMEs cannot change it.
- Only clipboard **text** is saved and put back. An image or file on the clipboard is lost after a rewrite.
- With nothing selected, VS Code and its forks (Antigravity) copy the whole current line, and fixit would rewrite and paste that line. Set `"editor.emptySelectionClipboard": false` in their settings. Visual Studio and Notepad++ mark such a copy, and fixit ignores it.
- If you switch to another window while fixit waits for Gemini, nothing is pasted.
- Apps running as administrator do not receive fixit's keys, so nothing happens there.
- fixit keeps its own clipboard writes out of clipboard history (Win+V). The app's own copy of your selection may still appear there.
- The API key reaches `curl.exe` through stdin. It is never in a file, an argument or a log.

## Development

| Task | Command |
|---|---|
| Tests (any OS) | `cargo test` |
| Check the Windows build from Linux/WSL | `cargo check --target x86_64-pc-windows-gnu` |

The modules `json`, `hotkey`, `config`, `gemini` and `guard` are pure and tested. `rewrite`, `tray` and `autostart` call Windows and are tested by hand. The checklist is in [docs/design.md](docs/design.md#manual-test-checklist). Also check:

- Switching windows while the request runs: nothing is pasted, and a message appears.
- `fixit fix` from Chord, before the tray app has ever run: the config is created and the rewrite works.
- **Open config** on a PC with no app for `.toml`: Notepad opens.
- A Windows user name with non-ASCII letters (the request file sits in `%TEMP%`).
