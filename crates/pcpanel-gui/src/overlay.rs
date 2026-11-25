//! The always-on-top volume overlay: opened on demand, updated while visible, removed shortly
//! after the last change.

use std::time::Duration;

use gpui_kit::*;

pub const OVERLAY_SIZE: Size<Pixels> = Size {
    width: px(320.),
    height: px(64.),
};
/// Gap between the overlay's bottom edge and the bottom of the display's visible area.
pub const BOTTOM_MARGIN: Pixels = px(48.);

/// Where the overlay sits on a display: centred horizontally, just above the bottom of
/// `visible` (the display's area minus the taskbar). Logical pixels.
pub fn overlay_bounds(visible: Bounds<Pixels>) -> Bounds<Pixels> {
    let x = visible.origin.x + (visible.size.width - OVERLAY_SIZE.width) / 2.;
    let y = visible.origin.y + visible.size.height - BOTTOM_MARGIN - OVERLAY_SIZE.height;
    Bounds::new(point(x, y), OVERLAY_SIZE)
}

/// How long the overlay stays up after the last change.
pub const HIDE_AFTER: Duration = Duration::from_millis(1500);

/// Owns the overlay window's lifecycle. At most one overlay window exists at a time.
pub struct VolumeOverlay {
    window: Option<WindowHandle<OverlayView>>,
    /// Pending hide. Replacing it drops (and so cancels) the previous one.
    hide_task: Option<Task<()>>,
}

impl VolumeOverlay {
    pub fn new() -> Self {
        Self {
            window: None,
            hide_task: None,
        }
    }

    /// Show the overlay at `level` (0.0–100.0), opening it if needed, and restart the hide timer.
    pub fn show(&mut self, level: f32, cx: &mut Context<Self>) {
        // Updating fails if the window is gone; then open a fresh one.
        let updated = self.window.is_some_and(|handle| {
            handle
                .update(cx, |view, _, cx| {
                    view.level = level;
                    cx.notify();
                })
                .is_ok()
        });
        if !updated {
            self.open(level, cx);
        }

        self.hide_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(HIDE_AFTER).await;
            // The app may be shutting down; nothing to do if this entity is gone.
            let _ = this.update(cx, |this, cx| this.hide(cx));
        }));
    }

    fn open(&mut self, level: f32, cx: &mut Context<Self>) {
        let Some(display) = cx.primary_display() else {
            eprintln!("volume overlay: no primary display");
            return;
        };
        let options = WindowOptions {
            kind: WindowKind::PopUp,
            focus: false,
            show: true,
            titlebar: None,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            window_background: WindowBackgroundAppearance::Transparent,
            window_bounds: Some(WindowBounds::Windowed(overlay_bounds(
                display.visible_bounds(),
            ))),
            ..Default::default()
        };
        // Plain GPUI `open_window`, not `gpui_kit::open_window`: the kit wraps views in a Root
        // whose component plugin paints the theme background over the whole window, which
        // defeats the transparent background. The overlay needs no Root (no dialogs, no
        // components), so its view is the window root.
        let opened = cx.open_window(options, |window, cx| {
            #[cfg(windows)]
            {
                use crate::overlay_win32::{enable_click_through, remove_frame};
                if let Err(err) = remove_frame(window) {
                    eprintln!("volume overlay: couldn't remove window frame: {err}");
                }
                if let Err(err) = enable_click_through(window) {
                    eprintln!("volume overlay: couldn't enable click-through: {err}");
                }
            }
            cx.new(|_| OverlayView { level })
        });
        match opened {
            Ok(handle) => self.window = Some(handle),
            Err(err) => eprintln!("volume overlay: failed to open window: {err:#}"),
        }
    }

    fn hide(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.window.take() {
            // Already gone is fine.
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
    }
}

/// The overlay's content: a dark rounded panel with a fill bar and the percentage.
struct OverlayView {
    level: f32,
}

impl Render for OverlayView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let level = self.level.clamp(0., 100.);
        div()
            .size_full()
            .flex()
            .items_center()
            .gap_4()
            .px_5()
            .rounded_xl()
            .bg(rgba(0x1c1c1ef0))
            .child(
                div()
                    .flex_1()
                    .h(px(8.))
                    .rounded_full()
                    .bg(rgb(0x48484a))
                    .child(
                        div()
                            .h_full()
                            .w(relative(level / 100.))
                            .rounded_full()
                            .bg(rgb(0xf2f2f7)),
                    ),
            )
            .child(
                div()
                    .w(px(48.))
                    .text_right()
                    .text_color(rgb(0xf2f2f7))
                    .child(format!("{level:.0}%")),
            )
    }
}

#[cfg(test)]
mod tests {
    // Not `use super::*`: gpui_kit's glob re-exports its own `test` attribute, which would
    // shadow the standard `#[test]`.
    use super::overlay_bounds;
    use gpui_kit::{Bounds, point, px, size};

    #[test]
    fn centred_above_bottom_of_primary_display() {
        let visible = Bounds::new(point(px(0.), px(0.)), size(px(1920.), px(1040.)));
        let b = overlay_bounds(visible);
        assert_eq!(b.size, size(px(320.), px(64.)));
        assert_eq!(b.origin, point(px(800.), px(928.))); // (1920-320)/2, 1040-48-64
    }

    #[test]
    fn respects_non_zero_display_origin() {
        let visible = Bounds::new(point(px(-1280.), px(200.)), size(px(1280.), px(984.)));
        assert_eq!(overlay_bounds(visible).origin, point(px(-800.), px(1072.)));
    }
}
