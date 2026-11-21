#![cfg_attr(not(windows), allow(dead_code))]
// A tray app: no console window of its own.
#![cfg_attr(not(test), windows_subsystem = "windows")]

#[cfg(windows)]
mod autostart;
mod config;
mod gemini;
mod guard;
mod hotkey;
mod json;
#[cfg(windows)]
mod rewrite;
#[cfg(windows)]
mod tray;

/// No arguments: tray app. `fixit <style>`: one rewrite, then exit 0 (replaced), 2 (nothing
/// selected) or 1 (error).
fn main() {
    #[cfg(windows)]
    {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let Some(path) = config::path() else {
            tray::message_box("fixit needs the APPDATA environment variable to find its config.");
            std::process::exit(1);
        };
        match args.as_slice() {
            [] => background(path),
            [style] => std::process::exit(one_shot(&path, style)),
            _ => {
                tray::message_box("Usage: fixit [style]");
                std::process::exit(1);
            }
        }
    }
    #[cfg(not(windows))]
    println!("fixit only runs on Windows");
}

#[cfg(windows)]
fn background(path: std::path::PathBuf) {
    use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows::Win32::System::Threading::CreateMutexW;
    use windows::core::w;

    // The handle is leaked on purpose: the mutex must live as long as the process.
    let first = unsafe { CreateMutexW(None, true, w!("Local\\fixit")).is_ok() && GetLastError() != ERROR_ALREADY_EXISTS };
    if !first {
        tray::message_box("fixit is already running. Its icon is in the notification area of the taskbar.");
        return;
    }
    if let Err(e) = config::ensure_file(&path) {
        tray::message_box(&format!("fixit could not create {}: {e}", path.display()));
    }
    tray::run(path);
}

#[cfg(windows)]
fn one_shot(path: &std::path::Path, style: &str) -> i32 {
    use rewrite::Outcome;

    let config = match config::ensure_file(path)
        .map_err(|e| format!("fixit could not create {}: {e}", path.display()))
        .and_then(|()| std::fs::read_to_string(path).map_err(|e| format!("Could not read {}: {e}", path.display())))
        .and_then(|text| config::parse(&text))
    {
        Ok(config) => config,
        Err(e) => {
            tray::message_box(&e);
            return 1;
        }
    };
    let Some(prompt) = config.style(style) else {
        let known: Vec<&str> = config.styles.keys().map(String::as_str).collect();
        tray::message_box(&format!("Unknown style {style:?}. Styles: {}", known.join(", ")));
        return 1;
    };
    let Some(owner) = tray::message_window() else {
        tray::message_box("fixit could not create a window for the clipboard.");
        return 1;
    };
    match rewrite::run(prompt, owner) {
        Outcome::Done => 0,
        Outcome::Nothing => 2,
        Outcome::Failed(message) => {
            tray::message_box(&message);
            1
        }
    }
}
