use super::*;
use egui_kittest::{Harness, kittest::Queryable};
use std::time::Duration;

fn fixture_picker(sender: mpsc::Sender<Event>, ctx: egui::Context, _limit: usize, _budget: usize) {
    sender
        .send(Event::Imported(vec![
            platform::InputFile {
                name: "test1.spa".into(),
                data: Ok(include_bytes!("../../../../tests/TestFiles/test1.spa").to_vec()),
            },
            platform::InputFile {
                name: "test2.spa".into(),
                data: Ok(include_bytes!("../../../../tests/TestFiles/test2.spa").to_vec()),
            },
            platform::InputFile {
                name: "broken.spa".into(),
                data: Ok(b"not a spa".to_vec()),
            },
        ]))
        .unwrap();
    ctx.request_repaint();
}
fn fixture_saver(name: String, bytes: Vec<u8>, sender: mpsc::Sender<Event>, ctx: egui::Context) {
    assert!(!bytes.is_empty());
    if name.ends_with(".json") {
        let doc: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(doc["series"].as_array().unwrap().len(), 3);
    } else if name.ends_with(".csv") {
        assert!(bytes.starts_with(b"series,secondary_selector,"));
    }
    sender
        .send(Event::Exported(Ok(format!("Captured {name}"))))
        .unwrap();
    ctx.request_repaint();
}
fn loaded() -> Harness<'static, InspectorApp> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1500.0, 1300.0))
        .build_eframe(|cc| {
            let mut app = InspectorApp::new(cc);
            app.pick_files = fixture_picker;
            app.save_file = fixture_saver;
            app
        });
    h.get_by_label("Open SPA files…").click();
    for _ in 0..100 {
        h.run_steps(2);
        if h.state().files.len() == 3 && !h.state().busy() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(h.state().files.len(), 3);
    assert!(!h.state().busy());
    h.run_steps(3);
    h
}
#[test]
fn file_loading_metadata_objects_exports_and_close_are_queryable() {
    let mut h = loaded();
    assert!(h.state().files[2].error.is_some());
    h.get_by_label("Export full document JSON").click();
    h.run_steps(3);
    assert_eq!(h.state().status, "Captured test1.json");
    h.get_by_label("Export all series CSV").click();
    h.run_steps(3);
    assert_eq!(h.state().status, "Captured test1.csv");
    h.get_by_label("Embedded objects (3)").click_accesskit();
    h.run_steps(3);
    h.get_by_label("Sections (13)").click_accesskit();
    h.run_steps(3);
    h.get_by_label("test2.spa").click();
    h.run_steps(3);
    assert_eq!(h.state().selected_file, 1);
    h.get_by_label("Close all").click();
    h.run_steps(3);
    assert!(h.state().files.is_empty());
}
#[test]
fn interval_buttons_wheel_and_reset_change_x_and_fit_y_without_mutating_samples() {
    let mut h = loaded();
    let original = h.state().files[0].document.as_ref().unwrap().series[0]
        .y
        .clone();
    let point_buffers = h.state().files[0].plot.point_buffer_addresses();
    h.get_by_label("1500–700").click();
    h.run_steps(3);
    let plot = &h.state().files[0].plot;
    assert_eq!(plot.range[1], 1500.0);
    assert!(plot.visible_points > 0 && plot.visible_points < 6846);
    assert!(plot.visible_y_bounds()[1] < 0.5);
    assert_eq!(plot.point_buffer_addresses(), point_buffers);
    let before = plot.range;
    let center = plot.graph_rect.unwrap().center();
    h.input_mut().events.extend([
        egui::Event::PointerMoved(center),
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            phase: egui::TouchPhase::Move,
            delta: egui::vec2(0.0, 100.0),
            modifiers: egui::Modifiers::NONE,
        },
    ]);
    h.run_steps(3);
    assert!(
        h.state().files[0].plot.range[1] - h.state().files[0].plot.range[0] < before[1] - before[0]
    );
    h.get_by_label("Full range").click();
    h.run_steps(3);
    assert_eq!(
        h.state().files[0].plot.range,
        h.state().files[0].plot.domain
    );
    assert_eq!(
        h.state().files[0].document.as_ref().unwrap().series[0].y,
        original
    );
}
#[test]
fn overlay_selection_and_zoom_keep_distinct_traces_and_shared_interval() {
    let mut h = loaded();
    h.get_by_label("Overlay spectra").click();
    h.run_steps(3);
    assert_eq!(
        h.state().overlay_plot.trace_names().collect::<Vec<_>>(),
        ["test1.spa", "test2.spa"]
    );
    h.get_by_label("1500–700").click();
    h.run_steps(3);
    let interval = h.state().overlay_plot.range;
    h.get_by_label("Include test2.spa").click();
    h.run_steps(3);
    assert_eq!(
        h.state().overlay_plot.trace_names().collect::<Vec<_>>(),
        ["test1.spa"]
    );
    assert_eq!(h.state().overlay_plot.range, interval);
    h.get_by_label("Individual files").click();
    h.run_steps(3);
    assert!(!h.state().overlay);
}

