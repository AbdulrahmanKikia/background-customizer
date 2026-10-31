//! The only unsafe Win32 in the shell. Removes the settings window from the taskbar.

use windows::Win32::Foundation::{SetLastError, HWND, WIN32_ERROR};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, SWP_FRAMECHANGED,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
};

use crate::lifecycle::TaskbarPresence;

pub fn omit_from_taskbar(hwnd: isize) -> Result<(), String> {
    if hwnd == 0 {
        return Err("settings window handle is null".to_string());
    }
    let mut taskbar = StyleTaskbar { hwnd };
    taskbar.omit_from_taskbar()
}

struct StyleTaskbar {
    hwnd: isize,
}

impl TaskbarPresence for StyleTaskbar {
    fn omit_from_taskbar(&mut self) -> Result<(), String> {
        unsafe { apply_tool_window(self.hwnd) }
    }
}

unsafe fn apply_tool_window(raw: isize) -> Result<(), String> {
    let hwnd = HWND(raw as *mut core::ffi::c_void);
    let previous = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
    let updated = exstyle_without_taskbar(previous)?;
    unsafe { SetLastError(WIN32_ERROR(0)) };
    let written = unsafe { SetWindowLongPtrW(hwnd, GWL_EXSTYLE, updated) };
    if written == 0 {
        let error = unsafe { windows::Win32::Foundation::GetLastError() };
        if error.0 != 0 {
            return Err(format!("SetWindowLongPtrW failed with error {}", error.0));
        }
    }
    let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED;
    unsafe { SetWindowPos(hwnd, None, 0, 0, 0, 0, flags) }
        .map_err(|error| format!("SetWindowPos failed: {error}"))?;
    Ok(())
}

fn exstyle_without_taskbar(style: isize) -> Result<isize, String> {
    let tool = isize::try_from(WS_EX_TOOLWINDOW.0)
        .map_err(|_| "WS_EX_TOOLWINDOW does not fit isize".to_string())?;
    let app = isize::try_from(WS_EX_APPWINDOW.0)
        .map_err(|_| "WS_EX_APPWINDOW does not fit isize".to_string())?;
    Ok((style | tool) & !app)
}

#[cfg(test)]
mod tests {
    use super::exstyle_without_taskbar;

    #[test]
    fn tool_window_style_is_absent_from_the_taskbar() {
        let app_window = 0x0004_0000isize;
        let updated = exstyle_without_taskbar(app_window).expect("style");
        assert_eq!(updated & app_window, 0);
        assert_ne!(updated & 0x80, 0);
    }
}
