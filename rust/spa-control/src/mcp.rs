//! MCP stdio framing; internal HTTP/WS transport is an app API, not HTTP MCP.
use crate::{Command, Reply, catalog, transport};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::{self, BufRead, Read, Write},
    path::Path,
    thread,
    time::{Duration, Instant},
};

pub fn prepare(mut command: Command) -> Result<Command, String> {
    if command.method == "files.open"
        && let Some(paths) = command.params.get("paths").and_then(Value::as_array)
    {
        if paths.is_empty() {
            return Err("Choose at least one path".into());
        }
        let mut files = Vec::new();
        let mut total = 0_u64;
        for path in paths {
            let path = Path::new(path.as_str().ok_or("Each path must be a string")?);
            let file =
                fs::File::open(path).map_err(|e| format!("Cannot open {}: {e}", path.display()))?;
            let size = file.metadata().map_err(|e| e.to_string())?.len();
            if size > 64 * 1024 * 1024 || total + size > 128 * 1024 * 1024 {
                return Err("Import exceeds 64 MiB per file or 128 MiB per selection".into());
            }
            let mut bytes = Vec::new();
            file.take(size + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() as u64 != size {
                return Err("File changed while reading; retry the import".into());
            }
            total += size;
            files.push(json!({"name":path.file_name().ok_or("Missing file name")?.to_string_lossy(),"dataBase64":STANDARD.encode(bytes)}));
        }
        command.params = json!({"files":files});
    }
    Ok(command)
}
pub fn execute(port: u16, command: Command) -> Reply {
    let method = command.method.clone();
    let original = command.params.clone();
    let result = transport::call(port, prepare(command)?)?;
    if method == "files.open" || method == "ui.perform" {
        let start = Instant::now();
        let after = result
            .get("queuedForFrame")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let mut ready_frame = None;
        loop {
            let state = transport::call(
                port,
                Command {
                    method: "state".into(),
                    params: json!({}),
                },
            )?;
            let frame = state["frame"].as_u64().unwrap_or(0);
            if state["busy"] == false && frame > after {
                if method == "ui.perform" || ready_frame.is_some_and(|previous| frame > previous) {
                    return Ok(state);
                }
                ready_frame = Some(frame);
            }
            if start.elapsed() > Duration::from_secs(30) {
                return Err("Timed out waiting for the inspector".into());
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
    if method == "ui.screenshot"
        && let Some(path) = original["output"].as_str()
    {
        let bytes = STANDARD
            .decode(result["dataBase64"].as_str().ok_or("No screenshot data")?)
            .map_err(|e| e.to_string())?;
        let mut options = OpenOptions::new();
        options.write(true);
        if original["overwrite"] == true {
            options.create(true).truncate(true);
        } else {
            options.create_new(true);
        }
        options
            .open(path)
            .and_then(|mut f| f.write_all(&bytes))
            .map_err(|e| format!("Cannot save screenshot: {e}"))?;
        return Ok(
            json!({"path":path,"mimeType":"image/png","width":result["width"],"height":result["height"]}),
        );
    }
    if method == "export" {
        let path = original["output"]
            .as_str()
            .ok_or("Export requires an output path")?;
        let text = result["text"]
            .as_str()
            .ok_or("Inspector returned no export text")?;
        let overwrite = original["overwrite"].as_bool().unwrap_or(false);
        let mut options = OpenOptions::new();
        options.write(true);
        if overwrite {
            options.create(true).truncate(true);
        } else {
            options.create_new(true);
        }
        let mut file = options
            .open(path)
            .map_err(|e| format!("Cannot write {path}: {e}"))?;
        file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        return Ok(json!({"path":path,"bytes":text.len(),"format":original["format"]}));
    }
    Ok(result)
}
fn error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}
#[derive(Default)]
pub struct Protocol {
    initialized: bool,
}
impl Protocol {
    pub fn handle(
        &mut self,
        request: Value,
        mut invoke: impl FnMut(Command) -> Reply,
    ) -> Option<Value> {
        let id = request.get("id").cloned();
        let Some(method) = request["method"].as_str() else {
            return Some(error(
                id.unwrap_or(Value::Null),
                -32600,
                "Invalid JSON-RPC request",
            ));
        };
        if request["jsonrpc"] != "2.0" {
            return Some(error(
                id.unwrap_or(Value::Null),
                -32600,
                "Expected JSON-RPC 2.0",
            ));
        }
        let id = id?;
        let result = match method {
            "initialize" => {
                self.initialized = true;
                let proposed = request["params"]["protocolVersion"]
                    .as_str()
                    .unwrap_or_default();
                let version = if ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"]
                    .contains(&proposed)
                {
                    proposed
                } else {
                    "2025-11-25"
                };
                json!({"protocolVersion":version,"capabilities":{"tools":{"listChanged":false}},"serverInfo":{"name":"spa-inspector-control","version":env!("CARGO_PKG_VERSION")},"instructions":"Controls a running local Rust SPA Inspector. Use spa_state and spa_ui_elements to inspect before changing its view. File paths and export paths belong to this MCP process's machine."})
            }
            "ping" => json!({}),
            _ if !self.initialized => {
                return Some(error(id, -32000, "Initialize the MCP session first"));
            }
            "tools/list" => {
                let tools = catalog()
                    .into_iter()
                    .map(|mut tool| {
                        tool.as_object_mut().unwrap().remove("method");
                        tool.as_object_mut().unwrap().remove("appInputSchema");
                        tool
                    })
                    .collect::<Vec<_>>();
                json!({"tools":tools})
            }
            "tools/call" => {
                let name = request["params"]["name"].as_str().unwrap_or_default();
                let Some(tool) = catalog().into_iter().find(|t| t["name"] == name) else {
                    return Some(error(id, -32602, "Unknown tool"));
                };
                let params = request["params"]
                    .get("arguments")
                    .cloned()
                    .unwrap_or(json!({}));
                if !params.is_object() {
                    return Some(error(id, -32602, "Tool arguments must be an object"));
                }
                match invoke(Command {
                    method: tool["method"].as_str().unwrap().into(),
                    params,
                }) {
                    Ok(mut value) => {
                        if value["mimeType"] == "image/png"
                            && let Some(data) = value["dataBase64"].as_str().map(str::to_owned)
                        {
                            value.as_object_mut().unwrap().remove("dataBase64");
                            json!({"content":[{"type":"image","mimeType":"image/png","data":data}],"structuredContent":value,"isError":false})
                        } else {
                            json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":false})
                        }
                    }
                    Err(message) => {
                        json!({"content":[{"type":"text","text":message}],"isError":true})
                    }
                }
            }
            _ => return Some(error(id, -32601, "Method not found")),
        };
        Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
    }
}
pub fn serve(port: u16, mut input: impl BufRead, mut output: impl Write) -> io::Result<()> {
    let mut protocol = Protocol::default();
    loop {
        let mut line = String::new();
        if input
            .by_ref()
            .take((transport::MAX_BODY + 1) as u64)
            .read_line(&mut line)?
            == 0
        {
            return Ok(());
        }
        if line.len() > transport::MAX_BODY {
            return Err(io::Error::other("MCP message is too large"));
        }
        let response = match serde_json::from_str(&line) {
            Ok(request) => protocol.handle(request, |c| execute(port, c)),
            Err(_) => Some(error(Value::Null, -32700, "Invalid JSON")),
        };
        if let Some(response) = response {
            serde_json::to_writer(&mut output, &response)?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
    }
}
