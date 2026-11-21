# fixit — design notes

Record of the planning conversation on 2026-11-21 that led to this project, and the agreed design. `fixit` is a placeholder name.

## Goal

The user is learning English writing. They want a tool that checks the grammar of text they just wrote and suggests a better version, everywhere on their Windows PC. The goal is accurate sentences, not an English-learning app.

## Requirements

1. Check grammar of the current text.
2. Suggest a better version.
3. Automatic: one action, no copy-paste into a chat.
4. Very lightweight and fast.
5. Works globally on this machine: Windows Terminal, Antigravity, LazyVim, Slack, Discord (web), Mattermost desktop, Chrome sites.
6. Style is customizable in settings.
7. Free.
8. Windows-native (WSL is possible but global hotkeys and clipboard need the Windows side).

Clarifications made during the conversation:
- "Offline / local" meant "runs on this machine, works in every app, light and fast", not "no internet". A cloud API is acceptable.
- No local LLM: a cloud API is lighter, faster and rewrites better.

## Research history

### Round 0: well-known tools

| Tool | Offline | Global on Windows | Terminal / nvim | Better version |
|---|---|---|---|---|
| Grammarly | No | Yes (desktop app) | No | Yes (cloud, paid for best) |
| LanguageTool | Yes, with local Java server | Browser + Office only | nvim via `ltex-ls` | Weak offline |
| LLM chat | Yes, with Ollama | No, copy-paste | No | Yes |

None fits all requirements.

### Round 1: open-source hotkey rewrite tools

- **Writing Tools** (theJayTea): Python + PySide6, GPL-3.0, 2.5k stars. Select → hotkey → Proofread / Rewrite / custom buttons, replaces in place. Gemini, OpenAI-compatible, Ollama.
- **ZyntaxAI**: Rust + Tauri 2, GPL-3.0-or-later, 9 stars, first release Aug 2026. Word-level diff overlay, personas. Downloads hosted off GitHub (zsync.eu).
- **GhostEdit**: Electron, Windows zip ~492 MB, 18 stars. Rejected (heavy).
- **GemType**: Electron, Gemini only, unsigned builds. Rejected.
- **Harper** (Automattic): offline rule-based grammar checker, very fast. Chrome extension (Discord listed; Slack web not confirmed) and `harper-ls` for Neovim. Only fixes errors, no rewrites.

### Round 2: QA findings

