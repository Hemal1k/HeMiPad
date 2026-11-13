//! The controls of HeMiDock (WinUI 3) painted in egui – the same as in HeMiUnlocker (not every option is used here).
#![allow(dead_code)]

use crate::theme::{self, Palette, glyph};
use eframe::egui::{self, Align, Align2, Color32, Layout, Margin, RichText, Sense, Stroke, StrokeKind, pos2, vec2};

pub const CONTROL_RADIUS: u8 = 8;
pub const CARD_RADIUS: u8 = 10;

#[derive(Clone, Copy, PartialEq)]
pub enum Tone {
    Success,
    Caution,
    Critical,
}

pub fn section(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).font(theme::semibold(14.0)).color(p.text));
    ui.add_space(4.0);
}

/// A card of the settings of HeMiDock: the card colour, a thin frame, rounded corners.
pub fn card<R>(ui: &mut egui::Ui, p: &Palette, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.card_stroke))
        .corner_radius(CARD_RADIUS)
        .inner_margin(Margin { left: 16, right: 14, top: 10, bottom: 10 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// A row of fixed height with the items centred vertically.
pub fn row_layout(ui: &mut egui::Ui, height: f32, add: impl FnOnce(&mut egui::Ui)) {
    let w = ui.available_width();
    ui.allocate_ui_with_layout(vec2(w, height), Layout::left_to_right(Align::Center), |ui| {
        ui.set_min_size(vec2(w, height));
        add(ui)
    });
}

pub fn glyph_box(ui: &mut egui::Ui, g: char, size: f32, color: Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
    ui.painter().text(r.center(), Align2::CENTER_CENTER, g, theme::icon(size), color);
}

pub fn two_lines(ui: &mut egui::Ui, p: &Palette, title: &str, detail: &str) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        ui.add(egui::Label::new(RichText::new(title).size(14.0).color(p.text)).truncate());
        ui.add(egui::Label::new(RichText::new(detail).size(12.0).color(p.text2)).wrap());
    });
}

/// An InfoBar of WinUI: a coloured strip with an icon. Returns true when its × was clicked.
pub fn info_bar(ui: &mut egui::Ui, p: &Palette, tone: Tone, text: &str, closable: Option<()>) -> bool {
    let (bg, fg, g) = match tone {
        Tone::Success => (p.success_bg, p.success, glyph::COMPLETED),
        Tone::Caution => (p.caution_bg, p.caution, glyph::WARNING),
        Tone::Critical => (p.critical_bg, p.critical, glyph::ERROR),
    };
    let mut closed = false;
    egui::Frame::new()
        .fill(bg)
        .stroke(Stroke::new(1.0, p.card_stroke))
        .corner_radius(CONTROL_RADIUS)
        .inner_margin(Margin { left: 14, right: 6, top: 6, bottom: 6 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            row_layout(ui, 32.0, |ui| {
                ui.label(RichText::new(g.to_string()).font(theme::icon(16.0)).color(fg));
                ui.add_space(4.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if closable.is_some() && Btn::new("").icon(glyph::CLOSE).subtle().show(ui, p).clicked() {
                        closed = true;
                    }
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.add(egui::Label::new(RichText::new(text).size(13.0).color(p.text)).truncate());
                    });
                });
            });
        });
    closed
}

/// A ToggleSwitch of WinUI: a pill 40 × 20 with a knob, the accent colour when on.
pub fn toggle(ui: &mut egui::Ui, p: &Palette, on: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(40.0, 20.0), Sense::click());
    let t = ui.ctx().animate_bool_with_time(resp.id, on, 0.12);
    let hover = resp.hovered();
    let painter = ui.painter();
    if on {
        painter.rect_filled(rect, 10, if hover { p.accent_hover } else { p.accent });
    } else {
        painter.rect_filled(rect, 10, if hover { p.subtle_hover } else { Color32::TRANSPARENT });
        painter.rect_stroke(rect, 10, Stroke::new(1.0, p.text2), StrokeKind::Inside);
    }
    let r = if hover { 7.0 } else { 6.0 };
    let x = egui::lerp((rect.left() + 10.0)..=(rect.right() - 10.0), t);
    painter.circle_filled(pos2(x, rect.center().y), r, if on { p.on_accent } else { p.text2 });
    resp
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Standard,
    Accent,
    Subtle,
}

/// A button of WinUI 3: standard (light fill, a frame with a darker bottom edge), accent or subtle.
pub struct Btn<'a> {
    text: &'a str,
    icon: Option<char>,
    kind: Kind,
    enabled: bool,
    tint: Option<Color32>,
    tip: Option<&'a str>,
    width: Option<f32>,
}

