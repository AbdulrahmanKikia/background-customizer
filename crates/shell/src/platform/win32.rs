//! The only unsafe Win32 in the shell.
//! Removes the settings window from the taskbar and reparents the wallpaper layer.

use std::time::Duration;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{
    SetLastError, COLORREF, ERROR_CLASS_ALREADY_EXISTS, HINSTANCE, HWND, LPARAM, LRESULT, POINT,
    RECT, WIN32_ERROR, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, InvalidateRect, MonitorFromPoint, UpdateWindow, HBRUSH, MONITORINFO,
    MONITOR_DEFAULTTOPRIMARY,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, FindWindowExW, FindWindowW, GetClientRect,
    GetParent, GetWindowLongPtrW, RegisterClassW, SendMessageTimeoutW, SetLayeredWindowAttributes,
    SetParent, SetWindowLongPtrW, SetWindowPos, ShowWindow, CS_HREDRAW, CS_VREDRAW, GWL_EXSTYLE,
    GWL_STYLE, HCURSOR, HICON, LWA_ALPHA, SMTO_NORMAL, SWP_FRAMECHANGED, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SW_SHOWNOACTIVATE, WM_ERASEBKGND,
    WM_PAINT, WNDCLASSW, WS_CHILD, WS_CLIPSIBLINGS, WS_EX_APPWINDOW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_POPUP, WS_VISIBLE,
};

use crate::desktop::{choose_parent, DesktopSurface, WallpaperParent};
use crate::lifecycle::TaskbarPresence;

/// Progman creates the wallpaper `WorkerW` when it receives this message.
const SPAWN_WORKER: u32 = 0x052C;

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

pub(super) fn attach_surface_layer() -> Result<DesktopSurface, String> {
    let parent = locate()?;
    let layer = create_layer()?;
    let primary = primary_monitor_rect()?;
    if let Err(error) = place_layer(layer, parent, primary) {
        destroy_hwnd(layer);
        return Err(error);
    }
    let width = u32::try_from(primary.right.saturating_sub(primary.left))
        .map_err(|_| "primary monitor width does not fit u32".to_string())?;
    let height = u32::try_from(primary.bottom.saturating_sub(primary.top))
        .map_err(|_| "primary monitor height does not fit u32".to_string())?;
    if width == 0 || height == 0 {
        destroy_hwnd(layer);
        return Err("primary monitor has no area".to_string());
    }
    Ok(DesktopSurface {
        hwnd: raw_hwnd(layer),
        width,
        height,
    })
}

pub(super) fn destroy_layer(raw: isize) -> Result<(), String> {
    let hwnd = hwnd_from_raw(raw)?;
    unsafe { DestroyWindow(hwnd) }.map_err(|error| format!("DestroyWindow failed: {error}"))
}

fn create_layer() -> Result<HWND, String> {
    let module = unsafe { GetModuleHandleW(PCWSTR::null()) }
        .map_err(|error| format!("GetModuleHandleW failed: {error}"))?;
    let instance = HINSTANCE::from(module);
    register_layer_class(instance)?;
    // WS_EX_LAYERED is required on Progman with WS_EX_NOREDIRECTIONBITMAP so the
    // GPU swapchain composites under the transparent icon view.
    let layer = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED,
            w!("BackgroundCustomizerLayer"),
            w!("Background Customizer Layer"),
            WS_POPUP | WS_CLIPSIBLINGS,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance),
            None,
        )
    }
    .map_err(|error| format!("CreateWindowExW failed: {error}"))?;
    unsafe {
        SetLayeredWindowAttributes(layer, COLORREF(0), 255, LWA_ALPHA)
            .map_err(|error| format!("SetLayeredWindowAttributes failed: {error}"))?;
    }
    Ok(layer)
}

fn register_layer_class(instance: HINSTANCE) -> Result<(), String> {
    let class = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(layer_window_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: instance,
        hIcon: HICON::default(),
        hCursor: HCURSOR::default(),
        hbrBackground: HBRUSH::default(),
        lpszMenuName: PCWSTR::null(),
        lpszClassName: w!("BackgroundCustomizerLayer"),
    };
    let atom = unsafe { RegisterClassW(&class) };
    if atom != 0 {
        return Ok(());
    }
    let error = unsafe { windows::Win32::Foundation::GetLastError() };
    if error == ERROR_CLASS_ALREADY_EXISTS {
        return Ok(());
    }
    Err(format!("RegisterClassW failed with error {}", error.0))
}

