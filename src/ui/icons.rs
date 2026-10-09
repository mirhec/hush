//! Small original vector icons: no icon font, asset fetch or remote SVG parser.
use crate::model::Kind;
use eframe::egui::{self, Color32, Painter, Rect, Stroke, pos2};
#[derive(Clone, Copy)]
pub enum Icon {
    Back,
    IssueOpened,
    ReviewRequested,
    ReviewSubmitted,
    ExternalLink,
    Mention,
    Settings,
    Sun,
    Moon,
}
impl From<Kind> for Icon {
    fn from(k: Kind) -> Self {
        match k {
            Kind::Request => Self::ReviewRequested,
            Kind::Issue => Self::IssueOpened,
            Kind::Review => Self::ReviewSubmitted,
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
        Icon::Back => {
            path(&[(14., 5.), (7., 12.), (14., 19.)]);
        }
        Icon::IssueOpened => {
            // Leave the upper-right arc open for the creation marker.
            let arc: Vec<_> = (0..=28)
                .map(|step| {
                    let angle = (20. + step as f32 * 260. / 28.).to_radians();
                    v(11. + 8. * angle.cos(), 13. + 8. * angle.sin())
                })
                .collect();
            p.add(egui::Shape::line(arc, s));
            p.circle_filled(v(11., 13.), 1.4 * rect.width() / 24., color);
            line((19., 2.), (19., 10.));
            line((15., 6.), (23., 6.));
        }
        Icon::ReviewRequested => {
            circle(5., 5., 2.5);
            circle(5., 19., 2.5);
            circle(19., 19., 2.5);
            line((5., 7.5), (5., 16.5));
            path(&[(19., 16.5), (19., 9.), (17., 6.), (12., 6.)]);
            path(&[(15., 3.), (12., 6.), (15., 9.)]);
        }
        Icon::ReviewSubmitted => {
            circle(5., 5., 2.5);
            circle(5., 19., 2.5);
            line((5., 7.5), (5., 16.5));
            path(&[(19., 10.), (19., 8.), (17., 5.), (12., 5.)]);
            path(&[(14., 3.), (12., 5.), (14., 7.)]);
            path(&[(12., 17.), (15.5, 20.5), (22., 13.5)]);
        }
        Icon::ExternalLink => {
            path(&[(10., 4.), (4., 4.), (4., 20.), (20., 20.), (20., 14.)]);
            path(&[(14., 3.), (21., 3.), (21., 10.)]);
            line((11., 13.), (21., 3.));
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
