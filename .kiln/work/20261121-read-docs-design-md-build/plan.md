# fixit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: use kiln-implement to work this plan task by task. Steps use checkbox (`- [ ]`) syntax, and a checked box is a claim kiln re-checks rather than believes.

**Goal:** A Windows tray app and CLI that replaces the selected text in any app with a Gemini rewrite in the style the hotkey and the foreground app pick.

**Architecture:** Pure modules (`json`, `hotkey`, `config`, `gemini`, `guard`) hold every decision that can be tested on Linux. Three `cfg(windows)` modules do the OS work: `rewrite` (clipboard, SendInput, curl.exe), `tray` (message loop, hotkeys, config watch, UI), and `autostart`. `main` dispatches between background mode and one-shot mode.

**Tech Stack:** Rust edition 2024, `toml = "0.8"`, `windows = "0.58"` (Windows only), `curl.exe`.

**Spec:** `.kiln/work/20261121-read-docs-design-md-build/spec.md`. Read it with this plan. Its Rewrite flow, Gemini and Config sections hold the exact values.

## Global Constraints

- Dependencies: `toml = "0.8"`; `windows = "0.58"` under `[target.'cfg(windows)'.dependencies]`. Nothing else.
- Edition 2024. Every Windows API use sits behind `cfg(windows)`. `cargo test` runs on Linux.
- `#![cfg_attr(not(windows), allow(dead_code))]` and `#![cfg_attr(not(test), windows_subsystem = "windows")]` go in `main.rs`, as in `../chord/src/main.rs`.
- The API key comes only from `GEMINI_API_KEY` and reaches curl only through stdin (`-H @-`).
- curl is `%SystemRoot%\System32\curl.exe`, run with `CREATE_NO_WINDOW` (0x0800_0000).
- Model: `GEMINI_MODEL`, default `gemini-flash-lite-latest`, allowed characters `[A-Za-z0-9._-]`.
- Timings: modifier wait 1 s (10 ms poll); copy wait 1 s (20 ms poll); `OpenClipboard` retry 500 ms (20 ms); paste-then-restore 500 ms; curl `--max-time 10`; config watch timer 2 s.
- No selected text and no key in any log, message or file, except the request body file, which is deleted after the call.
- Exit codes for `fixit <style>`: 0 replaced, 2 nothing to do, 1 error.
- Comments: why only (the project's comment-code rule).

## Risk flags

- security: handles a third-party API key, sends user text to a cloud API, runs `curl.exe` as a child process
- public-api: the config file format and the CLI (`fixit <style>`, exit codes) are user-facing contracts
- ui: tray icon, menu, balloon messages, message boxes

Practices applied: security (no key on disk or argv, curl by full path, Gemini output treated as untrusted and only ever pasted as text); API (validate at the boundaries: config, env, Gemini reply; one error type, `String` messages); docs (README with setup, config and commands); UI (native tray menu and balloons, which are keyboard and screen-reader accessible; every error is text, never colour alone). Observability: no telemetry or log file, on purpose. The only diagnostic is the user-visible message, because a log would risk holding selected text.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|---------------|----------------------------|----------------|
| Happy path | text selected, key set, 200 reply | selection replaced, clipboard text restored | — |
| App override | `slack.exe` in front, `ctrl+alt+b` | `casual` prompt used | — |
| Exe name case | `Slack.EXE` | matches `[apps."slack.exe"]` | — |
| No selection | copy yields no text | nothing pasted; one-shot exit 2 | silent |
| Whitespace-only selection | `"  \n"` | same as no selection | silent |
| Terminal in front | `WindowsTerminal.exe`, `cmd.exe`, … | nothing sent | "fixit is off in terminals" |
| Modifiers held | Ctrl still down after 1 s | nothing sent | "release the hotkey to continue" |
| Key missing | `GEMINI_API_KEY` unset or empty | no network call | "GEMINI_API_KEY is not set" |
| Bad model name | `GEMINI_MODEL=a/b?c` | no network call | "GEMINI_MODEL has invalid characters" |
| API error | status 400 with `error.message` | text unchanged | that message |
| Non-JSON error | status 503 with an HTML body | text unchanged | "HTTP 503" |
| Blocked prompt | 200, no `candidates`, `promptFeedback.blockReason: SAFETY` | text unchanged | "Gemini returned no text (SAFETY)" |
| Truncated | 200, `finishReason: MAX_TOKENS` with text | text unchanged | "Gemini returned no text (MAX_TOKENS)" |
| Multi-part reply | two `parts` | texts joined in order | — |
| curl fails | timeout / no network | text unchanged | "Could not reach Gemini: <first stderr line>" |
| Whitespace | `"  hi there\n"` → model `"Hi there."` | `"  Hi there.\n"` | — |
| CRLF | original has `\r\n`, model returns `\n` | result uses `\r\n` | — |
| Vietnamese / quotes / tabs | `Tiếng "Việt"\tok` | body escapes them; parses back the same | — |
| Config BOM | file starts with U+FEFF | parses | — |
| Config: unknown style | `[hotkeys] "ctrl+alt+g" = "nope"` | rejected | `hotkeys."ctrl+alt+g": unknown style "nope"` |
| Config: app key not global | `[apps."x.exe"] "ctrl+alt+q" = "fix"` | rejected | message names the app and the key |
| Config: duplicate combo | `"Ctrl+Alt+G"` and `"alt+ctrl+g"` | rejected | message names both |
| Config: invalid on reload | broken TOML after a good load | last good config kept | error shown once per mtime change |
| Hotkey taken | RegisterHotKey fails | other hotkeys still work | "ctrl+alt+g is used by another app" |
| Unknown one-shot style | `fixit nope` | exit 1 | lists the known styles |

## Review Focus

1. **CRLF text from Windows apps.** Notepad and Outlook copy `\r\n`; Gemini returns `\n`. The user expects line breaks to survive. Pinned by `fit_restores_crlf` (Task 4).
2. **Notepad-saved config with a BOM.** An older Notepad writes UTF-8 with a BOM, and the user expects it to load. Pinned by `parses_with_bom` (Task 3).
3. **A curl body without a trailing newline before the `-w` status.** The status must still split off correctly. Pinned by `split_status_without_newline_in_body` (Task 4).
4. **A JSON reply with `\u` escapes and surrogate pairs** (emoji, accented letters). It must decode exactly. Pinned by `parses_unicode_escapes_and_surrogates` (Task 1).
5. **Hotkey written with spaces or upper case** (`"Ctrl + Alt + G"`). The user expects it to work. Pinned by `normalises_case_and_spaces` (Task 2).

---

### Task 1: Crate scaffold and JSON

**Files:**
- Create: `Cargo.toml`, `src/main.rs`, `src/json.rs`
- Modify: `.gitignore` (add `/target`)

**Acceptance:**
- `cargo test` builds the crate and runs the json tests.
- `json::parse` handles every JSON value type; `json::escape` output parses back to the input.

**Verification:** `cargo test json` → all pass; `cargo check --target x86_64-pc-windows-gnu` → ok.
**Depends on:** none · **Size:** S

**Interfaces:**
- Produces: `pub enum Json { Null, Bool(bool), Num(f64), Str(String), Arr(Vec<Json>), Obj(Vec<(String, Json)>) }`; `pub fn parse(s: &str) -> Result<Json, String>`; `impl Json { pub fn get(&self, key: &str) -> Option<&Json>; pub fn at(&self, i: usize) -> Option<&Json>; pub fn as_str(&self) -> Option<&str> }`; `pub fn escape(s: &str) -> String`, which returns a quoted JSON string literal.
- `main.rs` at this stage: the crate attributes, a `say!` macro like Chord's, `mod json;`, and a `main` that prints "fixit only runs on Windows" on non-Windows.

- [ ] **Step 1: Write failing tests in `src/json.rs`:** `parses_scalars` (`null`, `true`, `-1.5e2` → `Num(-150.0)`, `"x"`); `parses_nested` (`{"a":[1,{"b":"c"}]}`, then `get("a").at(1).get("b").as_str() == Some("c")`); `parses_unicode_escapes_and_surrogates` (`"é😀\n"` → `"é😀\n"`); `rejects_bad_input` (`""`, `"{"`, `"[1,]"`, `"\"abc"`, `"{} x"` → `Err`); `escape_round_trips` (each of `"q\"b\\s"`, `"a\nb\r\tc"`, `"\u{1}"`, `"Tiếng Việt có dấu"` gives `parse(&escape(s)) == Ok(Str(s))`, and `escape("\u{1}")` contains `\u0001`).
- [ ] **Step 2: Run `cargo test json`.** Expected: FAIL (does not compile or tests fail).
- [ ] **Step 3: Implement** a recursive-descent parser over `chars().peekable()`, with whitespace skipping and trailing-garbage rejection. Surrogate pairs combine into one `char`; a lone surrogate is an error. `escape` writes `\"`, `\\`, `\n`, `\r`, `\t`, `\u00XX` for other characters below 0x20, and everything else as is. Write `Cargo.toml` with `[package] name = "fixit"`, `version = "0.1.0"`, `edition = "2024"`, and the deps from Global Constraints. The `windows` features are `Win32_Foundation`, `Win32_System_DataExchange`, `Win32_System_Memory`, `Win32_System_Ole`, `Win32_System_Threading`, `Win32_System_Registry`, `Win32_System_LibraryLoader`, `Win32_UI_Input_KeyboardAndMouse`, `Win32_UI_WindowsAndMessaging`, `Win32_UI_Shell`, `Win32_Storage_FileSystem`, `Win32_Security`. Add any missing feature that `cargo check` names.
- [ ] **Step 4: Run `cargo test json` and the Windows check.** Expected: PASS, check ok.

### Task 2: Hotkey strings

**Files:**
- Create: `src/hotkey.rs`; Modify: `src/main.rs` (`mod hotkey;`)

**Acceptance:**
- Valid strings give Win32 MOD flags and the virtual key, plus a normalised string.
- Invalid strings give an error that quotes the input.

**Verification:** `cargo test hotkey` → all pass.
**Depends on:** Task 1 · **Size:** XS

**Interfaces:**
- Produces: `pub struct Hotkey { pub mods: u32, pub vk: u32 }` (derive `Debug, Clone, Copy, PartialEq, Eq`); `pub const MOD_ALT: u32 = 1; MOD_CONTROL: u32 = 2; MOD_SHIFT: u32 = 4; MOD_WIN: u32 = 8;`; `pub fn parse(s: &str) -> Result<(Hotkey, String), String>`. The `String` is the normalised form: modifiers in the order `ctrl+alt+shift+win`, then the key, all lower case.

- [ ] **Step 1: Write failing tests:** `parses_ctrl_alt_g` (`"ctrl+alt+g"` → `mods == MOD_CONTROL|MOD_ALT`, `vk == 0x47`, normalised `"ctrl+alt+g"`); `normalises_case_and_spaces` (`"Alt + CTRL + G"` → same `Hotkey` and normalised `"ctrl+alt+g"`); `named_keys` (`"win+f13"` → vk `0x7C`; `"shift+ctrl+space"` → `0x20`, normalised `"ctrl+shift+space"`; `"ctrl+alt+pagedown"` → `0x22`; `"ctrl+alt+5"` → `0x35`); `rejects` (`""`, `"g"` with no modifier, `"ctrl+alt"` with no key, `"ctrl+g+h"` with two keys, `"ctrl+alt+foo"`, and `"ctrl+ctrl+g"` → `Err`, each message containing the input).
- [ ] **Step 2: Run `cargo test hotkey`.** Expected: FAIL.
- [ ] **Step 3: Implement `parse`.** Key table per the spec's Hotkey strings list. VKs: `a-z` 0x41–0x5A, `0-9` 0x30–0x39, `f1-f24` 0x70–0x87, `space` 0x20, `enter` 0x0D, `tab` 0x09, `insert` 0x2D, `delete` 0x2E, `home` 0x24, `end` 0x23, `pageup` 0x21, `pagedown` 0x22, `left` 0x25, `up` 0x26, `right` 0x27, `down` 0x28.
- [ ] **Step 4: Run `cargo test hotkey`.** Expected: PASS.

### Task 3: Config

**Files:**
- Create: `src/config.rs`; Modify: `src/main.rs` (`mod config;`)

**Acceptance:**
- The design.md example and `DEFAULT` parse. `resolve` picks the app override, else the global style.
- Each validation error in the matrix is rejected with a message that names the key.

**Verification:** `cargo test config` → all pass.
**Depends on:** Task 2 · **Size:** S

**Interfaces:**
- Consumes: `hotkey::parse`, `hotkey::Hotkey`.
- Produces: `pub struct Config { pub hotkeys: Vec<(String, hotkey::Hotkey, String)>, pub styles: BTreeMap<String, String>, apps: BTreeMap<String, BTreeMap<String, String>> }`. A hotkey entry is (normalised key, parsed, style name). App names are lower-case; app maps go normalised key → style name. Also `pub fn parse(text: &str) -> Result<Config, String>`; `impl Config { pub fn resolve(&self, hotkey: &str, exe: &str) -> Option<&str> }`, which takes a normalised key and any-case exe and returns the style **prompt**; `pub fn style(&self, name: &str) -> Option<&str>`, which returns the prompt; `pub const DEFAULT: &str`; `pub fn path() -> Option<PathBuf>` (`%APPDATA%\fixit\config.toml`, `None` without `APPDATA`).

- [ ] **Step 1: Write failing tests:** `parses_design_example` (the TOML from design.md: 2 hotkeys, 4 styles); `default_parses` (`parse(DEFAULT)` is `Ok` and `DEFAULT` equals design.md's example); `resolve_override_and_global` (`resolve("ctrl+alt+b","Slack.EXE")` returns the casual prompt; `resolve("ctrl+alt+b","notepad.exe")` the better prompt; `resolve("ctrl+alt+g","slack.exe")` the fix prompt; `resolve("ctrl+alt+x","notepad.exe")` is `None`); `parses_with_bom` (`"\u{feff}" + DEFAULT` parses); `rejects_unknown_style`, `rejects_app_key_not_in_hotkeys`, `rejects_duplicate_combo` (`"Ctrl+Alt+G"` and `"alt+ctrl+g"`), `rejects_bad_hotkey`, `rejects_bad_toml` (each `Err`, and the message contains the offending key or app name); `missing_sections_ok` (an empty file gives zero hotkeys and styles).
- [ ] **Step 2: Run `cargo test config`.** Expected: FAIL.
- [ ] **Step 3: Implement `parse`** with `toml::from_str::<toml::Table>` on the text with any BOM stripped. Each of `[hotkeys]`, `[styles]` and `[apps]` is an optional table of strings; a non-string value is an error naming the key. App hotkeys are normalised and must appear in `hotkeys`.
- [ ] **Step 4: Run `cargo test config`.** Expected: PASS.

### Task 4: Gemini request/reply and the terminal guard

**Files:**
- Create: `src/gemini.rs`, `src/guard.rs`; Modify: `src/main.rs` (`mod gemini; mod guard;`)

**Acceptance:**
- `body` produces the spec's JSON, and it parses back to the same strings.
- `parse_reply` maps every Gemini row of the matrix to text or to the exact message.
- `fit` restores whitespace and CRLF; `blocked` matches the six console hosts in any case.

**Verification:** `cargo test gemini guard` → all pass.
**Depends on:** Task 1 · **Size:** S

**Interfaces:**
- Consumes: `json::{parse, escape, Json}`.
- Produces:
  - `pub const URL_BASE: &str = "https://generativelanguage.googleapis.com/v1beta/models/";`
  - `pub const DEFAULT_MODEL: &str = "gemini-flash-lite-latest";`
  - `pub fn body(style: &str, text: &str) -> String`
  - `pub fn check_model(model: &str) -> Result<(), String>`
  - `pub fn parse_reply(curl_stdout: &str) -> Result<String, String>`. Its input is curl's stdout including the `-w "\n%{http_code}"` suffix. It splits at the last `\n`.
  - `pub fn fit(original: &str, result: &str) -> String`
  - `guard::blocked(exe: &str) -> bool`

- [ ] **Step 1: Write failing tests.**
  - `body_round_trips`: style `Fix "it"`, text `Tiếng Việt\n\t"x"`. Parse `body(..)`. Then `systemInstruction.parts[0].text` and `contents[0].parts[0].text` equal the inputs, `contents[0].role == "user"`, and `generationConfig.temperature` is `Num(0.2)`.
  - `reply_joins_parts`: `{"candidates":[{"content":{"parts":[{"text":"A"},{"text":"B"}]},"finishReason":"STOP"}]}\n200` gives `Ok("AB")`.
  - `reply_error_message`: `{"error":{"code":400,"message":"API key not valid."}}\n400` gives `Err("API key not valid.")`.
  - `reply_non_json_error`: `<html>…</html>\n503` gives `Err("HTTP 503")`.
  - `reply_blocked_prompt`: `{"promptFeedback":{"blockReason":"SAFETY"}}\n200` gives `Err("Gemini returned no text (SAFETY)")`.
  - `reply_max_tokens`: `MAX_TOKENS` with text gives `Err("Gemini returned no text (MAX_TOKENS)")`.
  - `reply_empty_text`: `STOP` with `""` gives `Err`.
  - `split_status_without_newline_in_body`: a body with no trailing newline still parses as status 200.
  - `reply_garbage`: `""` and `"abc"` each give `Err`.
  - `fit_keeps_whitespace`: `fit("  hi there\n", " Hi there. \n\n") == "  Hi there.\n"`.
  - `fit_restores_crlf`: `fit("a\r\nb\r\n", "A\nB") == "A\r\nB\r\n"`, and `fit("a\nb", "A\nB") == "A\nB"`.
  - `model_check`: `gemini-flash-lite-latest` is ok; `a/b`, `a?b` and `""` give `Err("GEMINI_MODEL has invalid characters")`.
  - `guard_blocks`: `WindowsTerminal.exe`, `CMD.EXE`, `pwsh.exe`, `powershell.exe`, `conhost.exe` and `OpenConsole.exe` give `true`; `slack.exe` and `chrome.exe` give `false`.
- [ ] **Step 2: Run `cargo test gemini guard`.** Expected: FAIL.
- [ ] **Step 3: Implement.** `fit` trims the result, converts lone `\n` to `\r\n` when the original contains `\r\n`, then wraps it in the original's leading and trailing whitespace. A status that is not 200 uses `error.message` when the body parses, else `HTTP <code>`. A missing or non-numeric status is `Err("Gemini reply was not understood")`.
- [ ] **Step 4: Run `cargo test gemini guard`.** Expected: PASS.

### Task 5: Rewrite flow (Windows)

**Files:**
- Create: `src/rewrite.rs`; Modify: `src/main.rs` (`#[cfg(windows)] mod rewrite;`)

**Acceptance:**
- `run` follows the spec's Rewrite flow steps 1–8 in order, with the timings from Global Constraints.
- It never shows UI and never leaves the request body file behind.

**Verification:** `cargo check --target x86_64-pc-windows-gnu` → ok, no warnings from `src/`. There is no Linux test: this module is all OS calls. It is covered by the manual checklist.
**Depends on:** Tasks 3, 4 · **Size:** S

**Interfaces:**
- Consumes: `guard::blocked`, `gemini::{body, check_model, parse_reply, fit, URL_BASE, DEFAULT_MODEL}`.
- Produces: `pub enum Outcome { Done, Nothing, Failed(String) }`; `pub fn foreground_exe() -> Option<String>`, the exe file name of the foreground window's process (via `GetWindowThreadProcessId`, `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` and `QueryFullProcessImageNameW`); `pub fn run(prompt: &str, owner: HWND) -> Outcome`.

- [ ] **Step 1: Implement the clipboard helpers in `rewrite.rs`:** `open(owner)` with the retry, `get_text() -> Option<String>`, `set_text(&str)` (`GlobalAlloc(GMEM_MOVEABLE)` holding UTF-16 plus NUL, `SetClipboardData(CF_UNICODETEXT)`, plus `RegisterClipboardFormatW("ExcludeClipboardContentFromMonitorProcessing")` with a zero-length global), and `clear()`. Each helper closes the clipboard on every path.
- [ ] **Step 2: Implement `send_ctrl(vk)`** with `SendInput`: Ctrl down, key down, key up, Ctrl up. Also `wait_modifiers_released() -> bool` over `VK_CONTROL`, `VK_MENU`, `VK_SHIFT`, `VK_LWIN` and `VK_RWIN`.
- [ ] **Step 3: Implement `call_gemini(prompt, text) -> Result<String, String>`.** Write the body to `%TEMP%\fixit-<pid>-<nanos>.json`. Spawn curl with the spec's arguments, `stdin` piped with the two header lines, `stdout`/`stderr` piped and `CREATE_NO_WINDOW`. Remove the file before reading the result. A spawn failure or a non-zero exit gives "Could not reach Gemini: <first stderr line>". Otherwise return `parse_reply`.
- [ ] **Step 4: Implement `run`.** The steps go in spec order. The clipboard is restored on every path after step 3. `Nothing` is returned when there is no copy or only whitespace was copied. The result is pasted through `fit(original, result)`.
- [ ] **Step 5: Run `cargo check --target x86_64-pc-windows-gnu` and `cargo test`.** Expected: check ok; tests still pass.

### Task 6: Tray, autostart, main wiring, README

**Files:**
- Create: `src/tray.rs`, `src/autostart.rs`, `README.md`; Modify: `src/main.rs`

**Acceptance:**
- No arguments: single-instance tray with hotkeys, a 2 s config watch, and the `Open config` / `Start with Windows` / `Quit` menu.
- `fixit <style>` runs once with a message-only owner window and exits 0/2/1. Errors go to `MessageBoxW` with `MB_TOPMOST`.
- The README covers setup (Rust on Windows, GNU toolchain, `GEMINI_API_KEY`), the config with the design.md example, both modes, the exit codes, the Chord `shell` line, and the Rules.

**Verification:** `cargo test && cargo check --target x86_64-pc-windows-gnu` → pass, no warnings from `src/`.
**Depends on:** Task 5 · **Size:** M

**Interfaces:**
- Consumes: `config::{parse, path, DEFAULT, Config}`, `rewrite::{run, foreground_exe, Outcome}`.
- Produces: `tray::run(path: PathBuf)`; `tray::message_box(text: &str)`; `autostart::enabled() -> bool`; `autostart::set(on: bool) -> Result<(), String>` (HKCU `Run` value `fixit` → `"<exe>"`, the same pattern as `../chord/src/autostart.rs`).

- [ ] **Step 1: Implement `autostart.rs`** following Chord's.
- [ ] **Step 2: Implement `tray.rs`.**
  - A hidden window owns the tray icon (`IDI_APPLICATION`, tip `fixit`).
  - `WM_HOTKEY` (id = index into `config.hotkeys`): ignored while busy. Otherwise re-read the config, resolve `(normalised, foreground_exe())` to a prompt, and spawn a thread with `rewrite::run(prompt, hwnd)`. A `Failed(msg)` posts a balloon (`NIF_INFO`, title `fixit`).
  - `WM_TIMER` every 2 s: if the file's mtime changed, re-parse it. On `Ok`, unregister the old hotkeys and register the new ones; on `Err`, show it once.
  - `RegisterHotKey(hwnd, i, mods | MOD_NOREPEAT, vk)` failures are each reported as "<key> is used by another app".
  - Right click opens the menu. `Open config` uses `ShellExecuteW(open)`, `Start with Windows` is a checkbox through `autostart`, and `Quit` unregisters, deletes the icon and posts quit.
  - Balloons are posted to the GUI thread from the worker with `PostMessageW` and a boxed string, so only the GUI thread touches the icon.
- [ ] **Step 3: Wire `main.rs`.**
  - No arguments: take the `Local\fixit` mutex (if it is held, `message_box("fixit is already running. Its icon is in the notification area of the taskbar.")` and return). Write `DEFAULT` to `path()` if the file is missing, creating the folder. Then call `tray::run`.
  - One argument: parse the config (on error, message and exit 1). Look up the style (if unknown, "Unknown style \"x\". Styles: a, b, c" and exit 1). Create a message-only window and call `rewrite::run`; map the result to the exit code.
  - On non-Windows, `main` keeps the Task 1 message.
- [ ] **Step 4: Write `README.md`.**
- [ ] **Step 5: Run `cargo test && cargo check --target x86_64-pc-windows-gnu`.** Expected: all tests pass, check ok with no warnings from `src/`.

## Checkpoints

- After Task 4: `cargo test` passes all the pure-module tests.
- After Task 6: the full step (`cargo test && cargo check --target x86_64-pc-windows-gnu`) passes. The Windows `.exe` build and the manual checklist are the user's, on Windows.

## Not covered by this plan

- Building `fixit.exe` from WSL. `gcc-mingw-w64-x86-64` is not installed, and the user builds on Windows.
- Spec success criterion 3 (the manual checklist) cannot run here.
