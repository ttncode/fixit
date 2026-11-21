# Ledger — 20261121-read-docs-design-md-build

Spec: .kiln/work/20261121-read-docs-design-md-build/spec.md (read).
Baseline: full verify red (no Cargo.toml); user chose "Fix it in this work: Task 1 creates Cargo.toml".
Ruling: kiln stack.id set to "node" and cargo called by $HOME/.cargo/bin path — user chose this; kiln ships no Rust stack — cost if wrong: kiln stack features (guards/DNA) mislabelled, none used here.

Pre-flight:
| Consumer | Producer | Interface | Found |
|---|---|---|---|
| T3 config | T2 hotkey | parse(&str)->Result<(Hotkey,String),String>, Hotkey{mods,vk} | consistent |
| T4 gemini | T1 json | parse, escape, Json::{get,at,as_str} | consistent |
| T5 rewrite | T4 gemini/guard | body, check_model, parse_reply, fit, URL_BASE, DEFAULT_MODEL, blocked | consistent |
| T6 tray/main | T3 config, T5 rewrite | parse, path, DEFAULT, resolve, style; run, foreground_exe, Outcome | consistent; Config::style needed by main one-shot (T3 produces it) |
Cross-model doubt skipped: non-interactive context. Browser check not run: no browser UI.
Task 1: complete (tests: kiln verify --phase full --task 1/6 → unit exit 0)
Task 2: complete (tests: kiln verify --phase full --task 2/6 → unit exit 0)
Task 3: Ruling: plan test default_parses renamed default_matches_design_doc; it checks DEFAULT equals design.md's block (passes before parse exists, by design); parse(DEFAULT) is pinned by parses_design_example — cost if wrong: none, both checks kept
Task 3: Ruling: Config.hotkeys is in sorted key order, not file order — toml::Table is a BTreeMap and preserve_order would add indexmap; spec needs no order — cost if wrong: hotkey ids differ from file order, invisible to the user
Task 3: complete (tests: kiln verify --phase full --task 3/6 → unit exit 0)
Task 4: complete (tests: kiln verify --phase full --task 4/6 → unit exit 0)
Task 5: Ruling: exclude-from-history marker is a 4-byte zero block, not a zero-length global — GlobalAlloc(0) handles are unreliable; the format's presence is what Windows reads — cost if wrong: temp text appears in Win+V history.
Task 5: Ruling: clipboard open failure before the copy returns Failed("The clipboard is busy. Try again.") — spec names retry but not the message — cost if wrong: wording only.
Task 5: complete (tests: kiln verify --phase full --task 5/6 → unit exit 0; Windows-only module, compile-checked)
Task 6: Ruling: say! macro dropped; non-Windows main uses println! — on Windows the macro had no caller (unused-macro warning) — cost if wrong: none.
Task 6: Ruling: Cargo features Win32_Graphics_Gdi added (WNDCLASSW needs it); Win32_System_Ole and Win32_Storage_FileSystem removed (unused; check still clean) — cost if wrong: none, compile-checked.
Task 6: Ruling: `fixit a b` (2+ args) → "Usage: fixit [style]", exit 1; missing APPDATA → message, exit 1 — spec silent — cost if wrong: wording.
Task 6: Ruling: State.seen is Option<Option<SystemTime>> so a missing config file shows its error once, not every 2 s — cost if wrong: none.
Task 6: complete (tests: kiln verify --phase full --task 6/6 → unit exit 0; 32 tests; Windows check clean, no warnings)
Handover: Ruling: get_text bounds its NUL scan by GlobalSize — hardening against malformed clipboard data from other apps; same result on valid data — cost if wrong: none.
