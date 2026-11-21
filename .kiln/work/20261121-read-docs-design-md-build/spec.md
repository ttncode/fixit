# Spec: fixit

Source: `docs/design.md` ("Final design"), plus the decisions below. Where this spec and design.md differ, the difference is listed under **Changes from design.md**.

## Objective

A Windows tray app. The user selects text in any app and presses a hotkey. fixit replaces the selection with a grammar-fixed or rewritten version from Gemini. The style depends on the hotkey and the app in front.

Acceptance criteria:

1. In background mode, each `[hotkeys]` entry in the config is registered with `RegisterHotKey`. Pressing one rewrites the selection in the foreground app with the resolved style.
2. `fixit <style>` does one rewrite with that style and exits. Exit code 0 when the text was replaced, 2 when there was nothing to do (no selection), 1 on any error, including a blocked terminal.
3. `[apps."<exe>"]` overrides the style of a hotkey for that process. Process names match case-insensitively.
4. If the foreground process is a console host (see Rules), fixit does nothing and shows a message.
5. With no selection, nothing is pasted and the text stays as it was.
6. A missing `GEMINI_API_KEY` gives the message `GEMINI_API_KEY is not set` and makes no network call.
7. Afterwards the clipboard holds the same text as before. If it held no text before, it is empty afterwards.
8. On any error, the selection is not changed and a short message is shown.
9. Edits to the saved config apply on the next hotkey press without a restart. That covers styles, app overrides, and added or removed hotkeys.

## Tech Stack

- Rust, edition 2024. cargo 1.100.0 is installed in WSL.
- `toml = "0.8"`. `windows = "0.58"` under `[target.'cfg(windows)'.dependencies]`. These match `../chord`.
- No other crates. HTTP goes through `curl.exe` (Windows 10 1803+). JSON is handled by a small parser of our own in `src/json.rs`.

## Commands

```
Test (all):        cargo test
Windows check:     cargo check --target x86_64-pc-windows-gnu
kiln full step:    cargo test && cargo check --target x86_64-pc-windows-gnu
Build (on Windows): cargo build --release        → target\release\fixit.exe
```

From WSL, building the `.exe` needs `gcc-mingw-w64-x86-64`, which is not installed. The `.exe` is built on Windows. This build does not verify it.

## Project Structure

