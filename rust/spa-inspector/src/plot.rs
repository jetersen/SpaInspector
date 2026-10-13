use eframe::egui;
use egui_plot::{Line, Plot, PlotBounds, PlotPoint, PlotPoints, Points};

pub struct TraceInput<'a> {
    pub name: &'a str,
    pub x: &'a [f64],
    pub y: &'a [f32],
}
pub struct PlotOptions {
    pub reversed: bool,
    pub wavenumber: bool,
    pub height: Option<f32>,
}
struct Trace {
    id: egui::Id,
    name: String,
    points: Vec<PlotPoint>,
    color: egui::Color32,
    drawing: Vec<Vec<PlotPoint>>,
    drawing_key: Option<([f64; 2], usize)>,
}
impl Trace {
    fn prepare_drawing(&mut self, range: [f64; 2], columns: usize) {
        if self.drawing_key == Some((range, columns)) {
            return;
        }
        self.drawing_key = Some((range, columns));
        self.drawing.clear();
        for segment in self
            .points
            .split(|p| !p.x.is_finite() || !p.y.is_finite())
            .filter(|s| !s.is_empty())
        {
            let segment = visible_segment(segment, range);
            if !segment.is_empty() {
                self.drawing.push(envelope(segment, range, columns));
            }
        }
    }
}

// Keep a neighbor on either side so lines crossing a zoom boundary stay joined.
// Unordered axes retain the whole path instead of assuming a binary search is valid.
fn visible_segment(points: &[PlotPoint], range: [f64; 2]) -> &[PlotPoint] {
    let ascending = points.windows(2).all(|p| p[0].x <= p[1].x);
    let descending = !ascending && points.windows(2).all(|p| p[0].x >= p[1].x);
    if ascending {
        let start = points.partition_point(|p| p.x < range[0]).saturating_sub(1);
        let end = (points.partition_point(|p| p.x <= range[1]) + 1).min(points.len());
        &points[start..end]
    } else if descending {
        let start = points.partition_point(|p| p.x > range[1]).saturating_sub(1);
        let end = (points.partition_point(|p| p.x >= range[0]) + 1).min(points.len());
        &points[start..end]
    } else {
        points
    }
}

// Retain actual minima/maxima in their source order within each screen column.
// First/last samples preserve segment endpoints; no averaging can erase a peak.
fn envelope(points: &[PlotPoint], range: [f64; 2], columns: usize) -> Vec<PlotPoint> {
    if points.len() <= columns.saturating_mul(2)
        || range[1] <= range[0]
        || !points.windows(2).all(|p| p[0].x <= p[1].x)
            && !points.windows(2).all(|p| p[0].x >= p[1].x)
    {
        return points.to_vec();
    }
    let column = |x: f64| {
        (((x - range[0]) / (range[1] - range[0]) * columns as f64)
            .floor()
            .clamp(-1.0, columns as f64)) as i64
    };
    let mut output = Vec::with_capacity(columns * 2 + 4);
    output.push(points[0]);
    let mut start = 0;
    while start < points.len() {
        let bucket = column(points[start].x);
        let mut end = start + 1;
        let mut min = start;
        let mut max = start;
        while end < points.len() && column(points[end].x) == bucket {
            if points[end].y < points[min].y {
                min = end;
            }
            if points[end].y > points[max].y {
                max = end;
            }
            end += 1;
        }
        for index in [min.min(max), min.max(max)] {
            if output
                .last()
                .is_none_or(|p| p.x != points[index].x || p.y != points[index].y)
            {
                output.push(points[index]);
            }
        }
        start = end;
    }
    let last = *points.last().unwrap();
    if output.last().is_none_or(|p| p.x != last.x || p.y != last.y) {
        output.push(last);
    }
    output
}

