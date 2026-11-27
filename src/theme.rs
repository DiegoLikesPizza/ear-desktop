//! Look & feel: monochrome Nothing-style surfaces, dot-matrix display font, red accent.

use eframe::egui::{
    self, Align, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Layout, Margin, Rect,
    Response, Role, Sense, Stroke, TextStyle, Ui, Vec2, WidgetInfo, pos2, vec2,
};
use std::sync::Arc;

pub const BG: Color32 = Color32::from_rgb(0x0A, 0x0A, 0x0A);
pub const CARD: Color32 = Color32::from_rgb(0x17, 0x17, 0x17);
pub const CARD_HI: Color32 = Color32::from_rgb(0x24, 0x24, 0x24);
pub const LINE: Color32 = Color32::from_rgb(0x2C, 0x2C, 0x2C);
pub const TEXT: Color32 = Color32::from_rgb(0xF2, 0xF2, 0xF2);
pub const MUTED: Color32 = Color32::from_rgb(0x8C, 0x8C, 0x8C);
pub const ACCENT: Color32 = Color32::from_rgb(0xD7, 0x19, 0x21);

pub const DOT: &str = "dot";
pub const MEDIUM: &str = "medium";

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "grotesk".into(),
        Arc::new(FontData::from_static(include_bytes!("../assets/fonts/SpaceGrotesk-Regular.ttf"))),
    );
    fonts.font_data.insert(
        MEDIUM.into(),
        Arc::new(FontData::from_static(include_bytes!("../assets/fonts/SpaceGrotesk-Medium.ttf"))),
    );
    fonts.font_data.insert(DOT.into(), Arc::new(FontData::from_static(include_bytes!("../assets/fonts/Ndot57.otf"))));
    fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "grotesk".into());
    let fallback = fonts.families[&FontFamily::Proportional].clone();
    let mut dot = vec![DOT.to_string()];
    dot.extend(fallback.iter().cloned());
    fonts.families.insert(FontFamily::Name(DOT.into()), dot);
    let mut med = vec![MEDIUM.to_string()];
    med.extend(fallback.iter().cloned());
    fonts.families.insert(FontFamily::Name(MEDIUM.into()), med);
    ctx.set_fonts(fonts);

    ctx.all_styles_mut(|s| {
        s.text_styles = [
            (TextStyle::Heading, FontId::new(22.0, FontFamily::Name(MEDIUM.into()))),
            (TextStyle::Body, FontId::new(14.5, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(14.0, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(12.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
        ]
        .into();
        s.spacing.item_spacing = vec2(8.0, 8.0);
        s.spacing.button_padding = vec2(12.0, 7.0);
        s.spacing.interact_size.y = 30.0;
        s.spacing.slider_width = 220.0;
        s.interaction.selectable_labels = false;
        let v = &mut s.visuals;
        *v = egui::Visuals::dark();
        v.panel_fill = BG;
        v.window_fill = CARD;
        v.extreme_bg_color = Color32::from_rgb(0x10, 0x10, 0x10);
        v.faint_bg_color = CARD_HI;
        v.override_text_color = Some(TEXT);
        v.selection.bg_fill = ACCENT;
        v.selection.stroke = Stroke::new(1.0, TEXT);
        v.hyperlink_color = ACCENT;
        v.window_corner_radius = CornerRadius::same(18);
        v.menu_corner_radius = CornerRadius::same(12);
        v.window_stroke = Stroke::new(1.0, LINE);
        for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
            w.corner_radius = CornerRadius::same(255);
        }
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
        v.widgets.inactive.weak_bg_fill = CARD_HI;
        v.widgets.inactive.bg_fill = CARD_HI;
        v.widgets.inactive.bg_stroke = Stroke::NONE;
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x30, 0x30, 0x30);
        v.widgets.hovered.bg_fill = Color32::from_rgb(0x30, 0x30, 0x30);
        v.widgets.hovered.bg_stroke = Stroke::NONE;
        v.widgets.active.weak_bg_fill = Color32::from_rgb(0x3A, 0x3A, 0x3A);
        v.widgets.active.bg_fill = Color32::from_rgb(0x3A, 0x3A, 0x3A);
        v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
        v.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT);
        v.widgets.active.fg_stroke = Stroke::new(1.0, TEXT);
        s.animation_time = 0.12;
    });
}

pub fn dot(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(DOT.into()))
}
pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MEDIUM.into()))
}

/// Rounded surface with a small caps title.
pub fn card<R>(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(CARD)
        .corner_radius(CornerRadius::same(20))
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if !title.is_empty() {
                ui.label(egui::RichText::new(title.to_uppercase()).font(medium(11.5)).color(MUTED).extra_letter_spacing(1.2));
                ui.add_space(4.0);
            }
            add(ui)
        })
        .inner
}

