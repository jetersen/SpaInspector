use eframe::egui;
use std::time::Duration;

struct Toast {
    id: u64,
    message: String,
    error: bool,
    expires: f64,
}

#[derive(Default)]
pub struct Notifications {
    next_id: u64,
    toasts: Vec<Toast>,
}

impl Notifications {
    pub fn notify(&mut self, message: impl Into<String>, error: bool, ctx: &egui::Context) {
        let message = message.into();
        let expires = ctx.input(|i| i.time) + if error { 10.0 } else { 5.0 };
        if let Some(toast) = self.toasts.iter_mut().find(|t| t.message == message) {
            toast.expires = expires;
        } else {
            self.toasts.push(Toast {
                id: self.next_id,
                message,
                error,
                expires,
            });
            self.next_id += 1;
            if self.toasts.len() > 3 {
                self.toasts.remove(0);
            }
        }
        ctx.request_repaint();
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        self.toasts.retain(|t| t.expires > now);
        if self.toasts.is_empty() {
            return;
        }
        let mut dismiss = None;
        egui::Area::new(egui::Id::new("notifications"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-12.0, -12.0))
            .show(ctx, |ui| {
                ui.set_max_width(340.0);
                for toast in &self.toasts {
                    let color = if toast.error {
                        ui.visuals().error_fg_color
                    } else {
                        ui.visuals().selection.stroke.color
                    };
                    egui::Frame::popup(ui.style())
                        .stroke(egui::Stroke::new(1.0, color))
                        .inner_margin(10)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.add(egui::Label::new(&toast.message).wrap());
                                let response =
                                    ui.small_button("×").on_hover_text("Dismiss notification");
                                response.widget_info(|| {
                                    egui::WidgetInfo::labeled(
                                        egui::WidgetType::Button,
                                        true,
                                        format!("Dismiss notification {}", toast.id),
                                    )
                                });
                                if response.clicked() {
                                    dismiss = Some(toast.id);
                                }
                            });
                        });
                    ctx.request_repaint_after(Duration::from_secs_f64(toast.expires - now));
                }
            });
        if let Some(id) = dismiss {
            self.toasts.retain(|t| t.id != id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notifications_expire_repeat_and_keep_a_bounded_queue() {
        let ctx = egui::Context::default();
        let mut notifications = Notifications::default();
        let mut output = ctx.run_ui(
            egui::RawInput {
                time: Some(0.0),
                ..Default::default()
            },
            |ui| {
                let ctx = ui.ctx();
                notifications.notify("Exported", false, ctx);
                notifications.notify("Export failed", true, ctx);
            },
        );
        output.textures_delta.clear();
        let mut output = ctx.run_ui(
            egui::RawInput {
                time: Some(6.0),
                ..Default::default()
            },
            |ui| {
                let ctx = ui.ctx();
                notifications.show(ctx);
                assert_eq!(notifications.toasts.len(), 1);
                assert!(notifications.toasts[0].error);
                notifications.notify("Export failed", true, ctx);
                assert_eq!(notifications.toasts.len(), 1);
                assert_eq!(notifications.toasts[0].expires, 16.0);
                for message in ["one", "two", "three", "four"] {
                    notifications.notify(message, false, ctx);
                }
                assert_eq!(notifications.toasts.len(), 3);
            },
        );
        output.textures_delta.clear();
        let mut output = ctx.run_ui(
            egui::RawInput {
                time: Some(17.0),
                ..Default::default()
            },
            |ui| {
                let ctx = ui.ctx();
                notifications.show(ctx);
                assert!(notifications.toasts.is_empty());
            },
        );
        output.textures_delta.clear();
    }
}
