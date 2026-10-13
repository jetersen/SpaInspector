use std::{env, fs, process::ExitCode};

use spa_reader::{ReadOptions, TextEncoding, parse};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("spa-convert: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!(
            "Usage: spa-convert INPUT.spa OUTPUT.csv|OUTPUT.json [auto|utf-8|windows-1252|latin1]\nReads SPA locally; output is created only if it does not already exist."
        );
        return Ok(());
    }
    if !(2..=3).contains(&args.len()) {
        return Err("expected input and output paths; use --help".into());
    }
    let text_encoding = match args.get(2).map(String::as_str).unwrap_or("auto") {
        "auto" => TextEncoding::Auto,
        "utf-8" => TextEncoding::Utf8,
        "windows-1252" => TextEncoding::Windows1252,
        "latin1" => TextEncoding::Latin1,
        _ => return Err("encoding must be auto, utf-8, windows-1252 or latin1".into()),
    };
    let bytes = fs::read(&args[0])?;
    let document = parse(&bytes, &ReadOptions { text_encoding })?;
    let extension = std::path::Path::new(&args[1])
        .extension()
        .and_then(|s| s.to_str());
    let output = match extension {
        Some("csv") => document.to_csv(),
        Some("json") => document.to_json()?,
        _ => return Err("output extension must be .csv or .json".into()),
    };
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[1])?;
    file.write_all(output.as_bytes())?;
    for warning in &document.warnings {
        eprintln!("warning: {warning}");
    }
    Ok(())
}
