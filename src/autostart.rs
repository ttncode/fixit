//! "Start with Windows": a value under HKCU\...\Run that launches fixit in background mode.

use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows::core::{HSTRING, PCWSTR, w};

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const VALUE: PCWSTR = w!("fixit");

pub fn enabled() -> bool {
    unsafe { RegGetValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE, RRF_RT_REG_SZ, None, None, None).is_ok() }
}

pub fn set(on: bool) -> Result<(), String> {
    let result = unsafe {
        if on {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let wide = HSTRING::from(format!("\"{}\"", exe.display()));
            let bytes = (wide.len() + 1) * 2;
            RegSetKeyValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE, REG_SZ.0, Some(wide.as_ptr().cast()), bytes as u32)
        } else {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE)
        }
    };
    result.ok().map_err(|e| e.to_string())
}
