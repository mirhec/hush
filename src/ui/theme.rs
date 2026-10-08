use crate::model::Kind;
use eframe::egui::{self, Color32, FontId, RichText, Stroke};

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color32, pub sidebar: Color32, pub card: Color32, pub hover: Color32,
    pub border: Color32, pub text: Color32, pub muted: Color32, pub faint: Color32,
    pub accent: Color32, pub on_accent: Color32, pub violet: Color32,
    pub amber: Color32, pub blue: Color32, pub danger: Color32,
}
const fn c(r:u8,g:u8,b:u8)->Color32 { Color32::from_rgb(r,g,b) }
impl Palette {
    pub fn dark()->Self { Self { bg:c(17,24,21),sidebar:c(14,21,18),card:c(23,33,28),hover:c(31,44,37),
        border:c(44,58,49),text:c(236,242,234),muted:c(156,175,160),faint:c(104,127,113),accent:c(171,228,194),on_accent:c(22,49,35),
        violet:c(194,181,235),amber:c(230,194,131),blue:c(147,190,220),danger:c(235,157,157) } }
    pub fn light()->Self { Self { bg:c(247,248,243),sidebar:c(235,240,231),card:c(255,255,251),hover:c(226,236,224),
        border:c(211,222,207),text:c(31,51,39),muted:c(90,112,95),faint:c(113,132,116),accent:c(44,107,72),on_accent:c(255,255,252),
        violet:c(108,83,152),amber:c(148,100,40),blue:c(53,110,158),danger:c(166,64,64) } }
    pub fn kind(self,k:Kind)->Color32 { match k {Kind::Request=>self.violet,Kind::Issue=>self.blue,Kind::Review=>self.accent,Kind::Mention=>self.amber} }
}
pub fn apply(ctx:&egui::Context,light:bool) {
    let p=if light { Palette::light() } else { Palette::dark() };
    ctx.set_theme(if light { egui::Theme::Light } else { egui::Theme::Dark });
    let mut s=egui::Style::default();
    s.visuals=if light {egui::Visuals::light()}else{egui::Visuals::dark()};
    s.visuals.override_text_color=Some(p.text);
    s.visuals.panel_fill=p.bg;s.visuals.window_fill=p.card;s.visuals.extreme_bg_color=p.sidebar;
    s.visuals.faint_bg_color=p.card;s.visuals.selection.bg_fill=p.hover;
    s.visuals.selection.stroke=Stroke::new(1.0,p.accent);
    s.visuals.hyperlink_color=p.accent;
    s.visuals.widgets.inactive.bg_fill=p.card;s.visuals.widgets.inactive.weak_bg_fill=p.card;
    s.visuals.widgets.inactive.bg_stroke=Stroke::new(1.0,p.border);
    s.visuals.widgets.inactive.fg_stroke=Stroke::new(1.0,p.text);
    s.visuals.widgets.hovered.bg_fill=p.hover;s.visuals.widgets.hovered.weak_bg_fill=p.hover;
    s.visuals.widgets.hovered.bg_stroke=Stroke::new(1.0,p.faint);
    s.visuals.widgets.hovered.fg_stroke=Stroke::new(1.0,p.text);
    s.visuals.widgets.active.bg_fill=p.hover;s.visuals.widgets.active.weak_bg_fill=p.hover;
    s.visuals.widgets.active.fg_stroke=Stroke::new(1.0,p.accent);
    s.spacing.item_spacing=egui::vec2(10.0,10.0);
    s.spacing.button_padding=egui::vec2(16.0,10.0);
    s.spacing.interact_size=egui::vec2(40.0,36.0);
    s.text_styles.insert(egui::TextStyle::Body,FontId::proportional(14.0));
    s.text_styles.insert(egui::TextStyle::Button,FontId::proportional(13.0));
    s.text_styles.insert(egui::TextStyle::Small,FontId::proportional(11.5));
    s.text_styles.insert(egui::TextStyle::Heading,FontId::proportional(30.0));
    ctx.set_global_style(s);
}
pub fn label(ui:&mut egui::Ui,text:impl Into<String>,size:f32,color:Color32) {
    ui.label(RichText::new(text).size(size).color(color));
}
pub fn primary(ui:&mut egui::Ui,text:&str,p:Palette)->egui::Response {
    ui.add(egui::Button::new(RichText::new(text).color(p.on_accent).strong()).fill(p.accent).corner_radius(9))
}
pub fn section(ui:&mut egui::Ui,label_text:&str,p:Palette) {
    ui.add_space(20.0); label(ui,label_text,11.0,p.faint); ui.add_space(3.0);
}
