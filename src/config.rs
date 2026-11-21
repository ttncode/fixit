//! `%APPDATA%\fixit\config.toml`: hotkeys, style prompts, and per-app style overrides.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::hotkey::{self, Hotkey};

/// Written on first start when no config exists; the same as the example in docs/design.md.
pub const DEFAULT: &str = r#"[hotkeys]
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
"#;

#[derive(Debug)]
pub struct Config {
    /// (normalised hotkey, parsed hotkey, style name), sorted by the hotkey as written.
    pub hotkeys: Vec<(String, Hotkey, String)>,
    /// Style name → prompt.
    pub styles: BTreeMap<String, String>,
    /// Lower-case exe name → normalised hotkey → style name.
    apps: BTreeMap<String, BTreeMap<String, String>>,
}

impl Config {
    /// The prompt for a normalised hotkey pressed while `exe` (any case) is in front.
    pub fn resolve(&self, hotkey: &str, exe: &str) -> Option<&str> {
        let name = self
            .apps
            .get(&exe.to_ascii_lowercase())
            .and_then(|keys| keys.get(hotkey))
            .or_else(|| self.hotkeys.iter().find(|(key, _, _)| key == hotkey).map(|(_, _, style)| style))?;
        self.style(name)
    }

    pub fn style(&self, name: &str) -> Option<&str> {
        self.styles.get(name).map(String::as_str)
    }
}

pub fn parse(text: &str) -> Result<Config, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let table: toml::Table = toml::from_str(text).map_err(|e| format!("config.toml: {e}"))?;
    let section = |name: &str| -> Result<BTreeMap<String, String>, String> {
        match table.get(name) {
            None => Ok(BTreeMap::new()),
            Some(toml::Value::Table(t)) => strings(t, name),
            Some(_) => Err(format!("config.toml: [{name}] must be a table")),
        }
    };

    let styles = section("styles")?;
    let known = |at: &str, style: &str| {
        if styles.contains_key(style) { Ok(()) } else { Err(format!("config.toml: {at}: unknown style {style:?}")) }
    };

    let mut hotkeys: Vec<(String, Hotkey, String)> = Vec::new();
    let mut spelled: BTreeMap<String, String> = BTreeMap::new();
    for (written, style) in section("hotkeys")? {
        let at = format!("hotkeys.{written:?}");
        let (hotkey, normal) = hotkey::parse(&written).map_err(|e| format!("config.toml: {e}"))?;
        known(&at, &style)?;
        if let Some(first) = spelled.insert(normal.clone(), written.clone()) {
            return Err(format!("config.toml: hotkeys {first:?} and {written:?} are the same key"));
        }
        hotkeys.push((normal, hotkey, style));
    }

    let mut apps = BTreeMap::new();
    match table.get("apps") {
        None => {}
        Some(toml::Value::Table(t)) => {
            for (exe, keys) in t {
                let toml::Value::Table(keys) = keys else {
                    return Err(format!("config.toml: apps.{exe:?} must be a table"));
                };
                let mut map = BTreeMap::new();
                for (written, style) in strings(keys, &format!("apps.{exe:?}"))? {
                    let at = format!("apps.{exe:?}.{written:?}");
                    let (_, normal) = hotkey::parse(&written).map_err(|e| format!("config.toml: {at}: {e}"))?;
                    if !spelled.contains_key(&normal) {
                        return Err(format!("config.toml: {at}: {written:?} must also be in [hotkeys]"));
                    }
                    known(&at, &style)?;
                    map.insert(normal, style);
                }
                apps.insert(exe.to_ascii_lowercase(), map);
            }
        }
        Some(_) => return Err("config.toml: [apps] must be a table".into()),
    }

    Ok(Config { hotkeys, styles, apps })
}

fn strings(table: &toml::Table, at: &str) -> Result<BTreeMap<String, String>, String> {
    table
        .iter()
        .map(|(k, v)| match v {
            toml::Value::String(s) => Ok((k.clone(), s.clone())),
            _ => Err(format!("config.toml: {at}.{k:?} must be text")),
        })
        .collect()
}

/// Writes `DEFAULT` to `path`, creating its folder, unless the file already exists.
pub fn ensure_file(path: &std::path::Path) -> std::io::Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, DEFAULT)
}

