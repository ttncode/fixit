//! Hotkey strings from the config, such as `"ctrl+alt+g"`, as `RegisterHotKey` arguments.

pub const MOD_ALT: u32 = 1;
pub const MOD_CONTROL: u32 = 2;
pub const MOD_SHIFT: u32 = 4;
pub const MOD_WIN: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hotkey {
    pub mods: u32,
    pub vk: u32,
}

/// The hotkey and its normalised spelling: lower case, modifiers in the order `ctrl+alt+shift+win`.
pub fn parse(s: &str) -> Result<(Hotkey, String), String> {
    let bad = |why: &str| format!("hotkey {s:?}: {why}");
    let mut mods = 0;
    let mut key: Option<(String, u32)> = None;
    for part in s.split('+').map(|p| p.trim().to_ascii_lowercase()) {
        let flag = match part.as_str() {
            "ctrl" => MOD_CONTROL,
            "alt" => MOD_ALT,
            "shift" => MOD_SHIFT,
            "win" => MOD_WIN,
            _ => 0,
        };
        if flag != 0 {
            if mods & flag != 0 {
                return Err(bad(&format!("{part} given twice")));
            }
            mods |= flag;
        } else if key.is_some() {
            return Err(bad("more than one key"));
        } else {
            let vk = vk(&part).ok_or_else(|| bad(&format!("unknown key {part:?}")))?;
            key = Some((part, vk));
        }
    }
    let (name, vk) = key.ok_or_else(|| bad("no key"))?;
    if mods == 0 {
        return Err(bad("needs ctrl, alt, shift or win"));
    }
    let mut normal: Vec<&str> = [(MOD_CONTROL, "ctrl"), (MOD_ALT, "alt"), (MOD_SHIFT, "shift"), (MOD_WIN, "win")]
        .into_iter()
        .filter(|(flag, _)| mods & flag != 0)
        .map(|(_, word)| word)
        .collect();
    normal.push(&name);
    Ok((Hotkey { mods, vk }, normal.join("+")))
}

fn vk(name: &str) -> Option<u32> {
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            return Some(c.to_ascii_uppercase() as u32);
        }
    }
    if let Some(n) = name.strip_prefix('f').and_then(|n| n.parse::<u32>().ok()) {
        return (1..=24).contains(&n).then(|| 0x6F + n);
    }
    Some(match name {
        "space" => 0x20,
        "enter" => 0x0D,
        "tab" => 0x09,
        "insert" => 0x2D,
        "delete" => 0x2E,
        "home" => 0x24,
        "end" => 0x23,
        "pageup" => 0x21,
        "pagedown" => 0x22,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(s: &str) -> (Hotkey, String) {
        parse(s).unwrap_or_else(|e| panic!("{s:?}: {e}"))
    }

    #[test]
    fn parses_ctrl_alt_g() {
        assert_eq!(ok("ctrl+alt+g"), (Hotkey { mods: MOD_CONTROL | MOD_ALT, vk: 0x47 }, "ctrl+alt+g".into()));
    }

    #[test]
    fn normalises_case_and_spaces() {
        assert_eq!(ok("Alt + CTRL + G"), (Hotkey { mods: MOD_CONTROL | MOD_ALT, vk: 0x47 }, "ctrl+alt+g".into()));
    }

    #[test]
    fn named_keys() {
        assert_eq!(ok("win+f13").0, Hotkey { mods: MOD_WIN, vk: 0x7C });
        assert_eq!(ok("shift+ctrl+space"), (Hotkey { mods: MOD_CONTROL | MOD_SHIFT, vk: 0x20 }, "ctrl+shift+space".into()));
        assert_eq!(ok("ctrl+alt+pagedown").0.vk, 0x22);
        assert_eq!(ok("ctrl+alt+5").0.vk, 0x35);
        assert_eq!(ok("ctrl+f24").0.vk, 0x87);
        assert_eq!(ok("alt+down").0.vk, 0x28);
    }

    #[test]
    fn rejects() {
        for bad in ["", "g", "ctrl+alt", "ctrl+g+h", "ctrl+alt+foo", "ctrl+ctrl+g", "ctrl+f25", "ctrl++g"] {
            let err = parse(bad).expect_err(bad);
            assert!(err.contains(&format!("{bad:?}")), "{err} should quote {bad:?}");
        }
    }
}
