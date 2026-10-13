use std::sync::{Arc, mpsc};

use crate::plot::{PlotOptions, SpectrumPlot, TraceInput};
use eframe::egui;
use spa_reader::{Document, ReadOptions, TextEncoding};

use crate::platform::{self, Event, MAX_FILE_BYTES, MAX_TOTAL_BYTES};

struct OpenFile {
    name: String,
    bytes: Option<Arc<Vec<u8>>>,
    document: Option<Arc<Document>>,
    error: Option<String>,
    selected_series: usize,
    cached_series: Option<usize>,
    plot: SpectrumPlot,
    overlay_enabled: bool,
    reverse_x: bool,
}

pub struct InspectorApp {
    files: Vec<OpenFile>,
    selected_file: usize,
    files_expanded: bool,
    encoding: TextEncoding,
    sender: mpsc::Sender<Event>,
    receiver: mpsc::Receiver<Event>,
    picking: bool,
    pending: usize,
    status: String,
    notifications: crate::notifications::Notifications,
    pick_files: fn(mpsc::Sender<Event>, egui::Context, usize, usize),
    save_file: fn(String, Vec<u8>, mpsc::Sender<Event>, egui::Context),
    overlay: bool,
    overlay_units: Option<(u32, u32)>,
    overlay_plot: SpectrumPlot,
    overlay_reverse: Option<bool>,
    control: control::Control,
}

