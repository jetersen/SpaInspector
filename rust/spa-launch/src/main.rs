use std::{
    env,
    path::Path,
    process::{Command, ExitCode},
};

fn main() -> ExitCode {
    match launch() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn launch() -> Result<u8, String> {
    let mut args = env::args_os().skip(1);
    let mode = args.next().unwrap_or_default();
    if mode == "--help" || mode == "-h" || mode.is_empty() {
        println!(
            "Usage: cargo run desktop [--control-port PORT] | cargo run browser [Trunk serve options] | cargo run control [CLI/MCP options]\n\nDesktop opens the egui application. Browser serves it at http://127.0.0.1:8081.\nBrowser requires Trunk and the wasm32-unknown-unknown Rust target."
        );
        return Ok(0);
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or("Cannot locate the SPA workspace")?;
    let mut command = match mode.to_str() {
        Some("desktop") => {
            let mut command = Command::new(env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
            command
                .current_dir(root)
                .args(["run", "--locked", "-p", "spa-inspector"]);
            if !cfg!(debug_assertions) {
                command.arg("--release");
            }
            command.arg("--").args(args);
            command
        }
        Some("control") => {
            let mut command = Command::new(env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
            command
                .current_dir(root)
                .args(["run", "--quiet", "--locked", "-p", "spa-control"]);
            if !cfg!(debug_assertions) {
                command.arg("--release");
            }
            command.arg("--").args(args);
            command
        }
        Some("browser") => {
            let args = args.collect::<Vec<_>>();
            let port = args
                .windows(2)
                .find_map(|pair| {
                    (pair[0] == "--port" || pair[0] == "-p")
                        .then(|| pair[1].to_str()?.parse::<u16>().ok())
                        .flatten()
                })
                .or_else(|| {
                    args.iter()
                        .find_map(|arg| arg.to_str()?.strip_prefix("--port=")?.parse::<u16>().ok())
                })
                .unwrap_or(8081);
            let custom_address = args.iter().any(|arg| {
                arg == "--address"
                    || arg == "-a"
                    || arg
                        .to_str()
                        .is_some_and(|arg| arg.starts_with("--address="))
            });
            if !custom_address
                && !args.iter().any(|arg| arg == "--help" || arg == "-h")
                && let Err(error) =
                    std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
                && error.kind() == std::io::ErrorKind::AddrInUse
            {
                return Err(format!(
                    "Port {port} is already in use. Stop the other server or choose a port, for example `cargo run browser --port {}`.",
                    port.saturating_add(1)
                ));
            }
            let mut command = Command::new("trunk");
            command
                .current_dir(root.join("rust/spa-inspector"))
                .arg("serve")
                .args(args);
            // Trunk expects a boolean, while some terminal hosts set NO_COLOR=1.
            if env::var_os("NO_COLOR").is_some() {
                command.env("NO_COLOR", "true");
            }
            command
        }
        _ => {
            return Err(format!(
                "Unknown mode {:?}; use desktop, browser or control.",
                mode
            ));
        }
    };
    let status = command.status().map_err(|error| {
        if mode == "browser" && error.kind() == std::io::ErrorKind::NotFound {
            "Trunk was not found. Install it with `cargo install trunk --version 0.21.14 --locked`, then run `rustup target add wasm32-unknown-unknown`.".into()
        } else { format!("Could not launch {:?}: {error}", mode) }
    })?;
    Ok(status
        .code()
        .and_then(|code| u8::try_from(code).ok())
        .unwrap_or(1))
}
