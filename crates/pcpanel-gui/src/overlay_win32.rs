//! Native (Win32) styling for the overlay window that GPUI doesn't expose.

use std::ffi::c_void;

use gpui_kit::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::{COLORREF, HWND};
use windows::Win32::Graphics::Dwm::{
    DWMNCRENDERINGPOLICY, DWMNCRP_DISABLED, DWMWA_NCRENDERING_POLICY, DwmSetWindowAttribute,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetWindowLongPtrW, LWA_ALPHA, SetLayeredWindowAttributes, SetWindowLongPtrW,
    WS_EX_LAYERED, WS_EX_TRANSPARENT,
};

fn hwnd(window: &Window) -> Result<HWND, String> {
    // Fully qualified: GPUI's `Window` has an inherent `window_handle()` that returns its own
    // handle type.
    let handle = HasWindowHandle::window_handle(window).map_err(|e| e.to_string())?;
    match handle.as_raw() {
        RawWindowHandle::Win32(h) => Ok(HWND(h.hwnd.get() as *mut c_void)),
        other => Err(format!("unexpected window handle: {other:?}")),
    }
}

/// Turn off the border and drop shadow Windows draws around GPUI's popup window, so only the
/// overlay's own rounded panel is visible.
pub fn remove_frame(window: &Window) -> Result<(), String> {
    let hwnd = hwnd(window)?;
    let policy = DWMNCRP_DISABLED;
    // SAFETY: `hwnd` is GPUI's live window, we're on the UI thread, and `policy` outlives the
    // call with the size we pass.
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_NCRENDERING_POLICY,
            &policy as *const DWMNCRENDERINGPOLICY as *const c_void,
            size_of::<DWMNCRENDERINGPOLICY>() as u32,
        )
    }
    .map_err(|e| e.to_string())
}

/// Let mouse clicks pass through the overlay to whatever window is underneath.
pub fn enable_click_through(window: &Window) -> Result<(), String> {
    let hwnd = hwnd(window)?;
    // SAFETY: `hwnd` is GPUI's live window and we're on the UI thread.
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            style | (WS_EX_LAYERED.0 | WS_EX_TRANSPARENT.0) as isize,
        );
        // A layered window stays invisible until its attributes are set; 255 = fully opaque,
        // so GPUI's own per-pixel alpha is what shows.
        SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA)
    }
    .map_err(|e| e.to_string())
}