```
Cargo.toml
src/main.rs       args, mode dispatch, `say!` macro, `#![cfg_attr(not(test), windows_subsystem = "windows")]` (no console window)
src/config.rs     TOML → Config; default config text; resolve(hotkey, exe) → style   (pure)
src/hotkey.rs     "ctrl+alt+g" → (modifiers, virtual key)                          (pure)
src/json.rs       minimal JSON value parser + string escaping                       (pure)
src/gemini.rs     request body, reply → text or error, model-name check, whitespace keep (pure)
src/guard.rs      blocked-process check                                             (pure)
src/rewrite.rs    the rewrite flow (Windows: clipboard, SendInput, curl.exe)        cfg(windows)
src/tray.rs       tray icon, menu, messages, hotkey registration, config watch      cfg(windows)
src/autostart.rs  HKCU Run value                                                    cfg(windows)
```

The pure modules are tested on Linux. The Windows modules are compile-checked only.

## Design

### Modes

- **Background (no arguments):** one instance only, enforced by the named mutex `Local\fixit`. It opens the config, writing the default config first if none exists. It adds a tray icon and registers the hotkeys, then runs the message loop. A second background instance shows "fixit is already running" and exits.
- **One-shot (`fixit <style>`):** loads the config, looks up `<style>` in `[styles]`, and runs the rewrite flow. Per-app overrides do not apply, because the style was named. Errors are shown with `MessageBoxW` (`MB_TOPMOST`), since there is no tray icon. An unknown style lists the known ones. One-shot ignores the single-instance mutex.

### Config (`%APPDATA%\fixit\config.toml`)

The format is the one in design.md. Rules:

- `[hotkeys]`: hotkey string → style name. Only these keys are registered.
- `[styles]`: style name → prompt.
- `[apps."<exe>"]`: hotkey → style name. The hotkey must also appear in `[hotkeys]`; an override cannot register a key on its own. Otherwise RegisterHotKey would swallow that key in every other app.
- Validation errors name the key. The checks: an unknown style, a bad hotkey string, an app hotkey missing from `[hotkeys]`, two strings for the same key combination.
- Hotkey strings are normalised: lower case, modifiers in a fixed order.
- `resolve(hotkey, exe)` returns the app override if there is one, else the global style.
- Reload: every hotkey press re-reads the file. A 2 s `WM_TIMER` compares the file's modified time and re-registers the hotkeys when it changes. An invalid file keeps the last good config and shows the error once per change of modified time, whether the timer or a press found it. A press on an invalid file still rewrites with the last good config.

### Hotkey strings

Modifiers: `ctrl`, `alt`, `shift`, `win`. Exactly one key: `a`–`z`, `0`–`9`, `f1`–`f24`, `space`, `enter`, `tab`, `insert`, `delete`, `home`, `end`, `pageup`, `pagedown`, `up`, `down`, `left`, `right`. At least one modifier is required. Registration uses `MOD_NOREPEAT`. If `RegisterHotKey` fails, the message names the hotkey ("ctrl+alt+g is used by another app"). The other hotkeys still register.

### Rewrite flow

It runs on a worker thread, so the tray stays responsive. A busy flag ignores presses until the current rewrite finishes.

`rewrite::run(style, owner_hwnd) -> Outcome`, where `Outcome` is `Done`, `Nothing` (no selection; silent) or `Failed(message)`. `rewrite` never shows UI. The caller shows `Failed` as a tray balloon (background) or a `MessageBoxW` (one-shot) and maps it to the exit code.

Clipboard access: `OpenClipboard(owner_hwnd)` is retried every 20 ms for up to 500 ms while another app holds it. A NULL owner makes `SetClipboardData` fail after `EmptyClipboard`. Background mode passes the tray HWND; one-shot mode creates a message-only window (`HWND_MESSAGE`).

1. Get the foreground window's process name. If it is blocked (see Rules), stop with "fixit is off in terminals".
2. Wait until Ctrl, Alt, Shift and Win are all released. Poll `GetAsyncKeyState` every 10 ms for up to 1 s. If any are still held, stop with "release the hotkey to continue". Otherwise the app would receive `Ctrl+Alt+C`.
3. Save the clipboard text (`CF_UNICODETEXT`, or none), then empty the clipboard.
4. Send `Ctrl+C` with `SendInput`. Poll every 20 ms for up to 1 s for `CF_UNICODETEXT`. If none arrives, or the text is only whitespace, restore the clipboard and stop silently. A missing selection is not an error.
5. Check `GEMINI_API_KEY` and the model name (`GEMINI_MODEL`, default `gemini-flash-lite-latest`, characters `[A-Za-z0-9._-]` only).
6. Write the JSON body to a temp file in `%TEMP%` with a unique name. Run
   `%SystemRoot%\System32\curl.exe -sS --max-time 10 -X POST -H @- --data-binary @<body> -w "\n%{http_code}" https://generativelanguage.googleapis.com/v1beta/models/<model>:generateContent`
   with `CREATE_NO_WINDOW`. The full path stops a `curl.exe` elsewhere on PATH from running. The headers `x-goog-api-key: <key>` and `Content-Type: application/json` go through stdin (`-H @-`), so the key never touches disk. Delete the body file whatever the outcome.
7. Parse the reply (see Gemini). On error, restore the clipboard and show the message.
8. Put the result on the clipboard, with the `ExcludeClipboardContentFromMonitorProcessing` format so it stays out of Win+V history. Send `Ctrl+V`, wait 500 ms, then restore the saved text (also excluded) or empty the clipboard.

### Gemini