#[test]
fn typed_intervals_validate_and_drag_selects_a_reversed_axis() {
    let mut h = loaded();
    let min = h.get_by_label("X min");
    min.focus();
    h.run_steps(1);
    h.key_combination_modifiers(egui::Modifiers::COMMAND, &[egui::Key::A]);
    h.get_by_label("X min").type_text("1700");
    h.run_steps(2);
    h.get_by_label("X max").focus();
    h.run_steps(1);
    h.key_combination_modifiers(egui::Modifiers::COMMAND, &[egui::Key::A]);
    h.get_by_label("X max").type_text("1200");
    h.run_steps(2);
    h.get_by_label("Apply interval").click();
    h.run_steps(3);
    assert_eq!(h.state().files[0].plot.range, [1200.0, 1700.0]);
    h.get_by_label("X max").focus();
    h.run_steps(1);
    h.key_combination_modifiers(egui::Modifiers::COMMAND, &[egui::Key::A]);
    h.get_by_label("X max").type_text("NaN");
    h.run_steps(2);
    h.get_by_label("Apply interval").click();
    h.run_steps(3);
    assert_eq!(h.state().files[0].plot.range, [1200.0, 1700.0]);
    h.get_by_label("Full range").click();
    h.run_steps(3);
    let rect = h.state().files[0].plot.graph_rect.unwrap();
    let start = egui::pos2(rect.left() + rect.width() * 0.25, rect.center().y);
    let end = egui::pos2(rect.left() + rect.width() * 0.60, rect.center().y);
    h.input_mut().events.extend([
        egui::Event::PointerMoved(start),
        egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    ]);
    h.run_steps(2);
    h.input_mut().events.push(egui::Event::PointerMoved(end));
    h.run_steps(2);
    let background = h
        .output()
        .shapes
        .iter()
        .rposition(
            |s| matches!(&s.shape,egui::Shape::Rect(r) if r.rect == rect && r.fill.a() == 255),
        )
        .unwrap();
    let band = h.output().shapes.iter().rposition(|s| matches!(&s.shape,egui::Shape::Rect(r) if r.rect.top() == rect.top() && r.rect.bottom() == rect.bottom() && r.rect.width() < rect.width() && r.fill.a() > 0 && r.fill.a() < 255)).expect("Visible drag band");
    assert!(
        band > background,
        "Selection must paint above the graph background"
    );
    assert_eq!(h.output().shapes[band].clip_rect, rect);
    h.input_mut().events.push(egui::Event::PointerButton {
        pos: end,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(3);
    let plot = &h.state().files[0].plot;
    assert!(plot.range[0] > plot.domain[0] && plot.range[1] < plot.domain[1]);
    assert!(plot.range[0] < plot.range[1]);
}

#[test]
fn legend_visibility_refits_y_and_counts_only_visible_traces() {
    let mut h = loaded();
    h.get_by_label("Overlay spectra").click();
    h.run_steps(3);
    assert_eq!(h.state().overlay_plot.visible_points, 13692);
    assert!(
        !h.get_by_label("test2.spa")
            .rect()
            .intersects(h.state().overlay_plot.graph_rect.unwrap()),
        "Legend stays outside the graph"
    );
    h.get_by_label("test2.spa").click();
    h.run_steps(4);
    assert_eq!(h.state().overlay_plot.visible_points, 6846);
    assert!(h.state().overlay_plot.visible_y_bounds()[1] < 1.3);
    h.get_by_label("test2.spa").click();
    h.run_steps(4);
    assert_eq!(h.state().overlay_plot.visible_points, 13692);
}

#[test]
fn hovering_a_section_previews_its_band_without_changing_the_zoom() {
    let mut h = loaded();
    h.get_by_label("1500–700").click();
    h.run_steps(3);
    let interval = h.state().files[0].plot.range;
    h.get_by_label("4000–2500").hover();
    h.run_steps(3);
    assert_eq!(h.state().files[0].plot.range, interval);
    let plot = &h.state().files[0].plot;
    let rect = plot.graph_rect.unwrap();
    let band = h
        .output()
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Rect(r)
                if r.rect.top() == rect.top()
                    && r.rect.bottom() == rect.bottom()
                    && r.fill.a() > 0
                    && r.fill.a() < 255 =>
            {
                Some(r.rect)
            }
            _ => None,
        })
        .expect("Hover selection band");
    let expected = rect.width() * (1500.0 / (plot.domain[1] - plot.domain[0])) as f32;
    assert!(
        (band.width() - expected).abs() < 1.0,
        "Preview uses the full spectrum as context"
    );
    h.hover_at(rect.center());
    h.run_steps(3);
    assert_eq!(h.state().files[0].plot.range, interval);
    h.get_by_label("4000–2500").click();
    h.run_steps(3);
    assert_eq!(h.state().files[0].plot.range, [2500.0, 4000.0]);
}

