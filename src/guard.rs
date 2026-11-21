//! Apps fixit refuses to touch.

/// Console hosts, where Ctrl+C with nothing selected interrupts the running command.
pub fn blocked(exe: &str) -> bool {
    const CONSOLES: [&str; 6] =
        ["windowsterminal.exe", "openconsole.exe", "conhost.exe", "cmd.exe", "powershell.exe", "pwsh.exe"];
    CONSOLES.iter().any(|c| c.eq_ignore_ascii_case(exe))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_blocks() {
        for exe in ["WindowsTerminal.exe", "CMD.EXE", "pwsh.exe", "powershell.exe", "conhost.exe", "OpenConsole.exe"] {
            assert!(blocked(exe), "{exe}");
        }
        for exe in ["slack.exe", "chrome.exe", "", "cmd.exe.bak"] {
            assert!(!blocked(exe), "{exe}");
        }
    }
}
