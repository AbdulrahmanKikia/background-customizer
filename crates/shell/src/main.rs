#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    // Wallpaper rendering already owns a wgpu/DX device. Keep WebView2 off the
    // GPU so the two do not fight (WebView GPU process disconnect looks like a crash).
    // SAFETY: set before any WebView is created; single-threaded at this point.
    unsafe {
        std::env::set_var(
            "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
            "--disable-gpu --disable-gpu-compositing",
        );
    }
    shell_lib::run();
}