#[test]
fn appearance_follows_system_light_and_dark_themes() {
    let ctx = egui::Context::default();
    crate::appearance::configure(&ctx);
    assert_eq!(
        ctx.options(|o| o.theme_preference),
        egui::ThemePreference::System
    );
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        let output = ctx.run_ui(
            egui::RawInput {
                system_theme: Some(theme),
                ..Default::default()
            },
            |ui| {
                let ctx = ui.ctx();
                assert_eq!(ctx.theme(), theme);
                assert_eq!(
                    ctx.global_style().visuals.dark_mode,
                    theme == egui::Theme::Dark
                );
            },
        );
        output.drop_without_applying_deltas();
    }
}

fn api(
    h: &mut Harness<'static, InspectorApp>,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    let ctx = h.ctx.clone();
    let result = h
        .state_mut()
        .command(
            spa_control::Command {
                method: method.into(),
                params,
            },
            &ctx,
        )
        .unwrap();
    for _ in 0..3 {
        let mut input = egui::RawInput::default();
        eframe::App::raw_input_hook(h.state_mut(), &ctx, &mut input);
        h.input_mut().events.extend(input.events);
        h.run_steps(1);
    }
    result
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn desktop_picker_and_control_import_more_than_32_files() {
    fn many_files_picker(
        sender: mpsc::Sender<Event>,
        ctx: egui::Context,
        limit: usize,
        _budget: usize,
    ) {
        assert!(limit > 32);
        sender
            .send(Event::Imported(
                (0..33)
                    .map(|i| platform::InputFile {
                        name: format!("picker-{i}.spa"),
                        data: Ok(include_bytes!("../../../../tests/TestFiles/test1.spa").to_vec()),
                    })
                    .collect(),
            ))
            .unwrap();
        ctx.request_repaint();
    }
    fn wait_for_import(h: &mut Harness<'static, InspectorApp>, expected: usize) {
        for _ in 0..100 {
            h.run_steps(2);
            if h.state().files.len() == expected && !h.state().busy() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("Import did not finish with {expected} files");
    }

    let mut h = loaded();
    h.state_mut().pick_files = many_files_picker;
    h.get_by_label("Open SPA files…").click();
    wait_for_import(&mut h, 36);
    assert!(h.state().status.is_empty());

    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD
        .encode(include_bytes!("../../../../tests/TestFiles/test1.spa"));
    api(
        &mut h,
        "files.open",
        serde_json::json!({"files":(0..33).map(|i| serde_json::json!({"name":format!("control-{i}.spa"),"dataBase64":encoded})).collect::<Vec<_>>()}),
    );
    wait_for_import(&mut h, 69);
    assert!(h.state().files[3..].iter().all(|f| f.document.is_some()));

    // The picker must remain usable after crossing the old cap.
    h.get_by_label("Open SPA files…").click();
    wait_for_import(&mut h, 102);
}

#[test]
fn control_api_inspects_documents_and_drives_files_series_overlay_and_exports() {
    let mut h = loaded();
    h.state_mut().control.enabled = true;
    h.ctx.enable_accesskit();
    h.run_steps(3);
    let state = api(&mut h, "state", serde_json::json!({}));
    assert_eq!(state["files"].as_array().unwrap().len(), 3);
    let document = api(&mut h, "inspect", serde_json::json!({"scope":"document"}));
    assert!(document["data"].get("sourceBytes").is_none());
    assert!(document["data"]["series"][0].get("y").is_none());
    let raw = api(
        &mut h,
        "inspect",
        serde_json::json!({"scope":"section","index":0,"includePayload":true}),
    );
    assert!(raw["data"]["payload"].is_string());
    let object = api(
        &mut h,
        "inspect",
        serde_json::json!({"scope":"object","index":2}),
    );
    assert_eq!(object["data"]["classId"], 159);
    api(
        &mut h,
        "files.select",
        serde_json::json!({"file":0,"series":1}),
    );
    assert_eq!(h.state().files[0].selected_series, 1);
    let interferogram = api(&mut h, "inspect", serde_json::json!({"scope":"series"}));
    assert_eq!(interferogram["data"]["metadata"]["yUnitCode"], 22);
    api(
        &mut h,
        "files.select",
        serde_json::json!({"file":1,"series":0}),
    );
    api(
        &mut h,
        "view.set",
        serde_json::json!({"mode":"overlay","includedFiles":[0,1],"xUnitCode":1,"yUnitCode":17,"reverseX":false}),
    );
    assert!(h.state().overlay);
    api(
        &mut h,
        "plot.set_range",
        serde_json::json!({"min":1200,"max":1700}),
    );
    assert_eq!(h.state().overlay_plot.range, [1200.0, 1700.0]);
    api(&mut h, "plot.zoom", serde_json::json!({"factor":0.5}));
    assert_eq!(h.state().overlay_plot.range, [1325.0, 1575.0]);
    api(
        &mut h,
        "plot.preset",
        serde_json::json!({"section":"4000-2500","preview":true}),
    );
    assert_eq!(h.state().overlay_plot.range, [1325.0, 1575.0]);
    assert_eq!(
        h.state().overlay_plot.rendered_range,
        h.state().overlay_plot.domain
    );
    api(
        &mut h,
        "plot.preset",
        serde_json::json!({"clearPreview":true}),
    );
    api(
        &mut h,
        "plot.trace",
        serde_json::json!({"trace":1,"visible":false}),
    );
    let point = api(
        &mut h,
        "plot.sample",
        serde_json::json!({"trace":0,"x":1500}),
    );
    assert!((point["x"].as_f64().unwrap() - 1500.0).abs() < 1.0);
    api(&mut h, "plot.reset", serde_json::json!({}));
    let export = api(
        &mut h,
        "export",
        serde_json::json!({"file":0,"format":"json"}),
    );
    let document: serde_json::Value =
        serde_json::from_str(export["text"].as_str().unwrap()).unwrap();
    use base64::Engine;
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(document["sourceBytes"].as_str().unwrap())
            .unwrap(),
        include_bytes!("../../../../tests/TestFiles/test1.spa")
    );
    api(&mut h, "files.close", serde_json::json!({"file":2}));
    assert_eq!(h.state().files.len(), 2);
    api(&mut h, "files.clear", serde_json::json!({}));
    assert!(h.state().files.is_empty());
}
#[test]
fn control_tree_contains_real_widgets_and_accessibility_actions_update_them() {
    let mut h = loaded();
    h.state_mut().control.enabled = true;
    h.ctx.enable_accesskit();
    h.run_steps(3);
    let tree = api(&mut h, "ui.elements", serde_json::json!({}));
    let nodes = tree["nodes"].as_array().unwrap();
    assert!(nodes.len() > 100);
    let button = nodes
        .iter()
        .find(|n| {
            n["label"] == "Overlay spectra"
                && n["actions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|a| a == "Click")
        })
        .unwrap();
    api(
        &mut h,
        "ui.perform",
        serde_json::json!({"id":button["id"],"action":"click"}),
    );
    assert!(h.state().overlay);
    api(&mut h, "view.set", serde_json::json!({"mode":"individual"}));
    let tree = api(&mut h, "ui.elements", serde_json::json!({}));
    let field = tree["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["label"] == "X min")
        .unwrap();
    api(
        &mut h,
        "ui.perform",
        serde_json::json!({"id":field["id"],"action":"setValue","value":"1000"}),
    );
    let tree = api(&mut h, "ui.elements", serde_json::json!({}));
    let button = tree["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| {
            n["label"] == "Apply interval"
                && n["actions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|a| a == "Click")
        })
        .unwrap();
    api(
        &mut h,
        "ui.perform",
        serde_json::json!({"id":button["id"],"action":"click"}),
    );
    assert_eq!(h.state().files[0].plot.range[0], 1000.0);
    api(&mut h, "preferences", serde_json::json!({"theme":"light"}));
    assert_eq!(h.ctx.theme(), egui::Theme::Light);
    api(&mut h, "preferences", serde_json::json!({"theme":"system"}));
}

#[test]
fn invalid_control_settings_leave_view_and_encoding_unchanged() {
    let mut h = loaded();
    let ctx = h.ctx.clone();
    assert!(
        h.state_mut()
            .command(
                spa_control::Command {
                    method: "view.set".into(),
                    params: serde_json::json!({"mode":"overlay","xUnitCode":999,"yUnitCode":17})
                },
                &ctx
            )
            .is_err()
    );
    assert!(!h.state().overlay);
    assert!(
        h.state_mut()
            .command(
                spa_control::Command {
                    method: "preferences".into(),
                    params: serde_json::json!({"encoding":"latin1","theme":"invalid"})
                },
                &ctx
            )
            .is_err()
    );
    assert_eq!(h.state().encoding, TextEncoding::Auto);
    assert!(!h.state().busy());
    assert!(
        h.state_mut()
            .command(
                spa_control::Command {
                    method: "ui.perform".into(),
                    params: serde_json::json!({"action":"pointerMove","x":1e100,"y":0}),
                },
                &ctx
            )
            .is_err()
    );
    assert!(h.state().control.input.is_empty());
}

#[test]
fn narrow_workspace_scrolls_details_and_keeps_graph_readout_above_plot() {
    let mut h = loaded();
    h.set_size(egui::vec2(800.0, 800.0));
    h.run_steps(3);
    let domain = h.state().files[0].plot.domain;
    h.input_mut().events.extend([
        egui::Event::PointerMoved(egui::pos2(750.0, 140.0)),
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            phase: egui::TouchPhase::Move,
            delta: egui::vec2(0.0, -250.0),
            modifiers: egui::Modifiers::NONE,
        },
    ]);
    h.run_steps(5);
    assert_eq!(h.state().files[0].plot.range, domain);
    let graph = h.state().files[0].plot.graph_rect.unwrap();
    let readout = h
        .output()
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(text) if text.galley.text().starts_with("6846 visible samples") => {
                Some(text.pos)
            }
            _ => None,
        })
        .expect("Graph readout remains above the plot");
    assert!(readout.y < graph.top(), "Readout must stay above the graph");

    let graph = h.state().files[0].plot.graph_rect.unwrap();
    h.input_mut().events.extend([
        egui::Event::PointerMoved(graph.center()),
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            phase: egui::TouchPhase::Move,
            delta: egui::vec2(0.0, 100.0),
            modifiers: egui::Modifiers::NONE,
        },
    ]);
    h.run_steps(3);
    assert!(
        h.state().files[0].plot.range[1] - h.state().files[0].plot.range[0] < domain[1] - domain[0]
    );
    assert_eq!(
        h.state().files[0].plot.graph_rect.unwrap().top(),
        graph.top()
    );
    h.input_mut().events.extend([
        egui::Event::PointerMoved(egui::pos2(750.0, 140.0)),
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            phase: egui::TouchPhase::Move,
            delta: egui::vec2(0.0, -400.0),
            modifiers: egui::Modifiers::NONE,
        },
    ]);
    h.run_steps(5);
    api(
        &mut h,
        "plot.preset",
        serde_json::json!({"section":"4000-2500","preview":true}),
    );
    let graph = h.state().files[0].plot.graph_rect.unwrap();
    let band = h.output().shapes.iter().find(|s| matches!(&s.shape, egui::Shape::Rect(r) if r.rect.top() == graph.top() && r.rect.bottom() == graph.bottom() && r.fill.a() > 0 && r.fill.a() < 255)).expect("Preview band");
    assert!(
        band.clip_rect.top() >= 66.0,
        "Band must not paint over the toolbar while scrolling"
    );
}

