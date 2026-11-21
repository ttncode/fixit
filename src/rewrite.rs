//! One rewrite of the selection in the foreground app: copy, ask Gemini, paste, restore the clipboard.

use std::io::Write as _;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{CloseHandle, GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
    VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetForegroundWindow, GetWindowThreadProcessId, MSG, PM_REMOVE, PeekMessageW, TranslateMessage,
};
use windows::core::{PWSTR, w};

use crate::{gemini, guard};

pub enum Outcome {
    Done,
    /// Nothing was selected: not an error, so nothing is shown.
    Nothing,
    Failed(String),
}

const CF_UNICODETEXT: u32 = 13;
const VK_C: u16 = 0x43;
const VK_V: u16 = 0x56;
const VK_CONTROL: u16 = 0x11;
const MODIFIERS: [i32; 5] = [0x11, 0x12, 0x10, 0x5B, 0x5C];
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Lower-case file name of the foreground window's process, e.g. `slack.exe`.
pub fn foreground_exe() -> Option<String> {
    unsafe {
        let mut pid = 0;
        GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid));
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 260];
        let mut len = buf.len() as u32;
        let result = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(process);
        result.ok()?;
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        Some(path.rsplit('\\').next()?.to_lowercase())
    }
}

/// Never shows UI: the caller turns `Failed` into a tray balloon or a message box.
pub fn run(prompt: &str, owner: HWND) -> Outcome {
    let target = unsafe { GetForegroundWindow() };
    if guard::blocked(&foreground_exe().unwrap_or_default()) {
        return Outcome::Failed("fixit is off in terminals".into());
    }
    // The user may still hold Ctrl+Alt from the hotkey; our Ctrl+C would arrive as Ctrl+Alt+C.
    if !wait_modifiers_released() {
        return Outcome::Failed("release the hotkey to continue".into());
    }
    let saved = match get_text(owner) {
        Ok(saved) => saved,
        Err(e) => return Outcome::Failed(e),
    };
    if let Err(e) = clear(owner) {
        return Outcome::Failed(e);
    }
    let restore = || match &saved {
        Some(text) => set_text(owner, text),
        None => clear(owner),
    };

    send_ctrl(VK_C);
    let Some(text) = wait_for_copy(owner).filter(|t| !t.trim().is_empty()) else {
        let _ = restore();
        return Outcome::Nothing;
    };

    let result = match ask_gemini(prompt, &text) {
        Ok(result) => gemini::fit(&text, &result),
        Err(e) => {
            let _ = restore();
            return Outcome::Failed(e);
        }
    };
    // The user may have switched apps during the request, possibly to a terminal.
    if unsafe { GetForegroundWindow() } != target {
        let _ = restore();
        return Outcome::Failed("The window changed, so nothing was pasted".into());
    }
    if let Err(e) = set_text(owner, &result) {
        let _ = restore();
        return Outcome::Failed(e);
    }
    send_ctrl(VK_V);
    // Apps read the clipboard after the key arrives; restoring at once would paste the old text.
    wait(Duration::from_millis(500));
    let _ = restore();
    Outcome::Done
}

fn wait_modifiers_released() -> bool {
    let held = || MODIFIERS.iter().any(|&vk| unsafe { GetAsyncKeyState(vk) } < 0);
    let start = Instant::now();
    while held() {
        if start.elapsed() > Duration::from_secs(1) {
            return false;
        }
        wait(Duration::from_millis(10));
    }
    true
}

fn wait_for_copy(owner: HWND) -> Option<String> {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(1) {
        if unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT) }.is_ok() {
            if copied_whole_line() {
                return None;
            }
            return get_text(owner).ok().flatten();
        }
        wait(Duration::from_millis(20));
    }
    None
}

/// Visual Studio and Notepad++ copy the current line when nothing is selected, and mark it so.
fn copied_whole_line() -> bool {
    let format = unsafe { RegisterClipboardFormatW(w!("MSDEVLineSelect")) };
    format != 0 && unsafe { IsClipboardFormatAvailable(format) }.is_ok()
}

/// Sleeps while still handling this thread's window messages. In one-shot mode the clipboard
/// owner window lives on this thread, and Windows sends it messages while another app copies.
fn wait(time: Duration) {
    let start = Instant::now();
    loop {
        let mut msg = MSG::default();
        while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        let left = time.saturating_sub(start.elapsed());
        if left.is_zero() {
            return;
        }
        sleep(left.min(Duration::from_millis(10)));
    }
}

fn send_ctrl(vk: u16) {
    let key = |vk: u16, flags: KEYBD_EVENT_FLAGS| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: VIRTUAL_KEY(vk), wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 },
        },
    };
    let none = KEYBD_EVENT_FLAGS(0);
    let inputs = [key(VK_CONTROL, none), key(vk, none), key(vk, KEYEVENTF_KEYUP), key(VK_CONTROL, KEYEVENTF_KEYUP)];
    unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
}