impl InspectorApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        crate::appearance::configure(&cc.egui_ctx);
        let (sender, receiver) = mpsc::channel();
        Self {
            files: Vec::new(),
            selected_file: 0,
            files_expanded: true,
            encoding: TextEncoding::Auto,
            sender,
            receiver,
            picking: false,
            pending: 0,
            status: String::new(),
            notifications: Default::default(),
            pick_files: platform::pick_files,
            save_file: platform::save_file,
            overlay: false,
            overlay_units: None,
            overlay_plot: SpectrumPlot::default(),
            overlay_reverse: None,
            control: control::Control::new(&cc.egui_ctx),
        }
    }

    fn busy(&self) -> bool {
        self.picking || self.pending != 0
    }

    fn process_events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                Event::Imported(inputs) => {
                    self.picking = false;
                    let mut total = self
                        .files
                        .iter()
                        .filter_map(|file| file.bytes.as_ref())
                        .map(|data| data.len())
                        .sum::<usize>();
                    let mut parse_jobs = Vec::new();
                    for input in inputs {
                        if platform::remaining_file_slots(self.files.len()) == 0 {
                            self.status = "At most 32 files can be open at once".into();
                            self.notifications.notify(&self.status, true, ctx);
                            break;
                        }
                        let index = self.files.len();
                        let (bytes, error) = match input.data {
                            Ok(data) if data.len() > MAX_FILE_BYTES => (None, Some("File exceeds the 64 MiB limit".into())),
                            Ok(data) if data.len() > MAX_TOTAL_BYTES.saturating_sub(total) => (None, Some("Open files exceed the 128 MiB total limit. Close files before importing more.".into())),
                            Ok(data) => {
                                total += data.len();
                                let bytes = Arc::new(data);
                                parse_jobs.push((index, Arc::clone(&bytes)));
                                (Some(bytes), None)
                            }
                            Err(error) => (None, Some(error)),
                        };
                        if let Some(error) = &error {
                            self.notifications.notify(
                                format!("{}: {error}", input.name),
                                true,
                                ctx,
                            );
                        }
                        self.files.push(OpenFile {
                            name: input.name,
                            bytes,
                            document: None,
                            error,
                            selected_series: 0,
                            cached_series: None,
                            plot: SpectrumPlot::default(),
                            overlay_enabled: true,
                            reverse_x: false,
                        });
                    }
                    self.pending += parse_jobs.len();
                    platform::parse_files(
                        parse_jobs,
                        ReadOptions {
                            text_encoding: self.encoding,
                        },
                        self.sender.clone(),
                        ctx.clone(),
                    );
                }
                Event::Parsed { index, result } => {
                    self.pending = self.pending.saturating_sub(1);
                    self.overlay_plot.invalidate();
                    if let Some(file) = self.files.get_mut(index) {
                        file.plot.invalidate();
                        match result {
                            Ok(document) => {
                                file.document = Some(Arc::from(document));
                                file.error = None;
                            }
                            Err(error) => {
                                file.document = None;
                                self.notifications.notify(
                                    format!("{}: {error}", file.name),
                                    true,
                                    ctx,
                                );
                                file.error = Some(error);
                            }
                        }
                    }
                }
                Event::Exported(result) => {
                    let error = result.is_err();
                    self.status = result.unwrap_or_else(|error| format!("Export failed: {error}"));
                    self.notifications.notify(&self.status, error, ctx);
                }
            }
        }
    }

    fn reparse(&mut self, ctx: &egui::Context) {
        let jobs = self
            .files
            .iter_mut()
            .enumerate()
            .filter_map(|(index, file)| {
                file.bytes.as_ref().map(|bytes| {
                    file.document = None;
                    file.error = None;
                    file.selected_series = 0;
                    file.cached_series = None;
                    file.plot.invalidate();
                    (index, Arc::clone(bytes))
                })
            })
            .collect::<Vec<_>>();
        self.pending = jobs.len();
        platform::parse_files(
            jobs,
            ReadOptions {
                text_encoding: self.encoding,
            },
            self.sender.clone(),
            ctx.clone(),
        );
    }

    fn toolbar(&mut self, parent: &mut egui::Ui) {
        let ctx = parent.ctx().clone();
        let frame = crate::appearance::panel(parent, 16);
        egui::Panel::top("toolbar").frame(frame).show(parent, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading(egui::RichText::new("SPA Inspector").strong());
                ui.add_space(20.0);
                if ui
                    .add(egui::Button::new("Individual files").selected(!self.overlay))
                    .clicked()
                {
                    self.overlay = false;
                }
                if ui
                    .add(egui::Button::new("Overlay spectra").selected(self.overlay))
                    .clicked()
                {
                    self.overlay = true;
                }
                ui.add_space(16.0);
                let busy = self.busy();
                let button = crate::appearance::primary(ui, "Open SPA files…");
                if ui
                    .add_enabled(
                        !busy && platform::remaining_file_slots(self.files.len()) > 0,
                        button,
                    )
                    .clicked()
                {
                    self.picking = true;
                    self.status.clear();
                    let used = self
                        .files
                        .iter()
                        .filter_map(|f| f.bytes.as_ref())
                        .map(|b| b.len())
                        .sum::<usize>();
                    (self.pick_files)(
                        self.sender.clone(),
                        ctx.clone(),
                        platform::remaining_file_slots(self.files.len()),
                        MAX_TOTAL_BYTES.saturating_sub(used),
                    );
                }
                let before = self.encoding;
                ui.menu_button("Preferences", |ui| {
                    ui.set_min_width(280.0);
                    ui.label("Text encoding");
                    ui.add_enabled_ui(!busy, |ui| {
                        egui::ComboBox::from_id_salt("text-encoding")
                            .selected_text(encoding_name(self.encoding))
                            .show_ui(ui, |ui| {
                                for encoding in [
                                    TextEncoding::Auto,
                                    TextEncoding::Utf8,
                                    TextEncoding::Windows1252,
                                    TextEncoding::Latin1,
                                ] {
                                    ui.selectable_value(
                                        &mut self.encoding,
                                        encoding,
                                        encoding_name(encoding),
                                    );
                                }
                            });
                    });
                    ui.separator();
                    ui.label("Appearance");
                    let mut preference = ctx.options(|o| o.theme_preference);
                    egui::ComboBox::from_id_salt("appearance")
                        .selected_text(match preference {
                            egui::ThemePreference::System => "Follow desktop",
                            egui::ThemePreference::Dark => "Dark",
                            egui::ThemePreference::Light => "Light",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut preference,
                                egui::ThemePreference::System,
                                "Follow desktop",
                            );
                            ui.selectable_value(
                                &mut preference,
                                egui::ThemePreference::Dark,
                                "Dark",
                            );
                            ui.selectable_value(
                                &mut preference,
                                egui::ThemePreference::Light,
                                "Light",
                            );
                        });
                    ctx.set_theme(preference);
                });
                if before != self.encoding {
                    self.reparse(&ctx);
                }
                if busy {
                    ui.spinner();
                }
            });
        });
    }

    fn file_list(&mut self, parent: &mut egui::Ui) {
        if !self.files_expanded {
            egui::Panel::left("files-collapsed")
                .exact_size(46.0)
                .resizable(false)
                .frame(crate::appearance::panel(parent, 5))
                .show(parent, |ui| {
                    let button = ui
                        .add_sized([36.0, 28.0], egui::Button::new("›"))
                        .on_hover_text("Expand files");
                    button.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Expand files")
                    });
                    if button.clicked() {
                        self.files_expanded = true;
                    }
                    ui.small(self.files.len().to_string());
                });
            return;
        }
        let mut close = None;
        let busy = self.busy();
        egui::Panel::left("files")
            .resizable(true)
            .default_size(230.0)
            .size_range(190.0..=500.0)
            .frame(crate::appearance::panel(parent, 10))
            .show(parent, |ui| {
                // Fill the requested width so content cannot shrink the panel after a drag.
                ui.set_min_width(ui.available_width());
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 3.0);
                ui.spacing_mut().button_padding = egui::vec2(6.0, 3.0);
                ui.spacing_mut().interact_size.y = 24.0;
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                ui.horizontal(|ui| {
                    ui.strong("FILES");
                    ui.weak(self.files.len().to_string());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let button = ui.button("‹").on_hover_text("Collapse files");
                        button.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::WidgetType::Button,
                                true,
                                "Collapse files",
                            )
                        });
                        if button.clicked() {
                            self.files_expanded = false;
                        }
                    });
                });
                ui.add_space(6.0);
                if self.files.is_empty() {
                    ui.weak("Your local SPA files appear here.");
                }
                egui::ScrollArea::vertical()
                    .id_salt("file-list")
                    .max_height((ui.available_height() - 36.0).max(0.0))
                    .show(ui, |ui| {
                        for (index, file) in self.files.iter_mut().enumerate() {
                            let selected = if self.overlay {
                                file.overlay_enabled
                            } else {
                                self.selected_file == index
                            };
                            let frame = egui::Frame::new().inner_margin(6).corner_radius(6).fill(
                                if selected {
                                    ui.visuals().selection.bg_fill
                                } else {
                                    ui.visuals().faint_bg_color
                                },
                            );
                            frame.show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    if self.overlay {
                                        let width = (ui.available_width() - 30.0).max(40.0);
                                        let response = ui.add_sized(
                                            [width, 24.0],
                                            egui::Checkbox::new(
                                                &mut file.overlay_enabled,
                                                &file.name,
                                            ),
                                        );
                                        response.widget_info(|| {
                                            egui::WidgetInfo::labeled(
                                                egui::WidgetType::Checkbox,
                                                true,
                                                format!("Include {}", file.name),
                                            )
                                        });
                                    } else {
                                        let width = (ui.available_width() - 30.0).max(40.0);
                                        if ui
                                            .add_sized(
                                                [width, 24.0],
                                                egui::Button::new(egui::RichText::new(&file.name))
                                                    .fill(egui::Color32::TRANSPARENT)
                                                    .stroke(egui::Stroke::NONE)
                                                    .wrap_mode(egui::TextWrapMode::Truncate),
                                            )
                                            .clicked()
                                        {
                                            self.selected_file = index;
                                        }
                                    }
                                    if ui
                                        .add_enabled(!busy, egui::Button::new("×").frame(false))
                                        .on_hover_text("Close file and release its data")
                                        .clicked()
                                    {
                                        close = Some(index);
                                    }
                                });
                                if let Some(error) = &file.error {
                                    ui.colored_label(ui.visuals().error_fg_color, error);
                                } else if let Some(document) = &file.document {
                                    ui.small(format!(
                                        "{} series · {} sections",
                                        document.series.len(),
                                        document.sections.len()
                                    ));
                                } else {
                                    ui.small("Reading…");
                                }
                            });
                        }
                    });
                ui.add_space(8.0);
                if ui
                    .add_enabled(
                        !busy && !self.files.is_empty(),
                        egui::Button::new("Close all").frame(false),
                    )
                    .clicked()
                {
                    self.files.clear();
                    self.selected_file = 0;
                    self.status = "Files released from memory".into();
                    self.notifications.notify(&self.status, false, ui.ctx());
                }
            });
        if let Some(index) = close {
            self.files.remove(index);
            if index < self.selected_file {
                self.selected_file -= 1;
            }
            self.selected_file = self.selected_file.min(self.files.len().saturating_sub(1));
        }
    }

    fn inspector(&mut self, parent: &mut egui::Ui) {
        if self.overlay {
            self.overlay_inspector(parent);
            return;
        }
        let ctx = parent.ctx().clone();
        let sender = self.sender.clone();
        let save = self.save_file;
        let wide = parent.available_width() >= 820.0;
        if let Some(file) = self.files.get_mut(self.selected_file) {
            prepare_series(file);
        }
        if wide
            && self
                .files
                .get(self.selected_file)
                .is_some_and(|f| f.document.is_some())
        {
            egui::Panel::right("inspector")
                .resizable(true)
                .default_size(310.0)
                .size_range(280.0..=440.0)
                .frame(crate::appearance::panel(parent, 18))
                .show(parent, |ui| {
                    ui.strong("INSPECTOR");
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical()
                        .id_salt("inspector-details")
                        .show(ui, |ui| {
                            if let Some(file) = self.files.get_mut(self.selected_file) {
                                file_details(ui, file);
                            }
                        });
                });
        }
        let frame = egui::Frame::new()
            .inner_margin(20)
            .fill(parent.visuals().panel_fill);
        egui::CentralPanel::default().frame(frame).show(parent, |ui| {
            let graph_hovered = self.files.get(self.selected_file)
                .and_then(|f| f.plot.graph_rect)
                .is_some_and(|r| ui.input(|i| i.pointer.hover_pos().is_some_and(|p| r.contains(p))));
            let mut content = |ui: &mut egui::Ui| {
                let Some(file) = self.files.get_mut(self.selected_file) else {
                    ui.add_space(70.0);
                    crate::appearance::card(ui).show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.add_space(24.0);
                        ui.heading("Your spectrum workspace");
                        ui.label("Open a SPA file to explore its spectrum and acquisition details.");
                        ui.add_space(12.0);
                        ui.weak("Compare files in an overlay, focus on a wavenumber interval, inspect saved metadata.");
                        ui.add_space(24.0);
                    });
                    return;
                };
                ui.heading(egui::RichText::new(&file.name).strong());
                if let Some(error) = &file.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                    return;
                }
                let Some(document) = &file.document else {
                    ui.spinner();
                    return;
                };
                ui.add(egui::Label::new(
                    egui::RichText::new(&document.title).color(ui.visuals().weak_text_color())
                ).truncate());
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    let csv = ui.button("Export CSV").on_hover_text("Export every series at full precision");
                    csv.widget_info(|| egui::WidgetInfo::labeled(
                        egui::WidgetType::Button, true, "Export all series CSV"
                    ));
                    if csv.clicked() {
                        save(export_name(&file.name, "csv"), document.to_csv().into_bytes(), sender.clone(), ctx.clone());
                    }
                    let json = ui.button("Export JSON").on_hover_text("Export metadata, raw sections and all series");
                    json.widget_info(|| egui::WidgetInfo::labeled(
                        egui::WidgetType::Button, true, "Export full document JSON"
                    ));
                    if json.clicked() {
                        match document.to_json() {
                            Ok(json) => save(export_name(&file.name, "json"), json.into_bytes(), sender.clone(), ctx.clone()),
                            Err(e) => {
                                self.status = format!("JSON export failed: {e}");
                                self.notifications.notify(&self.status, true, &ctx);
                            },
                        }
                    }
                });
                if document.series.is_empty() {
                    ui.label("No supported spectrum or interferogram series found.");
                    if !wide { file_details(ui, file); }
                    return;
                }
                crate::appearance::card(ui).show(ui, |ui| {
                    plot_source_controls(ui, "Series", &document.series[file.selected_series].name,
                        &mut file.reverse_x, |ui| {
                            for (index, series) in document.series.iter().enumerate() {
                                ui.selectable_value(&mut file.selected_series, index,
                                    format!("{} ({})", series.name, series.kind));
                            }
                        });
                    // Prepare a changed series before its first plot frame.
                    if file.cached_series != Some(file.selected_series) {
                        file.plot.invalidate();
                        file.cached_series = Some(file.selected_series);
                        file.reverse_x = matches!(document.series[file.selected_series].metadata.x_unit_code, 1 | 32);
                    }
                    let series = &document.series[file.selected_series];
                    let traces = vec![TraceInput { name: &series.name, x: &series.x, y: &series.y }];
                    file.plot.show(ui, ("spectrum", self.selected_file, file.selected_series), &traces,
                        (&series.metadata.x_unit, &series.metadata.y_unit),
                        PlotOptions {
                            reversed: file.reverse_x,
                            wavenumber: matches!(series.metadata.x_unit_code, 1 | 32),
                            height: if wide { None } else { Some(300.0) },
                        });
                });
                if !wide {
                    ui.add_space(12.0);
                    file_details(ui, file);
                }
            };
            if wide {
                content(ui);
            } else {
                egui::ScrollArea::vertical()
                    .id_salt("narrow-workspace")
                    .scroll_source(egui::scroll_area::ScrollSource {
                        drag: egui::scroll_area::DragScroll::Never,
                        mouse_wheel: !graph_hovered,
                        ..egui::scroll_area::ScrollSource::ALL
                    })
                    .show(ui, content);
            }
        });
    }
    fn overlay_inspector(&mut self, parent: &mut egui::Ui) {
        let frame = egui::Frame::new()
            .inner_margin(20)
            .fill(parent.visuals().panel_fill);
        egui::CentralPanel::default().frame(frame).show(parent, |ui| {
            ui.heading(egui::RichText::new("Overlay spectra").strong());
            let mut groups = Vec::new();
            for series in self.files.iter().filter_map(|f| f.document.as_ref())
                .flat_map(|d| d.series.iter()).filter(|s| s.kind == "spectrum")
            {
                let pair = (series.metadata.x_unit_code, series.metadata.y_unit_code);
                if !groups.iter().any(|(p, _)| *p == pair) {
                    groups.push((pair, format!("{} / {}", series.metadata.x_unit, series.metadata.y_unit)));
                }
            }
            if groups.is_empty() {
                ui.label("Open files containing spectra to compare them.");
                return;
            }
            if !groups.iter().any(|(p, _)| Some(*p) == self.overlay_units) {
                self.overlay_units = Some(groups[0].0);
            }
            ui.small("Choose files in the sidebar. Spectra with different units are shown in separate groups.");
            crate::appearance::card(ui).show(ui, |ui| {
                let old_units = self.overlay_units;
                let mut reversed = self.overlay_reverse.unwrap_or(matches!(self.overlay_units.unwrap().0, 1 | 32));
                let reverse_changed = plot_source_controls(ui, "Overlay units",
                    &groups.iter().find(|(p, _)| Some(*p) == self.overlay_units).unwrap().1,
                    &mut reversed, |ui| {
                        for (pair, label) in &groups {
                            ui.selectable_value(&mut self.overlay_units, Some(*pair), label);
                        }
                    });
                if old_units != self.overlay_units {
                    self.overlay_plot.invalidate();
                    self.overlay_reverse = None;
                    reversed = matches!(self.overlay_units.unwrap().0, 1 | 32);
                } else if reverse_changed {
                    self.overlay_reverse = Some(reversed);
                }
                let mut names = Vec::new();
                let mut series = Vec::new();
                let mut samples = Vec::new();
                for file in self.files.iter().filter(|f| f.overlay_enabled) {
                    if let Some(doc) = &file.document {
                        for s in doc.series.iter().filter(|s| s.kind == "spectrum"
                            && Some((s.metadata.x_unit_code, s.metadata.y_unit_code)) == self.overlay_units)
                        {
                            names.push(if doc.series.iter().filter(|s| s.kind == "spectrum").count() == 1 {
                                file.name.clone()
                            } else {
                                format!("{} · {} / {}", file.name, s.secondary_selector, s.tertiary_selector)
                            });
                            samples.push((s.x.as_slice(), s.y.as_slice()));
                            series.push(s);
                        }
                    }
                }
                if series.is_empty() {
                    ui.label("Select at least one file in this unit group.");
                    return;
                }
                let inputs = samples.iter().zip(&names).map(|((x, y), name)| TraceInput { name, x, y }).collect::<Vec<_>>();
                let m = &series[0].metadata;
                self.overlay_plot.show(ui, "overlay-spectrum", &inputs, (&m.x_unit, &m.y_unit),
                    PlotOptions { reversed, wavenumber: matches!(m.x_unit_code, 1 | 32), height: None });
            });
        });
    }
}