#[test]
#[ignore = "manual CPU frame benchmark; run with --ignored --nocapture"]
fn benchmark_overlay_frames() {
    let mut h = loaded();
    let fixture = h.state().files[0].document.clone().unwrap();
    h.state_mut().files.clear();
    for count in [1, 32, 64] {
        while h.state().files.len() < count {
            let index = h.state().files.len();
            h.state_mut().files.push(OpenFile {
                name: format!("spectrum-{index}.spa"),
                bytes: None,
                document: Some(fixture.clone()),
                error: None,
                selected_series: 0,
                cached_series: None,
                plot: Default::default(),
                overlay_enabled: true,
                reverse_x: true,
            });
        }
        h.state_mut().overlay = true;
        h.run_steps(3);
        let rect = h.state().overlay_plot.graph_rect.unwrap();
        let mut times = Vec::new();
        for frame in 0..21 {
            h.input_mut()
                .events
                .push(egui::Event::PointerMoved(egui::pos2(
                    rect.center().x + frame as f32,
                    rect.center().y,
                )));
            let start = std::time::Instant::now();
            h.run_steps(1);
            let output = h.output();
            std::hint::black_box(
                h.ctx
                    .tessellate(output.shapes.clone(), output.pixels_per_point),
            );
            times.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        times.sort_by(f64::total_cmp);
        eprintln!(
            "overlay {count} files: CPU UI+tessellation median={:.3} ms p95={:.3} ms",
            times[10], times[19]
        );
    }
}

#[test]
fn toolbar_positions_stay_fixed_when_hovering_view_buttons() {
    let mut h = loaded();
    for width in [1500.0, 1180.0, 800.0] {
        h.set_size(egui::vec2(width, 900.0));
        h.hover_at(egui::pos2(1.0, 1.0));
        h.run_steps(3);
        let labels = [
            "Individual files",
            "Overlay spectra",
            "Open SPA files…",
            "Preferences",
        ];
        let before: Vec<_> = labels
            .iter()
            .map(|label| h.get_by_label(label).rect())
            .collect();
        for hovered in ["Individual files", "Overlay spectra"] {
            h.get_by_label(hovered).hover();
            h.run_steps(3);
            let after: Vec<_> = labels
                .iter()
                .map(|label| h.get_by_label(label).rect())
                .collect();
            assert_eq!(after, before, "Hovering {hovered} at width {width}");
        }
    }
}

#[test]
fn app_status_and_plot_readout_stay_above_the_graph() {
    let mut h = loaded();
    for (width, height) in [(1500.0, 1300.0), (1180.0, 800.0), (800.0, 800.0)] {
        h.set_size(egui::vec2(width, height));
        for overlay in [false, true] {
            h.state_mut().overlay = overlay;
            h.run_steps(3);
            let graph = if overlay {
                h.state().overlay_plot.graph_rect.unwrap()
            } else {
                h.state().files[0].plot.graph_rect.unwrap()
            };

            let readout = h
                .output()
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text)
                        if text.galley.text().contains("visible samples · X") =>
                    {
                        Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
                    }
                    _ => None,
                })
                .expect("Plot sample readout");
            assert!(readout.bottom() < graph.top());
            if overlay || width >= 1180.0 {
                assert!(
                    graph.bottom() <= height,
                    "Graph extends below viewport: {graph:?}"
                );
            } else {
                // Narrow individual views scroll; offscreen paths must remain clipped.
                for shape in &h.output().shapes {
                    if matches!(&shape.shape, egui::Shape::Path(path) if path.points.len() > 2) {
                        assert!(shape.clip_rect.bottom() <= height);
                    }
                }
            }
        }
    }
}

