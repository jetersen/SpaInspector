use eframe::egui::{self, Color32, FontFamily, FontId, Stroke, TextStyle, Theme};

pub fn configure(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::System);
    for theme in [Theme::Dark, Theme::Light] {
        ctx.style_mut_of(theme, |style| {
            style.text_styles = [
                (
                    TextStyle::Heading,
                    FontId::new(24.0, FontFamily::Proportional),
                ),
                (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
                (
                    TextStyle::Button,
                    FontId::new(14.0, FontFamily::Proportional),
                ),
                (
                    TextStyle::Small,
                    FontId::new(12.0, FontFamily::Proportional),
                ),
                (
                    TextStyle::Monospace,
                    FontId::new(12.0, FontFamily::Monospace),
                ),
            ]
            .into();
            style.spacing.item_spacing = egui::vec2(10.0, 10.0);
            style.spacing.button_padding = egui::vec2(12.0, 7.0);
            style.spacing.interact_size.y = 34.0;
            style.spacing.combo_width = 210.0;
            let dark = theme == Theme::Dark;
            let bg = if dark {
                rgb(17, 23, 31)
            } else {
                rgb(243, 246, 250)
            };
            let panel = if dark {
                rgb(23, 31, 41)
            } else {
                Color32::WHITE
            };
            let inset = if dark {
                rgb(19, 26, 35)
            } else {
                rgb(248, 250, 252)
            };
            let widget = if dark {
                rgb(35, 46, 60)
            } else {
                rgb(237, 243, 248)
            };
            let border = if dark {
                rgb(46, 60, 77)
            } else {
                rgb(213, 223, 233)
            };
            let text = if dark {
                rgb(226, 234, 243)
            } else {
                rgb(32, 49, 68)
            };
            let muted = if dark {
                rgb(148, 165, 185)
            } else {
                rgb(100, 116, 137)
            };
            let accent = if dark {
                rgb(74, 205, 198)
            } else {
                rgb(9, 123, 123)
            };
            let visual = &mut style.visuals;
            visual.panel_fill = bg;
            visual.window_fill = panel;
            visual.window_stroke = Stroke::new(1.0, border);
            visual.window_corner_radius = 10.into();
            visual.menu_corner_radius = 8.into();
            visual.extreme_bg_color = inset;
            visual.text_edit_bg_color = Some(inset);
            visual.faint_bg_color = panel;
            visual.override_text_color = Some(text);
            visual.weak_text_color = Some(muted);
            visual.hyperlink_color = accent;
            visual.selection.bg_fill = if dark {
                rgb(30, 72, 78)
            } else {
                rgb(210, 238, 237)
            };
            visual.selection.stroke = Stroke::new(1.0, accent);
            visual.widgets.noninteractive.bg_stroke = Stroke::new(1.0, border);
            visual.widgets.noninteractive.fg_stroke = Stroke::new(1.0, text);
            for v in [
                &mut visual.widgets.inactive,
                &mut visual.widgets.hovered,
                &mut visual.widgets.active,
                &mut visual.widgets.open,
            ] {
                v.corner_radius = 6.into();
                v.bg_fill = widget;
                v.weak_bg_fill = widget;
                v.bg_stroke = Stroke::new(1.0, border);
                v.fg_stroke = Stroke::new(1.5, text);
                v.expansion = 0.0;
            }
            visual.widgets.hovered.bg_stroke = Stroke::new(1.0, accent);
            visual.widgets.active.bg_fill = visual.selection.bg_fill;
            visual.warn_fg_color = if dark {
                rgb(236, 187, 109)
            } else {
                rgb(149, 89, 20)
            };
            visual.error_fg_color = if dark {
                rgb(241, 145, 145)
            } else {
                rgb(172, 49, 62)
            };
        });
    }
}
fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}
pub fn panel(ui: &egui::Ui, margin: i8) -> egui::Frame {
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .inner_margin(margin)
}
pub fn card(ui: &egui::Ui) -> egui::Frame {
    panel(ui, 16)
        .corner_radius(10)
        .stroke(ui.visuals().window_stroke)
}
pub fn primary(ui: &egui::Ui, label: &str) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(label.to_owned()).color(Color32::WHITE)).fill(
        if ui.visuals().dark_mode {
            rgb(28, 112, 117)
        } else {
            rgb(9, 123, 123)
        },
    )
}
