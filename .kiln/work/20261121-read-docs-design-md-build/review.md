# Review — 20261121-read-docs-design-md-build

lenses: reviewer reported, blind-hunter reported, edge-case-hunter reported, verification-gap reported, intent-alignment reported, code-review-specialist reported, security-auditor reported, test-engineer reported, performance reported

Learnings: none. The project keeps no learnings, and no learnings reader ran.
Bar: nothing lowers the bar. Cargo.lock was not scanned (lock file).
Rules: no rule is routed to this change, so there is no checklist.

Scope — predicted 13 · actual 13

Suite after the fix pass: `kiln verify --phase full`, unit exit 0. `cargo test`: 34/34. `cargo check --target x86_64-pc-windows-gnu`: clean, 0 warnings.

## Triage log

Grade = severity × likelihood, set by me after checking each finding at its line. Readers' own grades are disregarded.

| # | Reader(s) | Location | Finding | Verdict | Grade | Route |
|---|---|---|---|---|---|---|
| 1 | reviewer | src/json.rs:226 | The surrogate-pair branch was not exercised: the test's `😀` escapes had been written as literal `é😀`. | true. Mutation `if false` passed 32/32 before the fix. | Important × Likely | fixed |
| 2 | blind-hunter, edge-case-hunter, security-auditor | src/rewrite.rs, `run` before `send_ctrl(VK_V)` | Ctrl+V goes to whichever window has focus after the wait of up to 10 s, including a terminal. | true | Critical × Possible | fixed |
| 3 | reviewer, edge-case-hunter | src/rewrite.rs `wait_for_copy` | With nothing selected, VS Code and its forks (Antigravity, on the user's list), Visual Studio and Notepad++ copy the whole line. It gets rewritten and pasted, so the line appears twice. | true | Important × Likely | fixed (partly, see Rulings) |
| 4 | blind-hunter, edge-case-hunter, reviewer | src/main.rs `one_shot` | `fixit fix` before the tray app has ever run fails with "Could not read …config.toml". | true | Important × Likely | fixed |
| 5 | blind-hunter, edge-case-hunter | src/tray.rs ID_OPEN | **Open config** does nothing on a clean Windows: no app is registered for `.toml`. | true | Important × Likely | fixed |
| 6 | reviewer | src/rewrite.rs sleeps; src/main.rs one-shot owner | The one-shot owner window never processes messages, but Windows sends it `WM_DESTROYCLIPBOARD` when the target app copies. The copy could stall. | maybe-false: depends on whether Windows sends the message synchronously; not testable here | Critical × Possible | fixed: pumping messages is cheap and harmless in both modes |
| 7 | reviewer, code-review-specialist | src/rewrite.rs `ask_gemini` | If writing the body file fails, it is left in `%TEMP%` with the selected text in it, which breaks the spec's "delete whatever the outcome". | true | Important × Possible | fixed |
| 8 | verification-gap, test-engineer | src/config.rs:96, :98 | No test pins how `[apps]` hotkeys and mixed-case exe tables are normalised. | true (gap; the code is right) | Important × Likely (an undetected regression) | fixed (tests added) |
| 9 | test-engineer, blind-hunter, edge-case-hunter, code-review-specialist | src/rewrite.rs:59 | The guard fails open when `foreground_exe()` returns `None`. | true. `PROCESS_QUERY_LIMITED_INFORMATION` succeeds on elevated processes; the lookup fails only for protected processes or paths over 260 characters. | Critical × Unlikely | discuss |
| 10 | security-auditor, blind-hunter, edge-case-hunter | src/guard.rs | The terminal list misses mintty, WezTerm, Alacritty, PuTTY, wsl.exe and others. | true; the spec fixes the list | Important × Unlikely (the user runs Windows Terminal) | note |
| 11 | blind-hunter, edge-case-hunter, code-review-specialist, security-auditor | src/rewrite.rs `ask_gemini` | The key is checked trimmed but sent untrimmed, so a trailing CR or LF would inject a header line. | true; only the user sets the variable | Minor × Possible | note |
| 12 | blind-hunter, edge-case-hunter, reviewer | src/tray.rs `on_hotkey` | A panic in a rewrite thread leaves `BUSY` set, and every hotkey stays dead until restart. | true | Important × Unlikely | note |
| 13 | blind-hunter, edge-case-hunter, reviewer, code-review-specialist | src/tray.rs ID_QUIT | Quitting during a rewrite can leave the clipboard holding the result, and the body file in `%TEMP%`. | true | Minor × Unlikely | note |
| 14 | blind-hunter, edge-case-hunter, test-engineer | src/main.rs, src/tray.rs | `BUSY` is per process, so a one-shot rewrite and a tray rewrite can overlap. | true | Minor × Unlikely | note |
| 15 | blind-hunter, edge-case-hunter, reviewer | src/rewrite.rs `send_ctrl` | UIPI drops input sent to an elevated app, so the press looks like "nothing selected". The `SendInput` result is ignored. | true | Minor × Possible | note (README now says so) |
| 16 | test-engineer, perf, security-auditor, blind-hunter | src/rewrite.rs | No cap on selection size. | true | Minor × Possible | note |
| 17 | security-auditor, blind-hunter | src/rewrite.rs body file | The selection sits in `%TEMP%` for the duration of the call. | true; the spec allows it | Minor × Possible | note |
| 18 | test-engineer, edge-case-hunter, reviewer, code-review-specialist, verification-gap | src/config.rs:93-98 | Duplicates inside one `[apps]` table, and exe tables that differ only by case, are merged silently. | true | Minor × Possible | note |
| 19 | edge-case-hunter, reviewer | src/config.rs vs src/rewrite.rs:53 | `to_ascii_lowercase` and `to_lowercase` disagree for exe names with non-ASCII capitals. | true | Minor × Unlikely | drop |
| 20 | test-engineer, edge-case-hunter | src/hotkey.rs | `ctrl+f01` and `ctrl+f1` slip past the duplicate check. | true | Minor × Unlikely | drop |
| 21 | test-engineer, security-auditor, edge-case-hunter, code-review-specialist | src/json.rs | No limit on nesting depth; leading zeros are accepted. | true; input comes only from Gemini over TLS | Minor × Unlikely | drop |
| 22 | performance | src/tray.rs WATCH_MS | A 2 s config poll wakes the process about 43k times a day. | true; microseconds per tick | Minor × Likely | note |
| 23 | performance | src/rewrite.rs curl | 0.3–0.5 s overhead per rewrite from process start and TLS. | true; this is the chosen design | Minor × Likely | note |
| 24 | performance | Cargo.toml | No `[profile.release]` tuning. | true; not measured | Minor × Possible | note |
| 25 | edge-case-hunter | src/rewrite.rs `wait_for_copy` | A copy that arrives after the 1 s wait can overwrite the restored clipboard. | true | Minor × Unlikely | drop |
| 26 | edge-case-hunter, reviewer | src/rewrite.rs `restore` | Restore failures are ignored with `let _`. | true | Minor × Unlikely | note |
| 27 | edge-case-hunter | src/tray.rs `reload` | Ids shift when a hotkey is re-registered while a press is still queued. | true; it needs an edit and a press within milliseconds | Minor × Unlikely | drop |
| 28 | edge-case-hunter (claim) | plan Task 6 | The plan says "Quit unregisters"; the code does not. | true; the OS frees hotkeys at exit | Minor × Unlikely | note |
| 29 | code-review-specialist | src/gemini.rs:43 | A missing `finishReason` is treated as STOP. | true; a deliberate lenient reading | Minor × Unlikely | note (ruling) |
| 30 | code-review-specialist | src/config.rs:72 | The bad-hotkey error lacks the `hotkeys."…"` prefix. | true; the key is still quoted | Minor × Unlikely | drop |
| 31 | reviewer | src/tray.rs WM_BALLOON | A message forged by another process could crash the tray through its `lparam` pointer. | true; same desktop and integrity level only | Minor × Unlikely | note |
| 32 | reviewer | src/tray.rs:244 | `WM_NULL` is not posted after `TrackPopupMenu`. | true | Minor × Possible | note |
| 33 | reviewer | src/rewrite.rs body path | A non-ASCII `%TEMP%` path may break `curl @file`. | maybe-false: needs a test on Windows | Important × Possible | note (on README's checklist) |
| 34 | blind-hunter, reviewer | src/autostart.rs | The autostart path goes stale after the exe moves. | true | Minor × Unlikely | note |
| 35 | blind-hunter | src/config.rs | Unknown top-level sections (`[hotkey]`) are ignored without a warning. | true | Minor × Possible | note |
| 36 | blind-hunter | src/main.rs:47 | When `CreateMutexW` fails outright, the message wrongly says "already running". | true | Minor × Unlikely | drop |
| 37 | blind-hunter | docs/design.md | The design doc is not updated to match the deviations. | true; spec.md "Changes from design.md" records them | Minor × Likely | deferred |
| 38 | blind-hunter | src/gemini.rs | Code fences, quotes and `thought` parts in the model output are pasted as is. | maybe-false: Flash-Lite does not think by default | Minor × Unlikely | note |
| 39 | blind-hunter | README | Launchers like Chord keep their old environment after `setx`. | true | Minor × Possible | note |
| 40 | intent-alignment | whole change | The tests exercise pure modules; the success criteria live on the Windows surface and are not demonstrated. | true; stated in the spec and plan | Important × Likely | discuss |
| 41 | intent-alignment | intent | The change goes beyond the design's literal text in places: guard list, exit codes, MessageBox in one-shot mode. | true; each is listed in spec.md Changes | — | note |
| 42 | blind-hunter, test-engineer | src/rewrite.rs | Clipboard content that is not text is lost. | true; the user chose text-only | — | note (spec change 5) |
| 43 | blind-hunter | design.md checklist | The checklist lacks items for window switching, elevated apps, a bad config and a taken hotkey. | true | Minor × Likely | fixed (README checklist additions) |

## Fixed

- fixed #1 surrogate-pair test — `parses_unicode_escapes_and_surrogates` rewritten with real `\u` escapes. Proved by mutation: the `if false` branch fails it (33/34); without the mutation, suite 34/34.
- fixed #4 one-shot creates the config — `ensure_file_writes_default_once` RED (not implemented) → GREEN. `main` uses `config::ensure_file` in both modes. Suite 34/34.
- fixed #8 `[apps]` normalisation pinned — `app_overrides_match_any_spelling`. It passed at once, because the code was right. Proved by two mutations: removing `to_ascii_lowercase` fails it, and storing the written key fails it (33/34 each). Suite 34/34.
- fixed #2 paste into a changed window — `run` records `GetForegroundWindow()` before the copy and fails with "The window changed, so nothing was pasted" if it differs before the paste. Windows-only: compile-checked; no test that failed first is possible here.
- fixed #3 whole-line copies — `copied_whole_line()` treats a copy that carries `MSDEVLineSelect` (Visual Studio, Notepad++) as nothing selected. The README tells VS Code and Antigravity users to set `editor.emptySelectionClipboard: false`. Windows-only: compile-checked.
- fixed #5 Open config — if `ShellExecuteW` returns ≤ 32, fixit opens the file with `notepad.exe`. Windows-only: compile-checked.
- fixed #6 one-shot message pump — every wait in `rewrite` goes through `wait()`, which handles this thread's messages. Windows-only: compile-checked.
- fixed #7 body file on write failure — `remove_file` now runs after a failed write too. Windows-only: compile-checked.
- fixed #43 checklist — the README adds four manual checks (window switch, one-shot first run, Open config with no app for `.toml`, non-ASCII profile).

## Rulings

- Ruling: the 5 Windows-only fixes (#2, #3, #5, #6, #7) have no test that failed first. They are Win32 calls that cannot run in WSL, and no mingw linker is installed, so they are compile-checked and put on the manual checklist. Cost if wrong: a fix that does not work, found only when the checklist runs on Windows.
- Ruling: #3 is fixed only for Visual Studio and Notepad++, by the `MSDEVLineSelect` marker. For VS Code and its forks there is a README setting, not code. They mark an empty-selection copy only inside Chromium's custom clipboard format, and I could not check that format's layout here. Cost if wrong: Antigravity users who skip the setting get a duplicated line when they press the hotkey with nothing selected.
- Ruling: #29, a missing `finishReason`, is accepted as STOP. Gemini sends it on every complete candidate, and being strict would refuse good replies if a proxy dropped the field. Cost if wrong: a reply without a reason could be truncated.
- Ruling: #9 (guard fails open when the exe cannot be read) stays as it is for now. Failing closed would refuse protected or long-path apps. Cost if wrong: in that rare case, Ctrl+C reaches a console.

## Discuss (for the gate)

- #9: guard fails open when `foreground_exe()` is `None`. Critical × Unlikely. One line changes it to fail closed.
- #40: none of the success criteria are demonstrated. Proof needs a Windows build plus the design.md checklist and the README additions.

## Deferred

- #37: update docs/design.md "Final design" to match spec.md "Changes from design.md". It is a design record, and this work only reads it.

## Declined to judge

- Other terminal emulators beyond the spec's list (#10): the spec fixes the list, and fixit inside terminals is a non-goal.
- Keeping non-text clipboard formats (#42): the user chose text-only on 2026-11-21.
- AltGr layouts and whether the hotkeys are free in each app: open questions carried from design.md.
- Real Windows behaviour (UIPI, clipboard timing, curl's `-H @-` on the Windows build): not runnable here.