fn locate() -> Result<WallpaperParent, String> {
    let progman = unsafe { FindWindowW(w!("Progman"), PCWSTR::null()) }
        .map_err(|error| format!("Progman was not found: {error}"))?;
    let _sent = unsafe {
        SendMessageTimeoutW(
            progman,
            SPAWN_WORKER,
            WPARAM(0),
            LPARAM(0),
            SMTO_NORMAL,
            1000,
            None,
        )
    };
    let mut latest = "desktop icons were not found".to_string();
    for attempt in 0..20 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(50));
        }
        match choose_parent(observe(progman)) {
            Ok(parent) => return Ok(parent),
            Err(error) => latest = error,
        }
    }
    Err(latest)
}

fn observe(progman: HWND) -> crate::desktop::ObservedDesktop {
    let no_redirection = has_ex_style(progman, WS_EX_NOREDIRECTIONBITMAP.0);
    if let Some(defview) = largest_child(progman, w!("SHELLDLL_DefView")) {
        return crate::desktop::ObservedDesktop {
            progman: raw_hwnd(progman),
            defview: raw_hwnd(defview),
            defview_parent: raw_hwnd(progman),
            worker_child: largest_child(progman, w!("WorkerW"))
                .map(raw_hwnd)
                .unwrap_or(0),
            worker_after_parent: next_toplevel(progman, w!("WorkerW"))
                .map(raw_hwnd)
                .unwrap_or(0),
            progman_no_redirection: no_redirection,
        };
    }
    let mut hit: Option<ClassicHit> = None;
    let _ = unsafe {
        windows::Win32::UI::WindowsAndMessaging::EnumWindows(
            Some(retain_largest_icon_owner),
            LPARAM((&mut hit as *mut Option<ClassicHit>) as isize),
        )
    };
    match hit {
        Some(found) => crate::desktop::ObservedDesktop {
            progman: raw_hwnd(progman),
            defview: raw_hwnd(found.defview),
            defview_parent: raw_hwnd(found.parent),
            worker_child: 0,
            worker_after_parent: raw_hwnd(found.worker_after),
            progman_no_redirection: no_redirection,
        },
        None => crate::desktop::ObservedDesktop {
            progman: raw_hwnd(progman),
            defview: 0,
            defview_parent: 0,
            worker_child: 0,
            worker_after_parent: 0,
            progman_no_redirection: no_redirection,
        },
    }
}

struct ClassicHit {
    parent: HWND,
    defview: HWND,
    worker_after: HWND,
    area: i64,
}

unsafe extern "system" fn retain_largest_icon_owner(
    hwnd: HWND,
    lparam: LPARAM,
) -> windows::core::BOOL {
    let hit = unsafe { &mut *(lparam.0 as *mut Option<ClassicHit>) };
    let defview =
        match unsafe { FindWindowExW(Some(hwnd), None, w!("SHELLDLL_DefView"), PCWSTR::null()) } {
            Ok(found) => found,
            Err(_) => return true.into(),
        };
    let area = client_area(defview);
    let keep = match hit {
        Some(current) => area > current.area,
        None => true,
    };
    if keep {
        let worker_after =
            unsafe { FindWindowExW(None, Some(hwnd), w!("WorkerW"), PCWSTR::null()) }
                .unwrap_or(HWND::default());
        *hit = Some(ClassicHit {
            parent: hwnd,
            defview,
            worker_after,
            area,
        });
    }
    true.into()
}

fn largest_child(parent: HWND, class_name: PCWSTR) -> Option<HWND> {
    let mut after: Option<HWND> = None;
    let mut best: Option<(HWND, i64)> = None;
    loop {
        let found = unsafe { FindWindowExW(Some(parent), after, class_name, PCWSTR::null()) };
        let Ok(hwnd) = found else {
            break;
        };
        if after == Some(hwnd) {
            break;
        }
        let area = client_area(hwnd);
        let replace = match best {
            Some((_, best_area)) => area > best_area,
            None => true,
        };
        if area > 0 && replace {
            best = Some((hwnd, area));
        }
        after = Some(hwnd);
    }
    best.map(|(hwnd, _)| hwnd)
}

fn next_toplevel(after: HWND, class_name: PCWSTR) -> Option<HWND> {
    unsafe { FindWindowExW(None, Some(after), class_name, PCWSTR::null()) }.ok()
}

fn client_area(hwnd: HWND) -> i64 {
    let mut rect = RECT::default();
    if unsafe { GetClientRect(hwnd, &mut rect) }.is_err() {
        return 0;
    }
    let width = i64::from(rect.right.saturating_sub(rect.left));
    let height = i64::from(rect.bottom.saturating_sub(rect.top));
    width.saturating_mul(height)
}

fn has_ex_style(hwnd: HWND, flag: u32) -> bool {
    let style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
    style & flag != 0
}

