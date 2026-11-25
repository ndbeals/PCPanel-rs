//! The normal application window.

use std::time::Duration;

use gpui_kit::component::{
    button::{Button, ButtonVariants},
    label::Label,
    slider::{Slider, SliderEvent, SliderState},
};
use gpui_kit::*;

use crate::overlay::VolumeOverlay;

/// How long "Show overlay in 3s" waits, so the user can switch to another app first.
const DELAYED_SHOW: Duration = Duration::from_secs(3);

pub struct MainWindow {
    clicks: u32,
    slider: Entity<SliderState>,
    overlay: Entity<VolumeOverlay>,
    _subscriptions: Vec<Subscription>,
}

impl MainWindow {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let slider = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(100.)
                .step(1.)
                .default_value(50.)
        });
        let overlay = cx.new(|_| VolumeOverlay::new());

        let subscription = cx.subscribe_in(&slider, window, |this, _, event, _, cx| {
            if let SliderEvent::Change(value) = event {
                let level = value.end();
                this.overlay.update(cx, |overlay, cx| overlay.show(level, cx));
            }
        });

        Self {
            clicks: 0,
            slider,
            overlay,
            _subscriptions: vec![subscription],
        }
    }

    /// Simulates a knob turn while the user is in another app.
    fn show_overlay_later(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DELAYED_SHOW).await;
            let _ = this.update(cx, |this, cx| {
                let level = this.slider.read(cx).value().end();
                this.overlay.update(cx, |overlay, cx| overlay.show(level, cx));
            });
        })
        .detach();
    }
}

impl Render for MainWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .items_center()
            .justify_center()
            .gap_3()
            .child(Label::new("PCPanel-rs — GPUI proof of concept"))
            .child(
                Button::new("click")
                    .primary()
                    .label("Click me")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.clicks += 1;
                        cx.notify();
                    })),
            )
            .child(Label::new(format!("Clicked {} times", self.clicks)))
            .child(div().w(px(300.)).pt_4().child(Slider::new(&self.slider)))
            .child(
                Button::new("delayed")
                    .label("Show overlay in 3s")
                    .on_click(cx.listener(|this, _, _, cx| this.show_overlay_later(cx))),
            )
    }
}
