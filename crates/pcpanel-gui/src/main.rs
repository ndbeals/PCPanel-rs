//! `pcpanel-gui`: graphical client for the daemon. Currently a GPUI proof of concept: a main
//! window plus an always-on-top volume overlay.

mod main_window;

use gpui_kit::*;
// Stub: keeps the declared dependency edges without unused-dependency warnings.
// Remove each once real code uses that crate.
use pcpanel_core as _;
use pcpanel_ipc as _;

use main_window::MainWindow;

fn main() {
    application()
        .with_assets(assets::Assets)
        .run(|cx| {
            init(cx);

            let options = WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some("PCPanel".into()),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(480.), px(360.)),
                    cx,
                ))),
                ..Default::default()
            };
            open_window(options, cx, |window, cx| {
                cx.new(|cx| MainWindow::new(window, cx))
            })
            .expect("failed to open the main window");
        });
}
