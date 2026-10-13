//! Shared commands and local transports for the native and browser inspectors.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::mpsc;

pub const DEFAULT_PORT: u16 = 8091;
pub type Reply = Result<Value, String>;
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Command {
    pub method: String,
    #[serde(default)]
    pub params: Value,
}
pub struct Envelope {
    pub command: Command,
    pub reply: mpsc::Sender<Reply>,
}
#[cfg(not(target_arch = "wasm32"))]
pub mod mcp;
#[cfg(not(target_arch = "wasm32"))]
pub mod transport;

fn schema(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
/// The same catalog drives MCP discovery and the application's capabilities.
pub fn catalog() -> Vec<Value> {
    let integer = json!({"type":"integer","minimum":0});
    let number = json!({"type":"number"});
    let text = json!({"type":"string"});
    let boolean = json!({"type":"boolean"});
    let specs = vec![
        (
            "state",
            "Inspect loaded files, active series/view, plot bounds, visible traces, theme.",
            schema(json!({}), &[]),
            true,
        ),
        (
            "files.open",
            "Load local SPA files. MCP/CLI accepts paths; the app API accepts files with name and base64 data. The browser allows at most 32 open files; desktop has no file-count limit.",
            schema(
                json!({"paths":{"type":"array","items":text,"minItems":1}}),
                &["paths"],
            ),
            false,
        ),
        (
            "files.select",
            "Select a loaded file and optionally a series.",
            schema(json!({"file":integer,"series":integer}), &["file"]),
            false,
        ),
        (
            "files.close",
            "Close one file and release its data.",
            schema(json!({"file":integer}), &["file"]),
            false,
        ),
        (
            "files.clear",
            "Close all files.",
            schema(json!({}), &[]),
            false,
        ),
        (
            "view.set",
            "Select individual or overlay view, included files, units and X direction.",
            schema(
                json!({"mode":{"enum":["individual","overlay"]},"includedFiles":{"type":"array","items":integer},"xUnitCode":integer,"yUnitCode":integer,"reverseX":boolean}),
                &["mode"],
            ),
            false,
        ),
        (
            "plot.set_range",
            "Set a finite X interval; Y fits visible traces.",
            schema(json!({"min":number,"max":number}), &["min", "max"]),
            false,
        ),
        (
            "plot.zoom",
            "Zoom X about a coordinate or the current interval center.",
            schema(
                json!({"factor":{"type":"number","exclusiveMinimum":0},"center":number}),
                &["factor"],
            ),
            false,
        ),
        (
            "plot.reset",
            "Restore the complete range.",
            schema(json!({}), &[]),
            false,
        ),
        (
            "plot.preset",
            "Apply or preview one of the three wavenumber sections. Preview does not change zoom.",
            schema(
                json!({"section":{"enum":["4000-2500","2500-1500","1500-700"]},"preview":boolean,"clearPreview":boolean}),
                &[],
            ),
            false,
        ),
        (
            "plot.trace",
            "Show or hide a trace by its index in state.plot.traces.",
            schema(
                json!({"trace":integer,"visible":boolean}),
                &["trace", "visible"],
            ),
            false,
        ),
        (
            "plot.sample",
            "Inspect the nearest stored sample at an X coordinate.",
            schema(json!({"trace":integer,"x":number}), &["trace", "x"]),
            true,
        ),
        (
            "inspect",
            "Inspect decoded documents, series, raw sections or objects. Large arrays/payloads are opt-in.",
            schema(
                json!({"scope":{"enum":["document","series","section","object"]},"file":integer,"index":integer,"includeData":boolean,"includePayload":boolean}),
                &["scope"],
            ),
            true,
        ),
        (
            "export",
            "Export original CSV/JSON. Output is written by the CLI/MCP process and refuses overwrite by default.",
            schema(
                json!({"file":integer,"format":{"enum":["csv","json"]},"output":text,"overwrite":boolean}),
                &["format", "output"],
            ),
            false,
        ),
        (
            "preferences",
            "Set text encoding or appearance. System follows the desktop/browser theme.",
            schema(
                json!({"encoding":{"enum":["auto","utf-8","windows-1252","latin1"]},"theme":{"enum":["system","dark","light"]}}),
                &[],
            ),
            false,
        ),
        (
            "ui.elements",
            "Inspect every currently rendered accessibility node, including names, values, bounds and actions.",
            schema(json!({}), &[]),
            true,
        ),
        (
            "ui.perform",
            "Interact with a node ID from ui.elements or inject graph pointer/key events. Coordinates use egui points.",
            schema(
                json!({"id":text,"action":{"enum":["click","focus","setValue","increment","decrement","scrollIntoView","hover","pointerDown","pointerMove","pointerUp","wheel","key"]},"value":{},"x":number,"y":number,"deltaY":number,"key":text,"ctrl":boolean,"shift":boolean}),
                &["action"],
            ),
            false,
        ),
        (
            "ui.screenshot",
            "Capture the rendered application as PNG. MCP returns image content or saves it to an optional local output path.",
            schema(json!({"output":text,"overwrite":boolean}), &[]),
            false,
        ),
        (
            "capabilities",
            "List the full command catalog and schemas.",
            schema(json!({}), &[]),
            true,
        ),
    ];
    specs
        .into_iter()
        .map(|(method, description, input_schema, read_only)| {
            let app_schema = match method {
                "files.open" => {
                    let mut files = json!({"type":"array","minItems":1,"items":schema(json!({"name":text,"dataBase64":text}), &["name","dataBase64"])});
                    if cfg!(target_arch = "wasm32") {
                        files["maxItems"] = json!(32);
                    }
                    schema(json!({"files":files}), &["files"])
                },
                "export" => schema(json!({"file":integer,"format":{"enum":["csv","json"]}}), &["format"]),
                "ui.screenshot" => schema(json!({}), &[]),
                _ => input_schema.clone(),
            };
            json!({"method":method,"name":format!("spa_{}",method.replace('.',"_")),"description":description,"inputSchema":input_schema,"appInputSchema":app_schema,"annotations":{"readOnlyHint":read_only,"destructiveHint":false,"openWorldHint":false}})
        })
        .collect()
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
