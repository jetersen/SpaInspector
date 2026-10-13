#[cfg(not(target_arch = "wasm32"))]
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
#[cfg(target_arch = "wasm32")]
fn main() {}
#[cfg(not(target_arch = "wasm32"))]
fn run() -> Result<(), String> {
    use serde_json::json;
    use spa_control::{
        Command, DEFAULT_PORT, mcp,
        transport::{Broker, Server},
    };
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    let mut port = DEFAULT_PORT;
    if let Some(index) = args.iter().position(|a| a == "--port" || a == "--endpoint") {
        let value = args.get(index + 1).ok_or("Missing endpoint/port")?;
        port = if args[index] == "--endpoint" {
            value
                .strip_prefix("http://127.0.0.1:")
                .or_else(|| value.strip_prefix("http://localhost:"))
                .ok_or("Endpoint must be http://127.0.0.1:PORT")?
                .trim_end_matches('/')
                .trim_end_matches("/control")
                .parse()
        } else {
            value.parse()
        }
        .map_err(|_| "Invalid port")?;
        args.drain(index..=index + 1);
    }
    let mode = args.first().map(String::as_str).unwrap_or("help");
    match mode {
        "mcp" => mcp::serve(port, std::io::stdin().lock(), std::io::stdout().lock())
            .map_err(|e| e.to_string()),
        "bridge" => {
            let server = Server::start(port, Broker::browser()).map_err(|e| e.to_string())?;
            eprintln!(
                "Browser control bridge: http://127.0.0.1:{}/control; open the Rust app with ?control={}",
                server.port, server.port
            );
            loop {
                std::thread::park_timeout(std::time::Duration::from_secs(60));
            }
        }
        "status" | "elements" | "capabilities" => {
            let method = match mode {
                "status" => "state",
                "elements" => "ui.elements",
                _ => "capabilities",
            };
            let value = mcp::execute(
                port,
                Command {
                    method: method.into(),
                    params: json!({}),
                },
            )?;
            println!("{}", serde_json::to_string_pretty(&value).unwrap());
            Ok(())
        }
        "open" => {
            if args.len() < 2 {
                return Err("Usage: spa-control open FILE...".into());
            }
            let value = mcp::execute(
                port,
                Command {
                    method: "files.open".into(),
                    params: json!({"paths":args[1..]}),
                },
            )?;
            println!("{}", serde_json::to_string_pretty(&value).unwrap());
            Ok(())
        }
        "call" => {
            let method = args.get(1).ok_or("Usage: spa-control call METHOD [JSON]")?;
            let params = serde_json::from_str(args.get(2).map(String::as_str).unwrap_or("{}"))
                .map_err(|e| e.to_string())?;
            let value = mcp::execute(
                port,
                Command {
                    method: method.clone(),
                    params,
                },
            )?;
            println!("{}", serde_json::to_string_pretty(&value).unwrap());
            Ok(())
        }
        "help" | "--help" | "-h" => {
            println!(
                "spa-control [--port PORT | --endpoint http://127.0.0.1:PORT] status | elements | capabilities | open FILE... | call METHOD [JSON] | bridge | mcp\n\nDefault port: 8091. Enable the desktop app with --control-port 8091.\nBrowser: start bridge, then open the Rust app with ?control=8091.\nMCP mode uses JSON-RPC on stdio; diagnostics go to stderr."
            );
            Ok(())
        }
        _ => Err(format!("Unknown mode {mode}; use --help")),
    }
}