impl eframe::App for InspectorApp {
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        if self.control.enabled {
            ctx.enable_accesskit();
        }
        self.control_raw_input(input);
        if !self.control.input.is_empty() {
            input.events.append(&mut self.control.input);
        } else if let Some(mut events) = self.control.deferred_input.pop_front() {
            input.events.append(&mut events);
        }
        if !self.control.deferred_input.is_empty() {
            ctx.request_repaint();
        }
    }
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.process_events(ctx);
        self.process_control(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.toolbar(ui);
        self.file_list(ui);
        self.inspector(ui);
        self.notifications.show(ui.ctx());
        self.capture_control_ui(ui.ctx());
    }
}

// Both views keep source selection and axis direction on the same responsive row.
fn plot_source_controls(
    ui: &mut egui::Ui,
    label: &str,
    selected_text: &str,
    reversed: &mut bool,
    choices: impl FnOnce(&mut egui::Ui),
) -> bool {
    ui.horizontal_wrapped(|ui| {
        let source = ui
            .allocate_ui_with_layout(
                egui::vec2(84.0, 34.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_size(egui::vec2(84.0, 34.0));
                    ui.add(egui::Label::new(label).truncate())
                },
            )
            .inner;
        ui.allocate_ui_with_layout(
            egui::vec2(260.0, 34.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_size(egui::vec2(260.0, 34.0));
                egui::ComboBox::from_id_salt(("plot-source", label))
                    .width(260.0)
                    .truncate()
                    .selected_text(selected_text)
                    .show_ui(ui, choices)
                    .response
                    .labelled_by(source.id);
            },
        );
        ui.checkbox(reversed, "Reverse X axis").changed()
    })
    .inner
}

