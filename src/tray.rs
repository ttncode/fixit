//! Background mode: tray icon and menu, global hotkeys, and the config file watch.

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::SystemTime;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{HOT_KEY_MODIFIERS, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_WARNING, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
    ShellExecuteW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DispatchMessageW, GetCursorPos,
    GetMessageW, HMENU, HWND_MESSAGE, IDI_APPLICATION, LoadIconW, MB_ICONWARNING, MB_OK, MB_TOPMOST, MF_CHECKED,
    MF_SEPARATOR, MF_STRING, MSG, MessageBoxW, PostMessageW, PostQuitMessage, RegisterClassW,
    RegisterWindowMessageW, SW_SHOWNORMAL, SetForegroundWindow, SetTimer, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    TrackPopupMenu, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_HOTKEY, WM_LBUTTONUP, WM_RBUTTONUP,
    WM_TIMER, WNDCLASSW,
};
use windows::core::{HSTRING, PCWSTR, w};

use crate::autostart;
use crate::config::{self, Config};
use crate::rewrite::{self, Outcome};

const WM_TRAY: u32 = WM_APP + 1;
/// lparam is a `Box<String>` from `Box::into_raw`, posted by a rewrite thread.
const WM_BALLOON: u32 = WM_APP + 2;
const ID_OPEN: usize = 1;
const ID_AUTOSTART: usize = 2;
const ID_QUIT: usize = 3;
const WATCH_TIMER: usize = 1;
const WATCH_MS: u32 = 2000;

static BUSY: AtomicBool = AtomicBool::new(false);
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

struct State {
    path: PathBuf,
    /// The last config that parsed; kept when the file later breaks.
    config: Option<Config>,
    /// Modified time of the file at the last read, good or bad (inner `None`: file missing).
    seen: Option<Option<SystemTime>>,
    /// Normalised hotkey for each registered id (the index).
    registered: Vec<String>,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

pub fn message_box(text: &str) {
    unsafe {
        MessageBoxW(None, &HSTRING::from(text), w!("fixit"), MB_OK | MB_ICONWARNING | MB_TOPMOST);
    }
}

/// A window that only exists to own the clipboard in one-shot mode.
pub fn message_window() -> Option<HWND> {
    unsafe {
        let module = GetModuleHandleW(None).ok()?;
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("STATIC"),
            w!("fixit"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            HMENU::default(),
            module,
            None,
        )
        .ok()
    }
}

/// Runs the tray until Quit.
pub fn run(path: PathBuf) {
    unsafe {
        let Ok(module) = GetModuleHandleW(None) else { return };
        let class = w!("fixit-tray");
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: module.into(),
            lpszClassName: class,
            ..Default::default()
        });
        let Ok(hwnd) = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            w!("fixit"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            HWND::default(),
            HMENU::default(),
            module,
            None,
        ) else {
            message_box("fixit could not create its tray window.");
            return;
        };
        let _ = Shell_NotifyIconW(NIM_ADD, &icon_data(hwnd));
        TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), Ordering::Release);

        STATE.with_borrow_mut(|s| *s = Some(State { path, config: None, seen: None, registered: Vec::new() }));
        reload(hwnd);
        SetTimer(hwnd, WATCH_TIMER, WATCH_MS, None);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn modified(path: &PathBuf) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Re-reads the config when its modified time changed since the last read. A bad file keeps
/// the last good config, and its error is shown once per change.
fn reload(hwnd: HWND) {
    let mut notes = Vec::new();
    STATE.with_borrow_mut(|state| {
        let Some(state) = state.as_mut() else { return };
        let now = modified(&state.path);
        if state.seen == Some(now) {
            return;
        }
        state.seen = Some(now);
        let parsed = std::fs::read_to_string(&state.path)
            .map_err(|e| format!("Could not read {}: {e}", state.path.display()))
            .and_then(|text| config::parse(&text));
        match parsed {
            Ok(config) => {
                for id in 0..state.registered.len() {
                    let _ = unsafe { UnregisterHotKey(hwnd, id as i32) };
                }
                state.registered = config.hotkeys.iter().map(|(key, _, _)| key.clone()).collect();
                for (id, (key, hotkey, _)) in config.hotkeys.iter().enumerate() {
                    let mods = HOT_KEY_MODIFIERS(hotkey.mods) | MOD_NOREPEAT;
                    if unsafe { RegisterHotKey(hwnd, id as i32, mods, hotkey.vk) }.is_err() {
                        notes.push(format!("{key} is used by another app"));
                    }
                }
                state.config = Some(config);
            }
            Err(e) => notes.push(e),
        }
    });
    if !notes.is_empty() {
        balloon(hwnd, &notes.join("\n"));
    }
}