#[test]
fn file_sidebar_keeps_resized_width_across_frames_collapse_and_views() {
    let mut h = loaded();
    let panel = |h: &Harness<'static, InspectorApp>| {
        egui::containers::panel::PanelState::load(&h.ctx, egui::Id::new("files"))
            .unwrap()
            .outer_rect
    };
    let before = panel(&h);
    let edge = egui::pos2(before.right(), before.top() + 180.0);
    h.hover_at(edge);
    h.run_steps(2);
    h.drag_at(edge);
    h.run_steps(2);
    let target = egui::pos2(edge.x + 100.0, edge.y);
    h.hover_at(target);
    h.run_steps(3);
    h.drop_at(target);
    h.run_steps(5);
    let resized = panel(&h).width();
    assert!(
        resized > before.width() + 80.0,
        "Resize did not stick: {before:?}, {}",
        panel(&h).width()
    );
    h.run_steps(10);
    assert_eq!(panel(&h).width(), resized);
    h.get_by_label("Collapse files").click();
    h.run_steps(3);
    assert!(!h.state().files_expanded);
    assert_eq!(h.state().files.len(), 3);
    h.get_by_label("Expand files").click();
    h.run_steps(3);
    assert_eq!(panel(&h).width(), resized);
    h.get_by_label("Overlay spectra").click();
    h.run_steps(3);
    assert_eq!(panel(&h).width(), resized);
    h.get_by_label("Include test2.spa").click();
    h.run_steps(3);
    assert!(!h.state().files[1].overlay_enabled);
    h.get_by_label("Individual files").click();
    h.run_steps(3);
    assert_eq!(panel(&h).width(), resized);
}