impl<'a> Btn<'a> {
    pub fn new(text: &'a str) -> Self {
        Btn { text, icon: None, kind: Kind::Standard, enabled: true, tint: None, tip: None, width: None }
    }
    pub fn icon(mut self, g: char) -> Self {
        self.icon = Some(g);
        self
    }
    pub fn accent(mut self) -> Self {
        self.kind = Kind::Accent;
        self
    }
    pub fn subtle(mut self) -> Self {
        self.kind = Kind::Subtle;
        self
    }
    pub fn enabled(mut self, on: bool) -> Self {
        self.enabled = on;
        self
    }
    pub fn tint(mut self, c: Color32) -> Self {
        self.tint = Some(c);
        self
    }
    pub fn tip(mut self, t: &'a str) -> Self {
        self.tip = Some(t);
        self
    }
    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(w);
        self
    }

    pub fn show(self, ui: &mut egui::Ui, p: &Palette) -> egui::Response {
        let text = (!self.text.is_empty()).then(|| ui.painter().layout_no_wrap(self.text.to_string(), theme::body(14.0), Color32::PLACEHOLDER));
        let icon = self.icon.map(|g| ui.painter().layout_no_wrap(g.to_string(), theme::icon(14.0), Color32::PLACEHOLDER));
        let gap = if text.is_some() && icon.is_some() { 8.0 } else { 0.0 };
        let content = text.as_ref().map_or(0.0, |g| g.size().x) + icon.as_ref().map_or(0.0, |g| g.size().x) + gap;
        let pad = if text.is_none() { 9.0 } else { 12.0 };
        let w = self.width.unwrap_or(content + pad * 2.0).max(32.0);
        let sense = if self.enabled { Sense::click() } else { Sense::hover() };
        let (rect, mut resp) = ui.allocate_exact_size(vec2(w, 32.0), sense);
        if ui.is_rect_visible(rect) {
            let (down, hover) = (resp.is_pointer_button_down_on(), resp.hovered());
            let (fill, stroke, bottom, fg) = match (self.kind, self.enabled) {
                (_, false) => (
                    if self.kind == Kind::Subtle { Color32::TRANSPARENT } else { p.control_disabled },
                    if self.kind == Kind::Subtle { Color32::TRANSPARENT } else { p.control_stroke },
                    None,
                    p.text3,
                ),
                (Kind::Accent, _) => (
                    if down { p.accent_pressed } else if hover { p.accent_hover } else { p.accent },
                    Color32::TRANSPARENT,
                    (!down).then(|| Color32::from_black_alpha(if p.light { 60 } else { 40 })),
                    if down { p.on_accent.gamma_multiply(0.8) } else { p.on_accent },
                ),
                (Kind::Standard, _) => (
                    if down { p.control_pressed } else if hover { p.control_hover } else { p.control },
                    p.control_stroke,
                    (!down).then_some(p.control_stroke_bottom),
                    if down { p.text2 } else { p.text },
                ),
                (Kind::Subtle, _) => (
                    if down { p.subtle_pressed } else if hover { p.subtle_hover } else { Color32::TRANSPARENT },
                    Color32::TRANSPARENT,
                    None,
                    if down { p.text2 } else { p.text },
                ),
            };
            let painter = ui.painter();
            painter.rect_filled(rect, CONTROL_RADIUS, fill);
            if stroke != Color32::TRANSPARENT {
                painter.rect_stroke(rect, CONTROL_RADIUS, Stroke::new(1.0, stroke), StrokeKind::Inside);
            }
            if let Some(b) = bottom {
                let r = CONTROL_RADIUS as f32;
                let y = rect.bottom() - 0.5;
                painter.hline(rect.left() + r - 1.0..=rect.right() - r + 1.0, y, Stroke::new(1.0, b));
            }
            let mut x = rect.center().x - content / 2.0;
            if let Some(g) = icon {
                let c = if self.enabled { self.tint.unwrap_or(fg) } else { fg };
                let y = rect.center().y - g.size().y / 2.0;
                let gw = g.size().x;
                painter.galley(pos2(x, y), g, c);
                x += gw + gap;
            }
            if let Some(g) = text {
                let y = rect.center().y - g.size().y / 2.0;
                painter.galley(pos2(x, y), g, fg);
            }
        }
        if let Some(t) = self.tip {
            resp = resp.on_hover_text(t);
        }
        if self.enabled && resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
        }
        resp
    }
}
