//! Native (Win32) styling for the overlay window that GPUI doesn't expose.
//!
//! The only module in the workspace allowed to use `unsafe` (the workspace denies
//! `unsafe_code`). Every Win32 call is an `unsafe fn` in the `windows` crate; each gets its own
//! block and `// SAFETY:` comment. Shared invariant for all of them: `hwnd` comes from GPUI's
//! live window via `raw-window-handle`, these functions are only called from the overlay's
//! `open_window` build closure (window alive, UI thread), and the handle is never stored.
#![allow(unsafe_code)]

use std::ffi::c_void;

use gpui_kit::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::{
    COLORREF, ERROR_SUCCESS, GetLastError, HWND, SetLastError, WIN32_ERROR,
};
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
    // SAFETY: valid live `hwnd` (module invariant). The pointer is to `policy`, a local that
    // outlives the call, and the byte count is the size of that same type.
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_NCRENDERING_POLICY,
            &policy as *const DWMNCRENDERINGPOLICY as *const c_void,
            size_of::<DWMNCRENDERINGPOLICY>() as u32,
        )
    }
    .map_err(|e| format!("DwmSetWindowAttribute: {e}"))
}

/// Let mouse clicks pass through the overlay to whatever window is underneath.
pub fn enable_click_through(window: &Window) -> Result<(), String> {
    let hwnd = hwnd(window)?;

    // SAFETY: valid live `hwnd` (module invariant); no pointers involved.
    let style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
    // OR our bits in so GPUI's own extended styles (topmost, toolwindow, ...) survive.
    let new_style = style | (WS_EX_LAYERED.0 | WS_EX_TRANSPARENT.0) as isize;

    // `SetWindowLongPtrW` returns the previous value, so 0 means failure only if the thread's
    // last-error code changed; clear it first so a stale code can't masquerade as a failure.
    // SAFETY: only writes this thread's last-error slot.
    unsafe { SetLastError(ERROR_SUCCESS) };
    // SAFETY: valid live `hwnd` (module invariant); no pointers involved.
    let previous = unsafe { SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_style) };
    if previous == 0 {
        // SAFETY: only reads this thread's last-error slot.
        let WIN32_ERROR(code) = unsafe { GetLastError() };
        if code != 0 {
            return Err(format!(
                "SetWindowLongPtrW(GWL_EXSTYLE) failed: Win32 error {code}"
            ));
        }
    }

    // A layered window stays invisible until its attributes are set; 255 = fully opaque, so
    // GPUI's own per-pixel alpha is what shows.
    // SAFETY: valid live `hwnd` (module invariant), now layered; no pointers involved.
    unsafe { SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA) }
        .map_err(|e| format!("SetLayeredWindowAttributes: {e}"))
}