/// `None` when `APPDATA` is not set.
pub fn path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("fixit").join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(text: &str) -> String {
        parse(text).expect_err(text)
    }

    #[test]
    fn parses_design_example() {
        let c = parse(DEFAULT).unwrap();
        assert_eq!(c.hotkeys.len(), 2);
        let g = c.hotkeys.iter().find(|(key, _, _)| key == "ctrl+alt+g").unwrap();
        assert_eq!(g.2, "fix");
        assert_eq!(c.styles.len(), 4);
    }

    #[test]
    fn default_matches_design_doc() {
        assert!(include_str!("../docs/design.md").contains(DEFAULT.trim_end()));
    }

    #[test]
    fn resolve_override_and_global() {
        let c = parse(DEFAULT).unwrap();
        assert_eq!(c.resolve("ctrl+alt+b", "Slack.EXE"), c.style("casual"));
        assert!(c.style("casual").unwrap().starts_with("Rewrite as a short, friendly"));
        assert_eq!(c.resolve("ctrl+alt+b", "notepad.exe"), c.style("better"));
        assert_eq!(c.resolve("ctrl+alt+g", "slack.exe"), c.style("fix"));
        assert_eq!(c.resolve("ctrl+alt+x", "notepad.exe"), None);
        assert_eq!(c.style("nope"), None);
    }

    #[test]
    fn app_overrides_match_any_spelling() {
        let c = parse(
            "[styles]\nbetter = \"B\"\ncasual = \"C\"\n[hotkeys]\n\"ctrl+alt+b\" = \"better\"\n[apps.\"Slack.EXE\"]\n\"Alt+Ctrl+B\" = \"casual\"\n",
        )
        .unwrap();
        assert_eq!(c.resolve("ctrl+alt+b", "slack.exe"), Some("C"));
        assert_eq!(c.resolve("ctrl+alt+b", "notepad.exe"), Some("B"));
    }

    #[test]
    fn ensure_file_writes_default_once() {
        let dir = std::env::temp_dir().join(format!("fixit-test-{}", std::process::id()));
        let path = dir.join("sub").join("config.toml");
        let _ = std::fs::remove_dir_all(&dir);
        ensure_file(&path).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), DEFAULT);
        std::fs::write(&path, "[styles]\n").unwrap();
        ensure_file(&path).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[styles]\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn parses_with_bom() {
        assert!(parse(&format!("\u{feff}{DEFAULT}")).is_ok());
    }

    #[test]
    fn rejects_unknown_style() {
        let e = err("[hotkeys]\n\"ctrl+alt+g\" = \"nope\"\n");
        assert!(e.contains("ctrl+alt+g") && e.contains("nope"), "{e}");
        let e = err("[styles]\nfix = \"x\"\n[hotkeys]\n\"ctrl+alt+g\" = \"fix\"\n[apps.\"x.exe\"]\n\"ctrl+alt+g\" = \"nope\"\n");
        assert!(e.contains("x.exe") && e.contains("nope"), "{e}");
    }

    #[test]
    fn rejects_app_key_not_in_hotkeys() {
        let e = err("[styles]\nfix = \"x\"\n[apps.\"x.exe\"]\n\"ctrl+alt+q\" = \"fix\"\n");
        assert!(e.contains("x.exe") && e.contains("ctrl+alt+q"), "{e}");
    }

    #[test]
    fn rejects_duplicate_combo() {
        let e = err("[styles]\nfix = \"x\"\n[hotkeys]\n\"Ctrl+Alt+G\" = \"fix\"\n\"alt+ctrl+g\" = \"fix\"\n");
        assert!(e.contains("Ctrl+Alt+G") && e.contains("alt+ctrl+g"), "{e}");
    }

    #[test]
    fn rejects_bad_hotkey() {
        let e = err("[styles]\nfix = \"x\"\n[hotkeys]\n\"ctrl+alt+foo\" = \"fix\"\n");
        assert!(e.contains("ctrl+alt+foo"), "{e}");
    }

    #[test]
    fn rejects_bad_toml() {
        assert!(err("[hotkeys\n").contains("config.toml"));
        let e = err("[styles]\nfix = 3\n");
        assert!(e.contains("fix"), "{e}");
        assert!(err("hotkeys = 1\n").contains("hotkeys"));
    }

    #[test]
    fn missing_sections_ok() {
        let c = parse("").unwrap();
        assert!(c.hotkeys.is_empty() && c.styles.is_empty());
    }
}
