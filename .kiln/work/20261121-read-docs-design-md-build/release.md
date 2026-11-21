# 20261121-read-docs-design-md-build — fixit, first version

## Change
- Added: select text in any Windows app, press `Ctrl+Alt+G` (fix) or `Ctrl+Alt+B` (better), and fixit replaces it with a Gemini rewrite; `fixit <style>` does the same once, for Chord or scripts.

## Before release
- [x] `cargo test`: 34 passed, 0 failed, run by `kiln verify` in this session (checked)
- [x] `cargo check --target x86_64-pc-windows-gnu`: clean, no warnings (checked)
- [x] The change carries no migration and no database. It reads `GEMINI_API_KEY`, `GEMINI_MODEL` (optional) and `APPDATA` from the environment, and `%APPDATA%\fixit\config.toml`; read in src/rewrite.rs and src/config.rs (checked)
- [ ] `GEMINI_API_KEY` is set as a user environment variable on the Windows PC (confirm)
- [ ] The `.exe` builds on Windows with `cargo build --release`, as README.md says. It was not built here: WSL has no mingw linker (confirm)
- [ ] The manual checklist in docs/design.md and the four extra checks in README.md pass on Windows (confirm)

## Release steps
1. On Windows, in this folder: `cargo build --release` (README.md "Setup").
2. Run `target\release\fixit.exe`. It creates `%APPDATA%\fixit\config.toml` on first start.
3. Optional: tray menu → **Start with Windows**.

## After release
- [ ] The fixit icon is in the notification area, and `Ctrl+Alt+G` fixes a selected sentence in Notepad (confirm)
- [ ] The clipboard text is the same before and after a rewrite (confirm)

## Rollback
- Trigger: a rewrite changes or loses text, the clipboard is wrong afterwards, or a hotkey breaks another app.
- Steps: tray menu → untick **Start with Windows**, then **Quit**; delete `fixit.exe`; optionally delete `%APPDATA%\fixit`.
- Time: three steps, about a minute (confirm)
- Nothing destructive: fixit writes only its own config file and the HKCU `Run` value `fixit`, and both are removed by the steps above.