/// Opens the clipboard for `f`, retrying while another app holds it, and always closes it.
fn with_clipboard<T>(owner: HWND, f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    let start = Instant::now();
    while unsafe { OpenClipboard(owner) }.is_err() {
        if start.elapsed() > Duration::from_millis(500) {
            return Err("The clipboard is busy. Try again.".into());
        }
        wait(Duration::from_millis(20));
    }
    let result = f();
    let _ = unsafe { CloseClipboard() };
    result
}

fn get_text(owner: HWND) -> Result<Option<String>, String> {
    with_clipboard(owner, || unsafe {
        let Ok(handle) = GetClipboardData(CF_UNICODETEXT) else { return Ok(None) };
        let memory = HGLOBAL(handle.0);
        let ptr = GlobalLock(memory) as *const u16;
        if ptr.is_null() {
            return Ok(None);
        }
        // Bounded by the block size: another app's clipboard data may lack its terminating NUL.
        let max = GlobalSize(memory) / 2;
        let mut len = 0;
        while len < max && *ptr.add(len) != 0 {
            len += 1;
        }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
        let _ = GlobalUnlock(memory);
        Ok(Some(text))
    })
}

fn clear(owner: HWND) -> Result<(), String> {
    with_clipboard(owner, || unsafe { EmptyClipboard() }.map_err(|e| format!("Could not clear the clipboard: {e}")))
}

/// Also marks the content so Windows keeps it out of clipboard history (Win+V) and cloud sync.
fn set_text(owner: HWND, text: &str) -> Result<(), String> {
    let wide: Vec<u16> = text.encode_utf16().chain([0]).collect();
    with_clipboard(owner, || unsafe {
        EmptyClipboard().map_err(|e| format!("Could not clear the clipboard: {e}"))?;
        put(CF_UNICODETEXT, bytes_of(&wide))?;
        let exclude = RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing"));
        if exclude != 0 {
            put(exclude, &[0; 4])?;
        }
        Ok(())
    })
}

fn bytes_of(wide: &[u16]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(wide.as_ptr().cast(), wide.len() * 2) }
}

/// Copies `data` into a global block and hands it to the clipboard, which then owns it.
unsafe fn put(format: u32, data: &[u8]) -> Result<(), String> {
    unsafe {
        let memory = GlobalAlloc(GMEM_MOVEABLE, data.len()).map_err(|e| format!("Out of memory: {e}"))?;
        let ptr = GlobalLock(memory) as *mut u8;
        if ptr.is_null() {
            let _ = GlobalFree(memory);
            return Err("Out of memory".into());
        }
        std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
        let _ = GlobalUnlock(memory);
        if let Err(e) = SetClipboardData(format, HANDLE(memory.0)) {
            let _ = GlobalFree(memory);
            return Err(format!("Could not write the clipboard: {e}"));
        }
        Ok(())
    }
}

fn ask_gemini(prompt: &str, text: &str) -> Result<String, String> {
    let key = std::env::var("GEMINI_API_KEY").unwrap_or_default();
    if key.trim().is_empty() {
        return Err("GEMINI_API_KEY is not set".into());
    }
    let model = std::env::var("GEMINI_MODEL").unwrap_or_else(|_| gemini::DEFAULT_MODEL.into());
    gemini::check_model(&model)?;

    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let body_path = std::env::temp_dir().join(format!("fixit-{}-{nanos}.json", std::process::id()));
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&body_path)
        .and_then(|mut f| f.write_all(gemini::body(prompt, text).as_bytes()));
    let output = match written {
        Ok(()) => curl(&key, &model, &body_path),
        Err(e) => Err(format!("Could not write the request: {e}")),
    };
    // The body holds the selected text: it must not outlive this call, whatever happened.
    let _ = std::fs::remove_file(&body_path);
    let output = output?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Could not reach Gemini: {}", stderr.lines().next().unwrap_or("curl failed")));
    }
    gemini::parse_reply(&stdout)
}

fn curl(key: &str, model: &str, body_path: &std::path::Path) -> Result<std::process::Output, String> {
    // A bare `curl.exe` would run whatever copy comes first on PATH.
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    let mut child = Command::new(format!(r"{root}\System32\curl.exe"))
        .args(["-sS", "--max-time", "10", "-X", "POST", "-H", "@-", "--data-binary"])
        .arg(format!("@{}", body_path.display()))
        .args(["-w", "\n%{http_code}"])
        .arg(format!("{}{model}:generateContent", gemini::URL_BASE))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("Could not reach Gemini: {e}"))?;
    // The key goes through stdin so it never shows in the process list or on disk.
    if let Some(mut stdin) = child.stdin.take() {
        let _ = write!(stdin, "x-goog-api-key: {key}\nContent-Type: application/json\n");
    }
    child.wait_with_output().map_err(|e| format!("Could not reach Gemini: {e}"))
}