#[test]
fn file_rows_are_compact_and_toolbar_omits_duplicate_information() {
    let h = loaded();
    let first = h
        .get_by_role_and_label(accesskit::Role::Button, "test1.spa")
        .rect();
    let second = h
        .get_by_role_and_label(accesskit::Role::Button, "test2.spa")
        .rect();
    assert!(
        second.top() - first.top() <= 64.0,
        "File rows are too spaced out"
    );
    for shape in &h.output().shapes {
        if let egui::Shape::Text(text) = &shape.shape {
            assert!(!text.galley.text().contains("Files stay on this device"));
            assert!(!text.galley.text().contains("open file(s)"));
            assert!(!text.galley.text().contains("64 MiB per file"));
        }
    }
}

#[test]
fn export_feedback_is_dismissible_without_changing_toolbar_layout() {
    let mut h = loaded();
    let toolbar = h.get_by_label("Open SPA files…").rect();
    h.get_by_label("Export full document JSON").click();
    h.run_steps(3);
    assert_eq!(h.state().status, "Captured test1.json");
    h.get_by_label("Captured test1.json");
    assert_eq!(h.get_by_label("Open SPA files…").rect(), toolbar);
    h.get_by_label("Dismiss notification 1").click();
    h.run_steps(3);
    assert_eq!(h.state().status, "Captured test1.json");
    assert!(h.output().shapes.iter().all(|shape| !matches!(&shape.shape,
        egui::Shape::Text(text) if text.galley.text() == "Captured test1.json")));
    h.state()
        .sender
        .send(Event::Exported(Err("disk full".into())))
        .unwrap();
    h.run_steps(3);
    h.get_by_label("Export failed: disk full");
    assert_eq!(h.state().status, "Export failed: disk full");
    assert_eq!(h.get_by_label("Open SPA files…").rect(), toolbar);
}