fn prepare_series(file: &mut OpenFile) {
    let Some(document) = &file.document else {
        return;
    };
    if document.series.is_empty() {
        return;
    }
    file.selected_series = file.selected_series.min(document.series.len() - 1);
    if file.cached_series != Some(file.selected_series) {
        file.plot.invalidate();
        file.cached_series = Some(file.selected_series);
        file.reverse_x = matches!(
            document.series[file.selected_series].metadata.x_unit_code,
            1 | 32
        );
    }
}
fn file_details(ui: &mut egui::Ui, file: &mut OpenFile) {
    let Some(document) = &file.document else {
        return;
    };
    if let Some(series) = document.series.get(file.selected_series) {
        egui::CollapsingHeader::new("Series metadata")
            .default_open(true)
            .show(ui, |ui| {
                egui::Grid::new("metadata")
                    .num_columns(2)
                    .max_col_width(150.0)
                    .min_row_height(18.0)
                    .spacing([14.0, 10.0])
                    .show(ui, |ui| {
                        row(ui, "Kind", &series.kind);
                        row(ui, "Points", &series.metadata.point_count.to_string());
                        row(
                            ui,
                            "Storage mode",
                            &series.metadata.storage_mode.to_string(),
                        );
                        row(
                            ui,
                            "X unit",
                            &format!(
                                "{} (code {})",
                                series.metadata.x_unit, series.metadata.x_unit_code
                            ),
                        );
                        row(
                            ui,
                            "Y unit",
                            &format!(
                                "{} (code {})",
                                series.metadata.y_unit, series.metadata.y_unit_code
                            ),
                        );
                        row(
                            ui,
                            "X endpoints",
                            &format!("{} -> {}", series.metadata.first_x, series.metadata.last_x),
                        );
                        row(
                            ui,
                            "Stored X endpoints",
                            &format!(
                                "{} -> {}",
                                series.metadata.stored_first_x, series.metadata.stored_last_x
                            ),
                        );
                        row(
                            ui,
                            "Selectors",
                            &format!(
                                "{} / {}",
                                series.secondary_selector, series.tertiary_selector
                            ),
                        );
                        if let Some(value) = series.metadata.scan_count {
                            row(ui, "Scans", &value.to_string());
                        }
                        if let Some(value) = series.metadata.background_scan_count {
                            row(ui, "Background scans", &value.to_string());
                        }
                        if let Some(value) = series.metadata.signal_strength {
                            row(ui, "Signal strength (unverified label)", &value.to_string());
                        }
                        if let Some(value) = series.metadata.background_gain {
                            row(ui, "Background gain", &value.to_string());
                        }
                        if let Some(value) = series.metadata.laser_frequency_cm_inverse {
                            row(ui, "Laser frequency (cm^-1)", &value.to_string());
                        }
                        if let Some(value) = series.metadata.sample_spacing {
                            row(ui, "Sample spacing", &value.to_string());
                        }
                        if let Some(value) = series.metadata.raman_laser_frequency_cm_inverse {
                            row(ui, "Raman laser frequency (cm^-1)", &value.to_string());
                        }
                        if let Some(value) = series.metadata.collection_duration_centiseconds {
                            row(
                                ui,
                                "Collection duration (s)",
                                &(f64::from(value) / 100.0).to_string(),
                            );
                        }
                        if let Some(value) = series.metadata.processing_algorithm_code {
                            row(ui, "Processing algorithm code", &value.to_string());
                        }
                        if let Some(value) = series.metadata.gain {
                            row(ui, "Sample gain", &value.to_string());
                        }
                        if let Some(value) = series.metadata.optical_velocity {
                            row(
                                ui,
                                "Optical velocity (unverified label)",
                                &value.to_string(),
                            );
                        }
                    });
            });
    }
    ui.separator();
    document_details(ui, document);
}