#[derive(Default)]
pub struct SpectrumPlot {
    keys: Vec<(usize, usize, usize, String)>,
    traces: Vec<Trace>,
    pub range: [f64; 2],
    pub domain: [f64; 2],
    y_range: [f64; 2],
    hidden: std::collections::HashSet<egui::Id>,
    palette_dark: bool,
    pub automation_preview: Option<[f64; 2]>,
    pub active_preview: Option<[f64; 2]>,
    pub rendered_range: [f64; 2],
    lower: String,
    upper: String,
    error: Option<String>,
    drag_start: Option<f64>,
    drag_cancelled: bool,
    pub graph_rect: Option<egui::Rect>,
    pub visible_points: usize,
}
impl SpectrumPlot {
    #[cfg(test)]
    pub fn trace_names(&self) -> impl Iterator<Item = &str> {
        self.traces.iter().map(|t| t.name.as_str())
    }
    #[cfg(test)]
    pub fn visible_y_bounds(&self) -> [f64; 2] {
        self.y_range
    }
    #[cfg(test)]
    pub fn point_buffer_addresses(&self) -> Vec<usize> {
        self.traces
            .iter()
            .map(|t| t.points.as_ptr() as usize)
            .collect()
    }
    pub fn invalidate(&mut self) {
        self.keys.clear();
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        id: impl egui::AsId,
        inputs: &[TraceInput<'_>],
        labels: (&str, &str),
        options: PlotOptions,
    ) {
        let PlotOptions {
            reversed,
            wavenumber,
            height,
        } = options;
        let plot_id = ui.make_persistent_id(id);
        let data_changed = self.keys.len() != inputs.len()
            || self.keys.iter().zip(inputs).any(|(key, input)| {
                key.0 != input.x.as_ptr() as usize
                    || key.1 != input.y.as_ptr() as usize
                    || key.2 != input.x.len()
                    || key.3 != input.name
            });
        if data_changed || self.palette_dark != ui.visuals().dark_mode {
            // Allocate cache keys only when a trace or the theme actually changes.
            let keys = inputs
                .iter()
                .map(|input| {
                    (
                        input.x.as_ptr() as usize,
                        input.y.as_ptr() as usize,
                        input.x.len(),
                        input.name.to_owned(),
                    )
                })
                .collect();
            self.palette_dark = ui.visuals().dark_mode;
            let preserve = !self.keys.is_empty() && self.range != self.domain;
            let old_range = self.range;
            let old_keys = std::mem::replace(&mut self.keys, keys);
            let colors = if self.palette_dark {
                [
                    egui::Color32::from_rgb(75, 200, 210),
                    egui::Color32::from_rgb(255, 171, 76),
                    egui::Color32::from_rgb(179, 142, 255),
                    egui::Color32::from_rgb(110, 210, 123),
                    egui::Color32::from_rgb(255, 110, 140),
                ]
            } else {
                [
                    egui::Color32::from_rgb(0, 124, 137),
                    egui::Color32::from_rgb(185, 91, 0),
                    egui::Color32::from_rgb(121, 85, 201),
                    egui::Color32::from_rgb(35, 125, 67),
                    egui::Color32::from_rgb(184, 51, 109),
                ]
            };
            self.traces.truncate(inputs.len());
            for (i, input) in inputs.iter().enumerate() {
                let points = input
                    .x
                    .iter()
                    .zip(input.y)
                    .map(|(&x, &y)| PlotPoint::new(x, f64::from(y)));
                if let Some(trace) = self.traces.get_mut(i) {
                    if old_keys.get(i) != self.keys.get(i) {
                        trace.drawing_key = None;
                        trace.points.clear();
                        trace.points.extend(points);
                        trace.id = egui::Id::new(("trace", i, input.name));
                        trace.name = input.name.to_owned();
                    }
                    trace.color = colors[i % colors.len()];
                } else {
                    self.traces.push(Trace {
                        id: egui::Id::new(("trace", i, input.name)),
                        name: input.name.to_owned(),
                        points: points.collect(),
                        drawing: Vec::new(),
                        drawing_key: None,
                        color: colors[i % colors.len()],
                    });
                }
            }
            let min = inputs
                .iter()
                .flat_map(|t| t.x.iter())
                .copied()
                .filter(|v| v.is_finite())
                .fold(f64::INFINITY, f64::min);
            let max = inputs
                .iter()
                .flat_map(|t| t.x.iter())
                .copied()
                .filter(|v| v.is_finite())
                .fold(f64::NEG_INFINITY, f64::max);
            self.domain = if min.is_finite() && max.is_finite() {
                [min, max]
            } else {
                [0.0, 1.0]
            };
            if preserve && old_range[1] >= self.domain[0] && old_range[0] <= self.domain[1] {
                self.set_range(old_range);
            } else {
                self.reset();
            }
        }
        ui.horizontal_wrapped(|ui| {
            let label = ui.label("X min");
            ui.add(egui::TextEdit::singleline(&mut self.lower).desired_width(90.0))
                .labelled_by(label.id);
            let label = ui.label("X max");
            ui.add(egui::TextEdit::singleline(&mut self.upper).desired_width(90.0))
                .labelled_by(label.id);
            if ui.button("Apply interval").clicked() {
                self.apply_fields();
            }
            let zoom_in = ui
                .add_sized([34.0, 34.0], egui::Button::new("+"))
                .on_hover_text("Zoom in on X");
            zoom_in.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Zoom in")
            });
            if zoom_in.clicked() {
                self.zoom(0.5, (self.range[0] + self.range[1]) / 2.0);
            }
            let zoom_out = ui
                .add_sized([34.0, 34.0], egui::Button::new("−"))
                .on_hover_text("Zoom out on X");
            zoom_out.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Zoom out")
            });
            if zoom_out.clicked() {
                self.zoom(2.0, (self.range[0] + self.range[1]) / 2.0);
            }
            if ui.button("Full range").clicked() {
                self.reset();
            }
        });
        let mut preset_hover = self.automation_preview;
        if wavenumber {
            ui.horizontal_wrapped(|ui| {
                ui.label("Wavenumber sections");
                for (name, range) in [
                    ("4000–2500", [2500.0, 4000.0]),
                    ("2500–1500", [1500.0, 2500.0]),
                    ("1500–700", [700.0, 1500.0]),
                ] {
                    let response = ui.add_enabled(
                        range[1] >= self.domain[0] && range[0] <= self.domain[1],
                        egui::Button::new(name),
                    );
                    if response.clicked() {
                        self.set_range(range);
                    } else if response.hovered() {
                        preset_hover =
                            Some([range[0].max(self.domain[0]), range[1].min(self.domain[1])]);
                    }
                }
            });
        }
        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
        ui.small("Drag across an X interval to zoom. Scroll to zoom X. Double-click or Full range to reset. Y fits the visible samples.");
        // Trace visibility controls live outside the graph, so curves stay unobstructed.
        let mut visibility_changed = false;
        let legend_width = ui.available_width().min(280.0);
        egui::ScrollArea::vertical()
            .id_salt((plot_id, "legend"))
            .max_height(82.0)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for trace in &self.traces {
                        let key = trace.id;
                        let visible = !self.hidden.contains(&key);
                        let button = ui
                            .add_sized(
                                [legend_width, 34.0],
                                egui::Button::new("")
                                    .left_text(
                                        egui::RichText::new(format!("    {}", trace.name)).color(
                                            if visible {
                                                trace.color
                                            } else {
                                                ui.visuals().weak_text_color()
                                            },
                                        ),
                                    )
                                    .selected(visible)
                                    .wrap_mode(egui::TextWrapMode::Truncate),
                            )
                            .on_hover_text(&trace.name);
                        let dot = egui::pos2(button.rect.left() + 12.0, button.rect.center().y);
                        ui.painter().circle_filled(
                            dot,
                            3.5,
                            if visible {
                                trace.color
                            } else {
                                ui.visuals().weak_text_color()
                            },
                        );
                        button.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &trace.name)
                        });
                        if button.clicked() {
                            if visible {
                                self.hidden.insert(key);
                            } else {
                                self.hidden.remove(&key);
                            }
                            visibility_changed = true;
                        }
                    }
                });
            });
        if visibility_changed {
            self.set_range(self.range);
        }
        let preview_context = preset_hover.is_some_and(|range| range != self.range);
        self.active_preview = preset_hover;
        let shown_range = if preview_context {
            self.domain
        } else {
            self.range
        };
        self.rendered_range = shown_range;
        let shown_y = if preview_context {
            self.fit_range(self.domain).0
        } else {
            self.y_range
        };
        let display_x = if shown_range[0] == shown_range[1] {
            [shown_range[0] - 0.5, shown_range[1] + 0.5]
        } else {
            shown_range
        };
        let bounds =
            PlotBounds::from_min_max([display_x[0], shown_y[0]], [display_x[1], shown_y[1]]);
        let (primary_down, primary_released, cancel_drag) = ui.input(|input| {
            (
                input.pointer.primary_down(),
                input.pointer.button_released(egui::PointerButton::Primary),
                input.pointer.button_pressed(egui::PointerButton::Secondary)
                    || input.key_pressed(egui::Key::Escape)
                    || input
                        .events
                        .iter()
                        .any(|event| matches!(event, egui::Event::PointerGone)),
            )
        });
        if cancel_drag && self.drag_start.is_some() {
            self.drag_start = None;
            self.drag_cancelled = true;
        }
        if !primary_down {
            self.drag_cancelled = false;
            // Preserve a normal release for the zoom commit below, but clear
            // a drag if its release was lost while the pointer left the app.
            if !primary_released {
                self.drag_start = None;
            }
        }
        let readout = if let Some((start, pointer, rect)) = self
            .drag_start
            .zip(ui.input(|i| i.pointer.latest_pos()))
            .zip(self.graph_rect)
            .map(|((start, pointer), rect)| (start, pointer, rect))
        {
            let fraction = f64::from((pointer.x - rect.left()) / rect.width());
            let end = if reversed {
                display_x[1] - fraction * (display_x[1] - display_x[0])
            } else {
                display_x[0] + fraction * (display_x[1] - display_x[0])
            };
            format!(
                "Selecting X {:.6} to {:.6} {}",
                start.min(end).clamp(self.domain[0], self.domain[1]),
                start.max(end).clamp(self.domain[0], self.domain[1]),
                labels.0
            )
        } else if let Some([min, max]) = preset_hover {
            format!(
                "Section preview · X {min:.2} to {max:.2} {} · click to zoom",
                labels.0
            )
        } else {
            format!(
                "{} visible samples · X {} to {}",
                self.visible_points, self.lower, self.upper
            )
        };
        ui.add(egui::Label::new(egui::RichText::new(readout).small()).truncate());
        let mut selection = None;
        let mut selecting = None;
        let ctx = ui.ctx().clone();
        let response = Plot::new(plot_id)
            .id(plot_id)
            .height(height.unwrap_or_else(|| ui.available_height().clamp(120.0, 800.0)))
            .invert_x(reversed)
            .x_axis_label(labels.0)
            .y_axis_label(labels.1)
            .allow_drag(false)
            .allow_scroll(false)
            .allow_zoom(false)
            .allow_boxed_zoom(false)
            .allow_axis_zoom_drag(false)
            .allow_double_click_reset(false)
            .show(ui, |plot_ui| {
                plot_ui.set_plot_bounds(bounds);
                plot_ui.set_auto_bounds(false);
                self.graph_rect = Some(plot_ui.response().rect);
                let columns = (plot_ui.response().rect.width() * ctx.pixels_per_point())
                    .ceil()
                    .max(1.0) as usize;
                for trace in self
                    .traces
                    .iter_mut()
                    .filter(|t| !self.hidden.contains(&t.id))
                {
                    trace.prepare_drawing(shown_range, columns);
                }
                for trace in self.traces.iter().filter(|t| !self.hidden.contains(&t.id)) {
                    for segment in &trace.drawing {
                        let points = PlotPoints::Borrowed(segment);
                        if segment.len() == 1 {
                            plot_ui.points(
                                Points::new(&trace.name, points)
                                    .id(trace.id)
                                    .radius(4.0)
                                    .color(trace.color),
                            );
                        } else {
                            plot_ui.line(
                                Line::new(&trace.name, points)
                                    .id(trace.id)
                                    .color(trace.color),
                            );
                        }
                    }
                }
                let response = plot_ui.response();
                let current_x = |p: egui::Pos2| {
                    let fraction = f64::from((p.x - response.rect.left()) / response.rect.width());
                    if reversed {
                        display_x[1] - fraction * (display_x[1] - display_x[0])
                    } else {
                        display_x[0] + fraction * (display_x[1] - display_x[0])
                    }
                };
                let pointer = ctx
                    .input(|i| i.pointer.latest_pos())
                    .map(|p| PlotPoint::new(current_x(p), 0.0));
                if !cancel_drag
                    && !self.drag_cancelled
                    && response.drag_started_by(egui::PointerButton::Primary)
                {
                    self.drag_start = ctx.input(|i| i.pointer.press_origin()).map(current_x);
                }
                if let (Some(start), Some(end)) = (self.drag_start, pointer) {
                    if response.dragged_by(egui::PointerButton::Primary) {
                        selecting = Some([start, end.x]);
                    }
                    if response.drag_stopped_by(egui::PointerButton::Primary) {
                        if (start - end.x).abs() > (self.range[1] - self.range[0]) * 0.005 {
                            selection = Some([start, end.x]);
                        }
                        self.drag_start = None;
                    }
                }
                if response.double_clicked() {
                    selection = Some(self.domain);
                }
                if response.hovered() {
                    let scroll = ctx.input(|i| {
                        i.raw
                            .events
                            .iter()
                            .filter_map(|event| match event {
                                egui::Event::MouseWheel { unit, delta, .. } => Some(
                                    delta.y
                                        * match unit {
                                            egui::MouseWheelUnit::Point => 1.0,
                                            egui::MouseWheelUnit::Line => 20.0,
                                            egui::MouseWheelUnit::Page => 100.0,
                                        },
                                ),
                                _ => None,
                            })
                            .sum::<f32>()
                    });
                    if scroll != 0.0
                        && let Some(pointer) = pointer
                    {
                        let factor = (-f64::from(scroll) * 0.005).exp().clamp(0.1, 10.0);
                        selection = Some(self.zoomed_range(factor, pointer.x));
                    }
                }
            });
        // egui_plot paints its background and traces after the build closure.
        // Paint the drag band here so it stays above the completed graph.
        if let Some([start, end]) = selecting.or(preset_hover) {
            let frame = *response.transform.frame();
            let first = response
                .transform
                .position_from_point_x(start)
                .clamp(frame.left(), frame.right());
            let last = response
                .transform
                .position_from_point_x(end)
                .clamp(frame.left(), frame.right());
            let band = egui::Rect::from_min_max(
                egui::pos2(first.min(last), frame.top()),
                egui::pos2(first.max(last), frame.bottom()),
            );
            let painter = ui.painter().with_clip_rect(ui.clip_rect().intersect(frame));
            let accent = if ui.visuals().dark_mode {
                egui::Color32::from_rgb(100, 225, 235)
            } else {
                egui::Color32::from_rgb(0, 124, 137)
            };
            painter.rect_filled(
                band,
                0.0,
                accent.linear_multiply(if selecting.is_some() { 0.28 } else { 0.16 }),
            );
            painter.rect_stroke(
                band,
                0.0,
                egui::Stroke::new(2.0, accent),
                egui::StrokeKind::Inside,
            );
            if selecting.is_some() {
                ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
            }
        }
        response.response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Spectrum graph")
        });
        if let Some(range) = selection {
            self.set_range(range);
            ctx.request_repaint();
        }
    }

    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({"domain":self.domain,"range":self.range,"renderedRange":self.rendered_range,"yRange":self.y_range,"previewRange":self.active_preview,"visiblePoints":self.visible_points,"graphBounds":self.graph_rect.map(|r|[r.left(),r.top(),r.right(),r.bottom()]),"traces":self.traces.iter().enumerate().map(|(index,t)|serde_json::json!({"index":index,"name":t.name,"points":t.points.len(),"visible":!self.hidden.contains(&t.id)})).collect::<Vec<_>>()})
    }
    pub fn interval(&mut self, min: f64, max: f64) -> Result<(), String> {
        if !min.is_finite() || !max.is_finite() || min == max {
            return Err("Enter two different, finite X values.".into());
        }
        if min.min(max) > self.domain[1] || min.max(max) < self.domain[0] {
            return Err("Interval is outside the data range.".into());
        }
        self.automation_preview = None;
        self.set_range([min, max]);
        Ok(())
    }
    pub fn zoom_by(&mut self, factor: f64, center: Option<f64>) -> Result<(), String> {
        let center = center.unwrap_or((self.range[0] + self.range[1]) / 2.0);
        if !factor.is_finite() || factor <= 0.0 || !center.is_finite() {
            return Err("Zoom factor must be finite and positive; center must be finite.".into());
        }
        let range = self.zoomed_range(factor, center);
        if range.iter().any(|v| !v.is_finite()) {
            return Err("Zoom exceeds the finite coordinate range.".into());
        }
        self.automation_preview = None;
        self.set_range(range);
        Ok(())
    }
    pub fn trace_visibility(&mut self, index: usize, visible: bool) -> Result<(), String> {
        let trace = self.traces.get(index).ok_or("Unknown trace index")?;
        if visible {
            self.hidden.remove(&trace.id);
        } else {
            self.hidden.insert(trace.id);
        }
        self.set_range(self.range);
        Ok(())
    }
    pub fn sample(&self, index: usize, x: f64) -> Result<serde_json::Value, String> {
        if !x.is_finite() {
            return Err("X must be finite".into());
        }
        let trace = self.traces.get(index).ok_or("Unknown trace index")?;
        let (i, p) = trace
            .points
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| (a.x - x).abs().total_cmp(&(b.x - x).abs()))
            .ok_or("Trace has no samples")?;
        let y = if p.y.is_finite() {
            serde_json::json!(p.y)
        } else {
            serde_json::json!(p.y.to_string())
        };
        Ok(serde_json::json!({"trace":index,"name":trace.name,"index":i,"x":p.x,"y":y}))
    }
    pub fn reset(&mut self) {
        self.automation_preview = None;
        self.set_range(self.domain);
    }
    fn apply_fields(&mut self) {
        let parsed = self
            .lower
            .parse::<f64>()
            .ok()
            .zip(self.upper.parse::<f64>().ok());
        if let Some((min, max)) = parsed
            && min.is_finite()
            && max.is_finite()
            && min != max
        {
            if min.min(max) > self.domain[1] || min.max(max) < self.domain[0] {
                self.error = Some("Interval is outside the data range.".into());
            } else {
                self.set_range([min, max]);
            }
        } else {
            self.error = Some("Enter two different, finite X values.".into());
        }
    }
    fn zoomed_range(&self, factor: f64, center: f64) -> [f64; 2] {
        [
            center + (self.range[0] - center) * factor,
            center + (self.range[1] - center) * factor,
        ]
    }
    fn zoom(&mut self, factor: f64, center: f64) {
        self.set_range(self.zoomed_range(factor, center));
    }
    pub fn set_range(&mut self, range: [f64; 2]) {
        self.range = [
            range[0].min(range[1]).clamp(self.domain[0], self.domain[1]),
            range[0].max(range[1]).clamp(self.domain[0], self.domain[1]),
        ];
        self.lower = format!("{:.6}", self.range[0])
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned();
        self.upper = format!("{:.6}", self.range[1])
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned();
        self.error = None;
        let (y_range, count) = self.fit_range(self.range);
        self.y_range = y_range;
        self.visible_points = count;
    }
    fn fit_range(&self, range: [f64; 2]) -> ([f64; 2], usize) {
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        let mut count = 0;
        for p in self
            .traces
            .iter()
            .filter(|t| !self.hidden.contains(&t.id))
            .flat_map(|t| t.points.iter())
            .filter(|p| p.x >= range[0] && p.x <= range[1] && p.y.is_finite())
        {
            min = min.min(p.y);
            max = max.max(p.y);
            count += 1;
        }
        if !min.is_finite() {
            ([0.0, 1.0], count)
        } else {
            let pad = if min == max {
                min.abs().max(1.0) * 0.05
            } else {
                (max - min) * 0.06
            };
            ([min - pad, max + pad], count)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawing_keeps_column_extrema_endpoints_and_source_order() {
        let mut points: Vec<_> = (0..100_000)
            .map(|i| PlotPoint::new(i as f64, (i as f64 * 0.17).sin()))
            .collect();
        points[12345].y = 100.0;
        points[12346].y = -100.0;
        for descending in [false, true] {
            if descending {
                points.reverse();
            }
            let before = points.clone();
            let drawing = envelope(&points, [0.0, 99_999.0], 100);
            assert!(drawing.len() <= 2 * 101 + 2);
            assert_eq!(drawing.first().unwrap().x, points.first().unwrap().x);
            assert_eq!(drawing.last().unwrap().x, points.last().unwrap().x);
            assert!(drawing.iter().any(|p| p.x == 12345.0 && p.y == 100.0));
            assert!(drawing.iter().any(|p| p.x == 12346.0 && p.y == -100.0));
            assert!(drawing.windows(2).all(|p| if descending {
                p[0].x >= p[1].x
            } else {
                p[0].x <= p[1].x
            }));
            assert!(
                points
                    .iter()
                    .zip(before)
                    .all(|(a, b)| a.x == b.x && a.y == b.y)
            );
        }
    }

    #[test]
    fn zoom_retains_boundary_neighbors_for_both_axis_directions() {
        let mut points: Vec<_> = (0..10)
            .map(|i| PlotPoint::new(i as f64, i as f64))
            .collect();
        for _ in 0..2 {
            let visible = visible_segment(&points, [3.2, 5.8]);
            let mut x: Vec<_> = visible.iter().map(|p| p.x).collect();
            x.sort_by(f64::total_cmp);
            assert_eq!(x, [3.0, 4.0, 5.0, 6.0]);
            points.reverse();
        }
        // Unordered axes must preserve their path.
        points.swap(1, 5);
        assert_eq!(visible_segment(&points, [3.2, 5.8]).len(), points.len());
        assert_eq!(envelope(&points, [0.0, 9.0], 1).len(), points.len());
    }

    #[test]
    fn drawing_cache_preserves_gaps_singletons_and_rebuilds_on_zoom() {
        let mut trace = Trace {
            id: egui::Id::new("test"),
            name: "test".into(),
            color: egui::Color32::WHITE,
            points: vec![
                PlotPoint::new(0.0, 0.0),
                PlotPoint::new(1.0, 1.0),
                PlotPoint::new(2.0, f64::NAN),
                PlotPoint::new(3.0, 100.0),
            ],
            drawing: Vec::new(),
            drawing_key: None,
        };
        trace.prepare_drawing([0.0, 3.0], 10);
        assert_eq!(
            trace.drawing.iter().map(Vec::len).collect::<Vec<_>>(),
            [2, 1]
        );
        let pointers: Vec<_> = trace.drawing.iter().map(|s| s.as_ptr()).collect();
        trace.prepare_drawing([0.0, 3.0], 10);
        assert_eq!(
            trace.drawing.iter().map(|s| s.as_ptr()).collect::<Vec<_>>(),
            pointers
        );
        trace.prepare_drawing([0.0, 1.0], 20);
        assert_eq!(trace.drawing_key, Some(([0.0, 1.0], 20)));
        assert_eq!(trace.points.len(), 4);
    }
}