#[test]
fn right_click_cancels_drag_readout_and_does_not_commit_on_release() {
    let mut h = loaded();
    for overlay in [false, true] {
        h.state_mut().overlay = overlay;
        h.run_steps(3);
        let graph = if overlay {
            h.state().overlay_plot.graph_rect.unwrap()
        } else {
            h.state().files[0].plot.graph_rect.unwrap()
        };
        let range = if overlay {
            h.state().overlay_plot.range
        } else {
            h.state().files[0].plot.range
        };
        let start = egui::pos2(graph.left() + graph.width() * 0.25, graph.center().y);
        let end = egui::pos2(graph.left() + graph.width() * 0.6, graph.center().y);
        h.hover_at(start);
        h.drag_at(start);
        h.run_steps(2);
        h.hover_at(end);
        h.run_steps(2);
        let selecting = |h: &Harness<'static, InspectorApp>| {
            h.output().shapes.iter().any(|s|
            matches!(&s.shape, egui::Shape::Text(text) if text.galley.text().starts_with("Selecting X")))
        };
        assert!(selecting(&h));
        h.input_mut().events.push(egui::Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Secondary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(1);
        assert!(!selecting(&h));
        let bands = h
            .output()
            .shapes
            .iter()
            .filter(|shape| {
                matches!(&shape.shape, egui::Shape::Rect(rect) if rect.rect.top() == graph.top()
                && rect.rect.bottom() == graph.bottom() && rect.fill.a() > 0 && rect.fill.a() < 255)
            })
            .count();
        assert_eq!(bands, 0);
        h.input_mut().events.push(egui::Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Secondary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(2);
        h.drop_at(end);
        h.run_steps(3);
        assert!(!selecting(&h));
        assert_eq!(
            if overlay {
                h.state().overlay_plot.range
            } else {
                h.state().files[0].plot.range
            },
            range
        );
        // A fresh primary drag still works after cancelling the previous one.
        h.hover_at(start);
        h.drag_at(start);
        h.run_steps(2);
        h.hover_at(end);
        h.run_steps(2);
        h.drop_at(end);
        h.run_steps(3);
        assert_ne!(
            if overlay {
                h.state().overlay_plot.range
            } else {
                h.state().files[0].plot.range
            },
            range
        );
    }
}

#[test]
fn public_app_does_not_offer_or_execute_numerical_processing() {
    let mut h = loaded();
    let ctx = h.ctx.clone();
    let capabilities = h
        .state_mut()
        .command(
            spa_control::Command {
                method: "capabilities".into(),
                params: serde_json::json!({}),
            },
            &ctx,
        )
        .unwrap();
    assert!(
        capabilities["commands"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| !c["method"].as_str().unwrap().starts_with("atr."))
    );
    for (method, params) in [
        ("atr.apply", serde_json::json!({})),
        ("inspect", serde_json::json!({"scope":"preview"})),
        (
            "export",
            serde_json::json!({"format":"json","preview":true}),
        ),
    ] {
        assert!(
            h.state_mut()
                .command(
                    spa_control::Command {
                        method: method.into(),
                        params
                    },
                    &ctx
                )
                .is_err()
        );
    }
    // Stored correction metadata remains readable without a correction engine.
    let object = api(
        &mut h,
        "inspect",
        serde_json::json!({"scope":"object","index":2}),
    );
    assert_eq!(object["data"]["classId"], 159);
}

#[test]
fn both_plot_views_share_control_order_and_legend_sizing() {
    let mut h = loaded();
    for width in [1500.0, 800.0] {
        h.set_size(egui::vec2(width, 1100.0));
        let mut legend_sizes = Vec::new();
        let mut control_layouts = Vec::new();
        for overlay in [false, true] {
            h.state_mut().overlay = overlay;
            h.run_steps(3);
            let source = h
                .get_by_label(if overlay { "Overlay units" } else { "Series" })
                .rect();
            let reversed = h.get_by_label("Reverse X axis").rect();
            let min = h.get_by_label("X min").rect();
            let max = h.get_by_label("X max").rect();
            let zoom = h.get_by_label("Zoom in").rect();
            let reset = h.get_by_label("Full range").rect();
            let preset = h.get_by_label("1500–700").rect();
            assert!(source.center().y <= reversed.center().y);
            assert!(reversed.bottom() < min.top());
            assert!(min.center().y == max.center().y);
            assert!(reset.top() >= zoom.top());
            assert!(preset.top() > min.bottom());
            let origin = egui::pos2(min.left(), source.top());
            control_layouts.push([
                reversed.min - origin,
                min.min - origin,
                max.min - origin,
                zoom.min - origin,
                reset.min - origin,
                preset.min - origin,
            ]);
            let name = if overlay {
                h.state().overlay_plot.trace_names().next().unwrap()
            } else {
                h.state().files[0].plot.trace_names().next().unwrap()
            }
            .to_owned();
            let sidebar_right =
                egui::containers::panel::PanelState::load(&h.ctx, egui::Id::new("files"))
                    .unwrap()
                    .outer_rect
                    .right();
            let legend = h
                .query_all_by_role_and_label(accesskit::Role::Button, &name)
                .find(|node| node.rect().left() > sidebar_right)
                .expect("Plot legend")
                .rect();
            assert!(legend.top() > preset.bottom());
            assert!(legend.width() <= 280.0);
            assert_eq!(legend.height(), 34.0);
            legend_sizes.push(legend.size());
            h.get_by_label("Reverse X axis").click();
            h.run_steps(3);
            if overlay {
                assert_eq!(h.state().overlay_reverse, Some(false));
            } else {
                assert!(!h.state().files[0].reverse_x);
            }
            // Restore axis direction so the next viewport starts identically.
            h.get_by_label("Reverse X axis").click();
            h.run_steps(3);
        }
        assert_eq!(legend_sizes[0], legend_sizes[1]);
        assert_eq!(
            control_layouts[0], control_layouts[1],
            "Plot control layout at width {width}"
        );
    }
}