fn row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(
        egui::RichText::new(label)
            .small()
            .color(ui.visuals().weak_text_color()),
    );
    ui.label(egui::RichText::new(value).small());
    ui.end_row();
}

fn document_details(ui: &mut egui::Ui, document: &Document) {
    egui::CollapsingHeader::new("History and timestamp").show(ui, |ui| {
        ui.label(format!("Timestamp (UTC): {}", document.timestamp_utc));
        ui.label(egui::RichText::new(&document.history).monospace());
    });
    if !document.warnings.is_empty() {
        egui::CollapsingHeader::new(format!("Warnings ({})", document.warnings.len()))
            .default_open(true)
            .show(ui, |ui| {
                for warning in &document.warnings {
                    ui.colored_label(ui.visuals().warn_fg_color, warning);
                }
            });
    }
    egui::CollapsingHeader::new(format!(
        "Embedded objects ({})",
        document.embedded_objects.len()
    ))
    .show(ui, |ui| {
        for object in &document.embedded_objects {
            egui::CollapsingHeader::new(format!(
                "{} · class {} · section {}",
                object.kind, object.class_id, object.section_index
            ))
            .show(ui, |ui| {
                ui.label(format!(
                    "{} body bytes · {} attachment bytes",
                    object.body.len(),
                    object.trailing_bytes.len()
                ));
                for attribute in &object.attributes {
                    let mut label = attribute
                        .name
                        .clone()
                        .unwrap_or_else(|| format!("Attribute {}", attribute.id));
                    if object.class_id == 121 && matches!(attribute.id, 3 | 4) {
                        label.push_str(" (inferred label)");
                    }
                    ui.label(format!(
                        "{} [+{}, {}]: {}",
                        label, attribute.offset, attribute.value_type, attribute.value
                    ));
                }
                if object.attributes.is_empty() {
                    ui.label(
                        "Field meanings remain unverified. JSON preserves the complete object.",
                    );
                }
            });
        }
    });
    egui::CollapsingHeader::new(format!("Sections ({})", document.sections.len())).show(ui, |ui| {
        egui::Grid::new("sections")
            .num_columns(5)
            .striped(true)
            .show(ui, |ui| {
                for title in ["Type", "Offset", "Bytes", "Secondary", "Tertiary"] {
                    ui.label(title);
                }
                ui.end_row();
                for section in &document.sections {
                    for value in [
                        section.type_code.to_string(),
                        section.offset.to_string(),
                        section.length.to_string(),
                        section.secondary_selector.to_string(),
                        section.tertiary_selector.to_string(),
                    ] {
                        ui.label(value);
                    }
                    ui.end_row();
                }
            });
    });
}

fn encoding_name(encoding: TextEncoding) -> &'static str {
    match encoding {
        TextEncoding::Auto => "Auto",
        TextEncoding::Utf8 => "UTF-8",
        TextEncoding::Windows1252 => "Windows-1252",
        TextEncoding::Latin1 => "Latin-1",
    }
}

fn export_name(name: &str, extension: &str) -> String {
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    format!("{stem}.{extension}")
}

mod control;
#[cfg(test)]
mod tests;
