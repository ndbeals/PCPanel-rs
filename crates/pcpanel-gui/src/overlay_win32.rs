//! Native (Win32) styling for the overlay window that GPUI doesn't expose.

use std::ffi::c_void;

use gpui_kit::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{
    DWMNCRP_DISABLED, DWMNCRENDERINGPOLICY, DWMWA_NCRENDERING_POLICY, DwmSetWindowAttribute,
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
