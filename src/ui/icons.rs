//! Small original vector icons: no icon font, asset fetch or remote SVG parser.
use crate::model::Kind;
use eframe::egui::{self, Color32, Painter, Pos2, Rect, Stroke, pos2};
#[derive(Clone, Copy)]
pub enum Icon {
    Inbox,
    Archive,
    Branch,
    Issue,
    Check,
    Mention,
    Settings,
    Refresh,
    Pause,
    Play,
    Arrow,
    Close,
    Shield,
    Sun,
    Moon,
    Search,
}
impl From<Kind> for Icon {
    fn from(k: Kind) -> Self {
        match k {
            Kind::Request => Self::Branch,
            Kind::Issue => Self::Issue,
            Kind::Review => Self::Check,
            Kind::Mention => Self::Mention,
        }
    }
}
pub fn paint(p: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let v = |x: f32, y: f32| {
        pos2(
            rect.left() + x * rect.width() / 24.0,
            rect.top() + y * rect.height() / 24.0,
        )
    };
    let s = Stroke::new(1.6, color);
    let line = |a: (f32, f32), b: (f32, f32)| {
        p.line_segment([v(a.0, a.1), v(b.0, b.1)], s);
    };
    let path = |points: &[(f32, f32)]| {
        p.add(egui::Shape::line(
            points.iter().map(|&(x, y)| v(x, y)).collect(),
            s,
        ));
    };
    let circle = |x: f32, y: f32, r: f32| {
        p.circle_stroke(v(x, y), r * rect.width() / 24.0, s);
    };
    match icon {
        Icon::Inbox => {
            path(&[(4., 5.), (20., 5.), (22., 18.), (2., 18.), (4., 5.)]);
            path(&[
                (3., 13.),
                (8., 13.),
                (10., 16.),
                (14., 16.),
                (16., 13.),
                (21., 13.),
            ]);
        }
        Icon::Archive => {
            path(&[(4., 8.), (4., 20.), (20., 20.), (20., 8.)]);
            path(&[(2., 4.), (22., 4.), (22., 8.), (2., 8.), (2., 4.)]);
            line((9., 12.), (15., 12.));
        }
        Icon::Branch => {
            circle(6., 5., 2.5);
            circle(6., 19., 2.5);
            circle(18., 19., 2.5);
            line((6., 7.5), (6., 16.5));
            path(&[(18., 16.), (18., 10.), (16., 7.), (12., 7.)]);
            path(&[(14., 4.), (11., 7.), (14., 10.)]);
        }
        Icon::Issue => {
            circle(12., 12., 8.5);
            line((12., 7.), (12., 12.));
            p.circle_filled(v(12., 16.), 1.0, color);
        }
        Icon::Check => {
            circle(12., 12., 8.5);
            path(&[(7.5, 12.), (10.5, 15.), (16.5, 9.)]);
        }
        Icon::Mention => {
            circle(11., 12., 4.);
            path(&[
                (15., 8.),
                (15., 15.),
                (17., 16.),
                (20., 14.),
                (21., 10.),
                (19., 6.),
                (15., 3.),
                (10., 3.),
                (5., 5.),
                (3., 10.),
                (3., 15.),
                (6., 19.),
                (11., 21.),
                (16., 20.),
            ]);
        }
        Icon::Settings => {
            circle(12., 12., 3.2);
            circle(12., 12., 8.0);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.;
                line(
                    (12. + 8. * a.cos(), 12. + 8. * a.sin()),
                    (12. + 10.5 * a.cos(), 12. + 10.5 * a.sin()),
                );
            }
        }
        Icon::Refresh => {
            path(&[
                (20., 9.),
                (18., 5.),
                (13., 3.),
                (8., 4.),
                (4., 8.),
                (3., 12.),
                (5., 17.),
                (10., 20.),
                (15., 20.),
                (19., 17.),
            ]);
            path(&[(21., 4.), (21., 10.), (15., 10.)]);
        }
        Icon::Pause => {
            line((8., 5.), (8., 19.));
            line((16., 5.), (16., 19.));
        }
        Icon::Play => {
            path(&[(7., 4.), (20., 12.), (7., 20.), (7., 4.)]);
        }
        Icon::Arrow => {
            line((5., 19.), (19., 5.));
            path(&[(8., 5.), (19., 5.), (19., 16.)]);
        }
        Icon::Close => {
            line((6., 6.), (18., 18.));
            line((18., 6.), (6., 18.));
        }
        Icon::Shield => {
            path(&[
                (12., 3.),
                (20., 6.),
                (19., 15.),
                (16., 19.),
                (12., 22.),
                (8., 19.),
                (5., 15.),
                (4., 6.),
                (12., 3.),
            ]);
            path(&[(8., 12.), (11., 15.), (16., 10.)]);
        }
        Icon::Search => {
            circle(10., 10., 6.5);
            line((15., 15.), (21., 21.));
        }
        Icon::Sun => {
            circle(12., 12., 4.);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.;
                line(
                    (12. + 7. * a.cos(), 12. + 7. * a.sin()),
                    (12. + 10. * a.cos(), 12. + 10. * a.sin()),
                );
            }
        }
        Icon::Moon => {
            path(&[
                (15., 3.),
                (10., 4.),
                (6., 7.),
                (4., 12.),
                (5., 17.),
                (9., 20.),
                (14., 21.),
                (19., 18.),
                (21., 14.),
                (17., 15.),
                (13., 13.),
                (11., 9.),
                (12., 5.),
                (15., 3.),
            ]);
        }
    }
}
pub fn mark(p: &Painter, center: Pos2, size: f32, color: Color32) {
    // Three rounded vertical strokes, like a quiet waveform / lower-case h.
    for (x, h) in [(-0.27, 0.76), (0., 0.48), (0.27, 0.62)] {
        let r = Rect::from_center_size(
            center + egui::vec2(x * size, (0.76 - h) * size * 0.5),
            egui::vec2(size * 0.12, size * h),
        );
        p.rect_filled(r, egui::CornerRadius::same(3), color);
    }
}
pub fn button(
    ui: &mut egui::Ui,
    icon: Icon,
    help: &str,
    p: super::theme::Palette,
) -> egui::Response {
    let (r, response) = ui.allocate_exact_size(egui::vec2(28., 28.), egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, help));
    if response.hovered() || response.has_focus() {
        ui.painter().rect_filled(r, 5, p.hover);
    }
    paint(
        ui.painter(),
        r.shrink(6.),
        icon,
        if response.hovered() { p.text } else { p.muted },
    );
    response
        .on_hover_text(help)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}