/// Full-width pill group; returns the index that was clicked.
pub fn segmented(ui: &mut Ui, labels: &[&str], selected: Option<usize>) -> Option<usize> {
    let n = labels.len().max(1);
    let h = 38.0;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(255), Color32::from_rgb(0x0F, 0x0F, 0x0F));
    let w = rect.width() / n as f32;
    let mut clicked = None;
    for (i, label) in labels.iter().enumerate() {
        let r = Rect::from_min_size(pos2(rect.left() + w * i as f32, rect.top()), vec2(w, h)).shrink(3.0);
        let resp = ui.interact(r, ui.scope_id().with(("seg", label, i)), Sense::click());
        let on = selected == Some(i);
        let fill = if on {
            TEXT
        } else if resp.hovered() {
            CARD_HI
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(r, CornerRadius::same(255), fill);
        ui.painter().text(
            r.center(),
            egui::Align2::CENTER_CENTER,
            *label,
            FontId::proportional(13.5),
            if on { Color32::BLACK } else { TEXT },
        );
        if resp.clicked() {
            clicked = Some(i);
        }
        resp.widget_info(|| WidgetInfo::selected(Role::Button, true, on, *label));
    }
    clicked
}

/// Pill button that shows a selected state.
pub fn pill(ui: &mut Ui, label: &str, on: bool, width: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 38.0), Sense::click());
    let fill = if on {
        TEXT
    } else if resp.hovered() {
        Color32::from_rgb(0x30, 0x30, 0x30)
    } else {
        CARD_HI
    };
    ui.painter().rect_filled(rect, CornerRadius::same(255), fill);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        FontId::proportional(13.5),
        if on { Color32::BLACK } else { TEXT },
    );
    resp.widget_info(|| WidgetInfo::selected(Role::Button, true, on, label));
    resp
}

/// iOS-style switch.
pub fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let size = vec2(44.0, 24.0);
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    resp.widget_info(|| WidgetInfo::selected(Role::Switch, true, *on, ""));
    let t = ui.ctx().animate_bool_responsive(resp.id, *on);
    let track = if *on { ACCENT } else { Color32::from_rgb(0x3A, 0x3A, 0x3A) };
    ui.painter().rect_filled(rect, CornerRadius::same(255), track);
    let r = rect.height() / 2.0 - 3.0;
    let x = egui::lerp((rect.left() + r + 3.0)..=(rect.right() - r - 3.0), t);
    ui.painter().circle_filled(pos2(x, rect.center().y), r, Color32::WHITE);
    resp
}

/// Label (+ optional hint) on the left, a widget on the right.
pub fn row<R>(ui: &mut Ui, label: &str, hint: Option<&str>, right: impl FnOnce(&mut Ui) -> R) -> R {
    ui.horizontal(|ui| {
        ui.set_min_height(34.0);
        ui.vertical(|ui| {
            ui.add_space(if hint.is_some() { 0.0 } else { 6.0 });
            ui.label(label);
            if let Some(h) = hint {
                ui.label(egui::RichText::new(h).small().color(MUTED));
            }
        });
        ui.with_layout(Layout::right_to_left(Align::Center), right).inner
    })
    .inner
}

pub fn toggle_row(ui: &mut Ui, label: &str, hint: Option<&str>, value: Option<bool>) -> Option<bool> {
    let mut v = value.unwrap_or(false);
    let changed = row(ui, label, hint, |ui| {
        ui.add_enabled_ui(value.is_some(), |ui| toggle(ui, &mut v).changed()).inner
    });
    changed.then_some(v)
}

pub fn separator(ui: &mut Ui) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().line_segment([r.left_center(), r.right_center()], Stroke::new(1.0, LINE));
}

/// Dot-matrix battery readout with a charging marker.
pub fn battery(ui: &mut Ui, label: &str, level: Option<u8>, charging: bool) {
    ui.vertical_centered(|ui| {
        let txt = level.map(|l| format!("{l}%")).unwrap_or_else(|| "—".into());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let g = ui.painter().layout_no_wrap(txt.clone(), dot(24.0), TEXT);
            let total = g.size().x + if charging { 12.0 } else { 0.0 };
            ui.add_space(((ui.available_width() - total) / 2.0).max(0.0));
            ui.label(egui::RichText::new(txt).font(dot(24.0)).color(if level.is_some() { TEXT } else { MUTED }));
            if charging {
                let (r, _) = ui.allocate_exact_size(vec2(8.0, 22.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 3.5, ACCENT);
            }
        });
        ui.label(egui::RichText::new(label).small().color(MUTED));
    });
}

pub fn spinner_text(ui: &mut Ui, text: &str) {
    ui.vertical_centered(|ui| {
        ui.add(egui::Spinner::new().size(28.0).color(TEXT));
        ui.add_space(6.0);
        ui.label(egui::RichText::new(text).color(MUTED));
    });
}

pub fn outline_button(ui: &mut Ui, label: &str) -> Response {
    ui.add(
        egui::Button::new(label)
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::new(1.0, LINE))
            .corner_radius(CornerRadius::same(255)),
    )
}