fn primary_monitor_rect() -> Result<RECT, String> {
    let monitor = unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        rcMonitor: RECT::default(),
        rcWork: RECT::default(),
        dwFlags: 0,
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return Err("GetMonitorInfoW failed for the primary monitor".to_string());
    }
    Ok(info.rcMonitor)
}

fn place_layer(layer: HWND, parent: WallpaperParent, primary: RECT) -> Result<(), String> {
    match parent {
        WallpaperParent::Worker { hwnd } => {
            let worker = hwnd_from_raw(hwnd)?;
            fit_into_parent(layer, worker, None, primary)
        }
        WallpaperParent::BehindIcons { progman, defview } => {
            let progman = hwnd_from_raw(progman)?;
            let defview = hwnd_from_raw(defview)?;
            // hwndInsertAfter is the window above us: DefView stays on top of the layer.
            fit_into_parent(layer, progman, Some(defview), primary)
        }
    }
}

fn fit_into_parent(
    layer: HWND,
    parent: HWND,
    behind: Option<HWND>,
    primary: RECT,
) -> Result<(), String> {
    make_child(layer)?;
    assign_parent(layer, parent)?;
    let width = primary.right.saturating_sub(primary.left);
    let height = primary.bottom.saturating_sub(primary.top);
    if width <= 0 || height <= 0 {
        return Err("primary monitor has no client area".to_string());
    }
    // Progman / WorkerW cover the virtual screen, so primary screen coords
    // map into the parent client area.
    let flags = SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_FRAMECHANGED;
    unsafe {
        SetWindowPos(
            layer,
            behind,
            primary.left,
            primary.top,
            width,
            height,
            flags,
        )
    }
    .map_err(|error| format!("SetWindowPos failed: {error}"))?;
    let _ = unsafe { ShowWindow(layer, SW_SHOWNOACTIVATE) };
    let _ = unsafe { InvalidateRect(Some(layer), None, true) };
    let _ = unsafe { UpdateWindow(layer) };
    Ok(())
}

fn make_child(layer: HWND) -> Result<(), String> {
    let previous = unsafe { GetWindowLongPtrW(layer, GWL_STYLE) } as u32;
    let updated = (previous & !WS_POPUP.0) | WS_CHILD.0 | WS_CLIPSIBLINGS.0 | WS_VISIBLE.0;
    let updated = isize::try_from(updated).map_err(|_| "window style does not fit".to_string())?;
    write_window_long(layer, GWL_STYLE, updated)
}

fn assign_parent(child: HWND, parent: HWND) -> Result<(), String> {
    unsafe { SetLastError(WIN32_ERROR(0)) };
    if unsafe { SetParent(child, Some(parent)) }.is_err() {
        let error = unsafe { windows::Win32::Foundation::GetLastError() };
        if error.0 != 0 {
            return Err(format!("SetParent failed with error {}", error.0));
        }
    }
    match unsafe { GetParent(child) } {
        Ok(actual) if actual == parent => Ok(()),
        Ok(_) => Err("the wallpaper layer was not reparented".to_string()),
        Err(error) => Err(format!("GetParent failed: {error}")),
    }
}

fn write_window_long(
    hwnd: HWND,
    index: windows::Win32::UI::WindowsAndMessaging::WINDOW_LONG_PTR_INDEX,
    value: isize,
) -> Result<(), String> {
    unsafe { SetLastError(WIN32_ERROR(0)) };
    let written = unsafe { SetWindowLongPtrW(hwnd, index, value) };
    if written == 0 {
        let error = unsafe { windows::Win32::Foundation::GetLastError() };
        if error.0 != 0 {
            return Err(format!("SetWindowLongPtrW failed with error {}", error.0));
        }
    }
    Ok(())
}

fn destroy_hwnd(hwnd: HWND) {
    if hwnd.is_invalid() {
        return;
    }
    let _ = unsafe { DestroyWindow(hwnd) };
}

fn raw_hwnd(hwnd: HWND) -> isize {
    hwnd.0 as isize
}

fn hwnd_from_raw(raw: isize) -> Result<HWND, String> {
    if raw == 0 {
        return Err("window handle is null".to_string());
    }
    Ok(HWND(raw as *mut core::ffi::c_void))
}

unsafe extern "system" fn layer_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        // The GPU swapchain owns the pixels. Skip GDI fills so we do not
        // overwrite the still image after the first present.
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            let mut paint = windows::Win32::Graphics::Gdi::PAINTSTRUCT::default();
            let _hdc = unsafe { windows::Win32::Graphics::Gdi::BeginPaint(hwnd, &mut paint) };
            let _ = unsafe { windows::Win32::Graphics::Gdi::EndPaint(hwnd, &paint) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
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
