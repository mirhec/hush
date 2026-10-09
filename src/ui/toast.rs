use super::theme::Palette;
use eframe::egui::{self, Align2, RichText, Stroke, vec2};
use std::time::{Duration, Instant};

pub(super) struct Toast {
    text: String,
    error: bool,
    expires: Instant,
}

impl Toast {
    pub(super) fn new(text: String, error: bool) -> Self {
        Self {
            text,
            error,
            expires: Instant::now() + Duration::from_secs(if error { 10 } else { 4 }),
        }
    }

    pub(super) fn show(current: &mut Option<Self>, ctx: &egui::Context, p: Palette) {
        Self::show_at(current, ctx, p, Instant::now());
    }

    fn show_at(current: &mut Option<Self>, ctx: &egui::Context, p: Palette, now: Instant) {
        let Some(toast) = current else { return };
        if now >= toast.expires {
            *current = None;
            return;
        }
        ctx.request_repaint_after(toast.expires - now);
        let mut close = false;
        egui::Area::new(egui::Id::new("feedback-toast"))
            .order(egui::Order::Foreground)
            .anchor(Align2::RIGHT_BOTTOM, vec2(-16., -16.))
            .interactable(true)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(p.hover)
                    .stroke(Stroke::new(
                        1.,
                        if toast.error { p.danger } else { p.accent },
                    ))
                    .corner_radius(8)
                    .inner_margin(10)
                    .show(ui, |ui| {
                        ui.set_max_width((ctx.content_rect().width() - 100.).clamp(120., 440.));
                        ui.horizontal(|ui| {
                            // Keep room for dismissing a long, wrapped error message.
                            let width = (ui.available_width() - 35.).max(0.);
                            ui.add_sized(
                                [width, 0.],
                                egui::Label::new(RichText::new(&toast.text).color(p.text)).wrap(),
                            );
                            close = ui
                                .small_button("×")
                                .on_hover_text("Hinweis schließen")
                                .clicked();
                        });
                    });
            });
        if close {
            *current = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feedback_expires_without_user_interaction_and_errors_allow_more_reading_time() {
        let ctx = egui::Context::default();
        for error in [false, true] {
            let toast = Toast::new("Einstellungen gespeichert.".into(), error);
            let expires = toast.expires;
            assert!(expires - Instant::now() > Duration::from_secs(if error { 9 } else { 3 }));
            let mut current = Some(toast);
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                Toast::show_at(
                    &mut current,
                    ui.ctx(),
                    Palette::dark(),
                    expires - Duration::from_millis(1),
                );
            });
            output.textures_delta.clear();
            assert!(current.is_some());
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                Toast::show_at(&mut current, ui.ctx(), Palette::dark(), expires);
            });
            output.textures_delta.clear();
            assert!(current.is_none());
        }
    }
}