- Body: `{"systemInstruction":{"parts":[{"text":<style>}]},"contents":[{"role":"user","parts":[{"text":<selection>}]}],"generationConfig":{"temperature":0.2}}`. Strings are escaped by `json::escape`: quotes, backslash, control characters as `\n`/`\r`/`\t`/`\uXXXX`, and non-ASCII kept as UTF-8.
- Status 200: join every `candidates[0].content.parts[*].text`. If `finishReason` is not `STOP`, the reply is an error even when text is present ("Gemini returned no text (MAX_TOKENS)"), so a truncated rewrite is never pasted. A missing `candidates` is an error with `promptFeedback.blockReason` as the reason ("Gemini returned no text (SAFETY)"). Empty text is an error.
- Status not 200: use `error.message` if present, else `HTTP <code>`.
- curl failure (timeout, no network): "Could not reach Gemini: <curl stderr's first line>".
- Whitespace: the result's own leading and trailing whitespace is removed, and the original's is put back. A selection ending in a newline keeps it.

### Tray

- Icon: the stock `IDI_APPLICATION`. Tooltip: `fixit`.
- Menu items: `Open config` (opens the file in its default app via `ShellExecuteW`), `Start with Windows` (checkbox, writes the HKCU `Run` value `fixit` → `"<exe>"`), `Quit`.
- Messages: balloon notifications (`NIF_INFO`) with the title `fixit`.

## Rules (from design.md, made exact)

- The API key comes only from the environment. It never appears in the config, a command-line argument or a log.
- The result is pasted, never typed.
- Blocked processes (case-insensitive): `windowsterminal.exe`, `openconsole.exe`, `conhost.exe`, `cmd.exe`, `powershell.exe`, `pwsh.exe`. In each, `Ctrl+C` without a selection interrupts the running command.
- fixit writes no selected text to disk, except the request body file, which is deleted after the call.

## Changes from design.md

1. `--data-binary` instead of `-d`. curl's `-d @file` strips newlines, which would join lines.
2. The key goes to curl through stdin (`-H @-`), not in the URL, an argument or a file, so it never shows in the process list or on disk. curl is called by its full path.
3. The blocked list covers more console hosts than `windowsterminal.exe`: the same Ctrl+C danger applies to each.
4. An `[apps]` hotkey must also be in `[hotkeys]` (reason under Config).
5. Only clipboard text is saved and restored (the user chose this on 2026-11-21). An image or file on the clipboard is lost after a rewrite.

## Testing Strategy

Built-in `#[test]` in each pure module. Cases:

- `config`: parses the design.md example; resolves an app override vs the global style; app names match case-insensitively; errors for an unknown style, a bad hotkey, an app key missing from `[hotkeys]`, and duplicate normalised keys; the default config text parses.
- `hotkey`: valid combos give the right MOD flags and VK; errors for no modifier, two keys, an unknown key, an empty string; case and spacing normalised.
- `json`: parses nested objects and arrays, string escapes including `😀` surrogates, numbers, true/false/null; rejects bad input; escape round-trips quotes, newlines, tabs, control characters and Vietnamese text (`Tiếng Việt có dấu`).
- `gemini`: the body contains the escaped style and text and parses back to the same strings; reply with two parts joined; error reply → message; `promptFeedback.blockReason` with no candidates → error; `MAX_TOKENS` with text → error; whitespace restored; model-name check; split of the `-w` status line from the body.
- `guard`: blocked names in any case; `slack.exe` not blocked.

The Windows modules have no automated tests. They are covered by design.md's manual checklist, run on Windows by the user.

## Boundaries

- Always: run `cargo test` and the Windows check before calling a task done. Keep Windows code behind `cfg(windows)`.
- Ask first: any crate beyond `toml` and `windows`; changes to the config format beyond this spec.
- Never: read the key from a file; type the result with synthetic keystrokes; log selected text.

## Non-goals

Taken from design.md: diff view, explanations, mistake history, offline model, own keyboard hook. Also out: keeping the target app's own `Ctrl+C` copy out of Win+V history (only fixit's own clipboard writes are excluded), an installer, a custom icon, automatic updates, a provider other than Gemini, and fixit inside terminals. LazyVim running in Windows Terminal is covered by harper-ls only.

## Success Criteria

- `cargo test` passes with the cases above.
- `cargo check --target x86_64-pc-windows-gnu` passes with no warnings in fixit's code.
- The manual checklist in design.md passes on Windows. The user runs it; this build cannot.

## Open Questions

None block the build. Carried from design.md: whether the hotkeys are free in each app and AltGr layouts, the real Gemini free-tier limits, idle RAM, and the final name.