- Writing Tools has 72 open issues, several recent Windows hotkey bugs (#278, #277, #275, #270). Demoted.
- Harper Desktop is macOS-only (beta); Windows "Not yet". So desktop Slack and Mattermost get no as-you-type checks.
- All hotkey tools work the same way: simulate `Ctrl+C`, call the AI, simulate `Ctrl+V`.
- **ecornell/ai-tools-ahk** (AutoHotkey, MIT, 92 stars): OpenAI, Azure OpenAI and Gemini; prompts in `settings.ini`. Was the pick for one round.
- Gemini free tier (terms effective 2026-03-23): Google uses unpaid content to improve products and human reviewers may read it. Described as for professional/business use. Free use not allowed to serve EEA/CH/UK users. Flash-Lite limits per third-party blogs (Oct 2026): ~15 RPM, ~500 requests/day; Flash only ~20/day. Official numbers are in AI Studio and were not readable.
- **Danger:** in Windows Terminal, a simulated `Ctrl+C` with no selection sends an interrupt and kills the running command.

### Round 3: the user's own Chord app

The user built [ttncode/chord](https://github.com/ttncode/chord) (local: `../chord`): Rust keyboard-chord launcher for Windows (Caps Lock + Space + letters), per-app TOML chord files, actions `shortcut`, `palette`, `focus`, `mouse`, `hints`, `window`, `shell`. It replaces AutoHotkey on this machine. It has no grammar feature.

Options considered:
1. Add a `rewrite` action inside Chord.
2. A separate project.

Decision: **separate project**. Chord stays a shortcut tool. The new app has its own hotkeys and per-app styles; Chord is only one optional way to trigger it (through its `shell` action).

## Final design

### Components

| Part | Role |
|---|---|
| **fixit** (this project, Rust) | Hotkey fix / rewrite of the selected text in any app, style chosen per app |
| **Gemini Flash-Lite**, free key | The AI engine |
| **Harper Chrome extension** | Offline as-you-type underlines in Chrome |
| **harper-ls** | Same in LazyVim |

### Two modes, one program

1. **Background mode** (default, no arguments): tray app, registers hotkeys from the config with Windows `RegisterHotKey`. Not a low-level keyboard hook, so it can't clash with Chord's Caps Lock handling. Optional start at sign-in.
2. **One-shot mode**: `fixit <style>` (e.g. `fixit fix`) does one rewrite and exits. For Chord or scripts:
   ```toml
   # chord: global.toml
   "/gr" = { name = "Fix grammar", shell = "fixit fix" }
   ```

### Config: `%APPDATA%\fixit\config.toml`

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

Saved changes apply on the next use, without restart.

### Rewrite flow (both modes)

1. Read the foreground process. If it is `windowsterminal.exe`, stop.
2. Save the clipboard, clear it, send `Ctrl+C`. Wait up to 1 s; if nothing was copied, stop.
3. Write the request body to a temp file and send it with `curl.exe -d @file` to Gemini `generateContent`. Key from `GEMINI_API_KEY`, model from `GEMINI_MODEL` (default `gemini-flash-lite-latest`). Timeout 10 s.
4. Put the result on the clipboard, send `Ctrl+V`, restore the old clipboard.
5. On any error: short tray message, the original text stays untouched.

### Rules

- Never read the API key from a config file that may be committed.
- Paste the result, never type it: Vietnamese Telex would alter typed letters.
- Block Windows Terminal (see the Ctrl+C danger above).
- Free tier: never select passwords, tokens or confidential text.

### Stack

- Rust, dependencies `windows` and `toml`. HTTP through `curl.exe` (ships with Windows 10 1803+), the same way Chord's `update.rs` does it. JSON reply parsed minimally.
- Tests: config parsing, request building (quotes, newlines, Vietnamese text), reply parsing.

### Manual test checklist

- [ ] Hotkey fix works in Slack, Mattermost, Discord, Chrome, Antigravity, Notepad.
- [ ] Per-app style override picks the right style.
- [ ] No selection: nothing pasted, text unchanged.
- [ ] Missing `GEMINI_API_KEY`: a clear message.
- [ ] Windows Terminal in front: refused.
- [ ] `Ctrl+Z` restores the original text.
- [ ] Clipboard content is the same before and after.
- [ ] `fixit fix` from a Chord `shell` chord works.

### Left out on purpose

- Diff view of what changed.
- Explanations of each mistake / mistake history.
- Offline model.
- Own low-level keyboard hook.

### Open items

- Check `Ctrl+Alt+G` / `Ctrl+Alt+B` are free in Antigravity, Slack, Chrome; `Ctrl+Alt` can act as AltGr on some layouts.
- Confirm Gemini free-tier limits in AI Studio.
- Measure idle RAM after building.
- Pick the final project name.

## Sources

- [theJayTea/WritingTools](https://github.com/theJayTea/WritingTools) · [issues](https://github.com/theJayTea/WritingTools/issues)
- [TheHolyOneZ/ZyntaxAI](https://github.com/TheHolyOneZ/ZyntaxAI)
- [nareshnavinash/GhostEdit](https://github.com/nareshnavinash/GhostEdit) · [riponcm/GemType](https://github.com/riponcm/GemType)
- [ecornell/ai-tools-ahk](https://github.com/ecornell/ai-tools-ahk)
- [Harper Chrome extension](https://writewithharper.com/docs/integrations/chrome-extension) · [Harper Desktop](https://writewithharper.com/desktop) · [Harper Neovim](https://writewithharper.com/docs/integrations/neovim)
- [Gemini API terms](https://aistudio.google.com/docs/terms) · [Gemini rate limits](https://aistudio.google.com/docs/rate-limits) · [klymentiev: Gemini free tier 2026](https://klymentiev.com/blog/gemini-api-free-tier)
- [Groq rate limits](https://console.groq.com/docs/rate-limits) (fallback provider, not used yet)
