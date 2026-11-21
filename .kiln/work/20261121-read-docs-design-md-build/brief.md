# Brief — 20261121-read-docs-design-md-build

## What is asked

Build `fixit` as designed in `docs/design.md`.

- **Who:** the user, who is learning to write English and works across many Windows apps (Slack, Mattermost, Discord web, Chrome, Antigravity, LazyVim, Notepad).
- **Why:** to get grammatically correct sentences from the text they just wrote, in any app, with one hotkey and no copy-paste into a chat.
- **Success:** select text, press a hotkey, and the selection is replaced by the fixed or rewritten version. The clipboard is unchanged afterwards. `Ctrl+Z` brings the original back. Any failure leaves the text untouched and shows a short message.
- **Constraints:** Rust, Windows-native, deps `windows` + `toml` only, HTTP via `curl.exe`, Gemini Flash-Lite free tier, key from `GEMINI_API_KEY` only, paste never type, refuse Windows Terminal, lightweight.
- **Out of scope (design.md "Left out on purpose"):** diff view, explanations / mistake history, offline model, own low-level keyboard hook. The Harper Chrome extension and harper-ls are separate installs, not part of this build.

## What I found

- The repo was empty apart from `docs/design.md`. It was not a git repo; `git init -b main` ran with the user's consent on 2026-11-21.
- There was no Rust toolchain in WSL. rustup installed cargo 1.100.0 plus the `x86_64-pc-windows-gnu` target, with the user's consent.
- `x86_64-w64-mingw32-gcc` is not installed. `cargo check --target x86_64-pc-windows-gnu` works without it; linking a Windows `.exe` from WSL does not. The `.exe` gets built on Windows, the way Chord's AGENTS.md does it ("Run: executed on Windows by the maintainer").
- `../chord` (the user's own project) has patterns to reuse:
  - `Cargo.toml`: edition 2024, `toml = "0.8"`, `windows = "0.58"`, with Windows deps under `cfg(windows)`.
  - `src/update.rs:62`: `curl.exe -sfL --max-time` with `CREATE_NO_WINDOW`.
  - `src/autostart.rs`: HKCU `Run` value for "Start with Windows".
  - `src/main.rs`: `windows_subsystem = "windows"`, a single-instance mutex, `say!`, pure modules testable on Linux, Windows modules gated by `cfg(windows)`.
  - `src/panel.rs`: tray icon via `Shell_NotifyIconW` and a popup menu.
- `kiln blast` matched only `docs/design.md`; no DNA store and no learnings exist.

## Risks the design does not spell out

1. **Held modifiers.** The user is still holding `Ctrl+Alt` from the hotkey when fixit sends `Ctrl+C`, so the app would get `Ctrl+Alt+C`. fixit must wait for the modifiers to be released, or release them, before sending keys.
2. **Clipboard restore timing.** Apps read the clipboard asynchronously after `Ctrl+V`. Restoring too early pastes the old content.
3. **Clipboard history (Win+V).** Without the exclusion format, the temporary contents land in clipboard history and cloud clipboard.
4. **API key exposure.** Passing the key in a `curl.exe` argument shows it in the process list. A header file (`-H @file`) keeps it out of the command line.
5. **Trailing whitespace.** The model often adds or drops a trailing newline. Keep the original's leading and trailing whitespace.
6. **Hotkey conflicts.** `RegisterHotKey` fails when another app owns the combination. That failure must be reported, not silent.

## Still unknown

- How much of the clipboard to save and restore (text only, or every format).
- How new hotkeys added to the config get registered without a restart.
- Open items carried from design.md: whether the hotkeys are free in each app, the real Gemini free-tier limits, idle RAM, and the final name. None of these blocks the build.

## Past learnings

none relevant: the project keeps no learnings (`.kiln/learnings/` does not exist).

## Classification

**full**: a new project with no existing flow to change, confirmed by the user.
