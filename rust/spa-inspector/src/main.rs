#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod appearance;
#[cfg(target_arch = "wasm32")]
mod browser_control;
mod notifications;
mod platform;
mod plot;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    let mut port = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--control-port" => {
                port = Some(
                    args.next()
                        .and_then(|p| p.parse::<u16>().ok())
                        .ok_or_else(|| {
                            eframe::Error::AppCreation(
                                "--control-port requires a port number".into(),
                            )
                        })?,
                );
            }
            "--help" | "-h" => {
                println!(
                    "SPA Inspector desktop [--control-port PORT]\nThe optional control endpoint binds only to 127.0.0.1. Default CLI/MCP port: 8091."
                );
                return Ok(());
            }
            _ => {
                return Err(eframe::Error::AppCreation(
                    format!("Unknown option {arg}").into(),
                ));
            }
        }
    }

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 800.0])
            .with_min_inner_size([700.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "SPA Inspector",
        options,
        Box::new(move |cc| {
            let mut app = app::InspectorApp::new(cc);
            if let Some(port) = port {
                let port = app.enable_control(port, &cc.egui_ctx)?;
                eprintln!("Inspector control endpoint: http://127.0.0.1:{port}/control");
            }
            Ok(Box::new(app))
        }),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast;
    wasm_bindgen_futures::spawn_local(async {
        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
            return;
        };
        let Some(canvas) = document
            .get_element_by_id("spa-canvas")
            .and_then(|element| element.dyn_into::<web_sys::HtmlCanvasElement>().ok())
        else {
            return;
        };
        let result = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|cc| Ok(Box::new(app::InspectorApp::new(cc)))),
            )
            .await;
        if let Some(loading) = document.get_element_by_id("loading") {
            match result {
                Ok(()) => loading.remove(),
                Err(error) => loading
                    .set_text_content(Some(&format!("Could not start SPA Inspector: {error:?}"))),
            }
        }
    });
}