fn on_hotkey(hwnd: HWND, id: usize) {
    if BUSY.swap(true, Ordering::AcqRel) {
        return;
    }
    let key = STATE.with_borrow(|s| s.as_ref().and_then(|s| s.registered.get(id).cloned()));
    reload(hwnd);
    let exe = rewrite::foreground_exe().unwrap_or_default();
    let prompt = key.and_then(|key| {
        STATE.with_borrow(|s| s.as_ref()?.config.as_ref()?.resolve(&key, &exe).map(str::to_owned))
    });
    let Some(prompt) = prompt else {
        BUSY.store(false, Ordering::Release);
        return;
    };
    // HWND is not Send; the GUI window outlives every rewrite thread.
    let raw = hwnd.0 as usize;
    std::thread::spawn(move || {
        let hwnd = HWND(raw as *mut _);
        if let Outcome::Failed(message) = rewrite::run(&prompt, hwnd) {
            let boxed = Box::into_raw(Box::new(message));
            if unsafe { PostMessageW(hwnd, WM_BALLOON, WPARAM(0), LPARAM(boxed as isize)) }.is_err() {
                drop(unsafe { Box::from_raw(boxed) });
            }
        }
        BUSY.store(false, Ordering::Release);
    });
}

fn icon_data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut data = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        hIcon: unsafe { LoadIconW(None, IDI_APPLICATION).unwrap_or_default() },
        ..Default::default()
    };
    copy_wide(&mut data.szTip, "fixit");
    data
}

/// Truncates to fit and always leaves a terminating NUL.
fn copy_wide(dst: &mut [u16], text: &str) {
    let max = dst.len() - 1;
    for (d, s) in dst.iter_mut().zip(text.encode_utf16().take(max).chain([0])) {
        *d = s;
    }
}

fn balloon(hwnd: HWND, text: &str) {
    let mut data = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_INFO,
        dwInfoFlags: NIIF_WARNING,
        ..Default::default()
    };
    copy_wide(&mut data.szInfo, text);
    copy_wide(&mut data.szInfoTitle, "fixit");
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
    }
}

fn menu(hwnd: HWND) {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return };
        let autostart_flags = if autostart::enabled() { MF_STRING | MF_CHECKED } else { MF_STRING };
        let _ = AppendMenuW(menu, MF_STRING, ID_OPEN, w!("Open config"));
        let _ = AppendMenuW(menu, autostart_flags, ID_AUTOSTART, w!("Start with Windows"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, MF_STRING, ID_QUIT, w!("Quit"));
        let mut point = Default::default();
        let _ = GetCursorPos(&mut point);
        // Without this the menu does not close when clicking elsewhere (documented TrackPopupMenu quirk).
        let _ = SetForegroundWindow(hwnd);
        let choice = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, point.x, point.y, 0, hwnd, None);
        let _ = DestroyMenu(menu);
        match choice.0 as usize {
            ID_OPEN => {
                if let Some(path) = STATE.with_borrow(|s| s.as_ref().map(|s| s.path.clone())) {
                    let opened =
                        ShellExecuteW(hwnd, w!("open"), &HSTRING::from(path.as_os_str()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
                    // Values up to 32 are errors; a clean Windows has no app for .toml files.
                    if opened.0 as isize <= 32 {
                        let _ = std::process::Command::new("notepad.exe").arg(&path).spawn();
                    }
                }
            }
            ID_AUTOSTART => {
                if let Err(e) = autostart::set(!autostart::enabled()) {
                    balloon(hwnd, &format!("Could not change Start with Windows: {e}"));
                }
            }
            ID_QUIT => {
                let _ = Shell_NotifyIconW(NIM_DELETE, &icon_data(hwnd));
                PostQuitMessage(0);
            }
            _ => {}
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg != 0 && msg == TASKBAR_CREATED.load(Ordering::Acquire) {
        let _ = unsafe { Shell_NotifyIconW(NIM_ADD, &icon_data(hwnd)) };
        return LRESULT(0);
    }
    match msg {
        WM_HOTKEY => on_hotkey(hwnd, wparam.0),
        WM_TIMER if wparam.0 == WATCH_TIMER => reload(hwnd),
        WM_BALLOON => {
            let message = unsafe { Box::from_raw(lparam.0 as *mut String) };
            balloon(hwnd, &message);
        }
        WM_TRAY => {
            let event = (lparam.0 & 0xFFFF) as u32;
            if event == WM_RBUTTONUP || event == WM_LBUTTONUP {
                menu(hwnd);
            }
        }
        _ => return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
    LRESULT(0)
}
