# SpaInspector

Inspect OMNIC `.spa` spectra and saved interferograms locally, with CSV and JSON export. Rust egui supplies the desktop and browser interface; independent Rust and C# libraries read SPA data:

- `rust/spa-reader`: Rust library and `spa-convert` CLI.
- `rust/spa-inspector`: egui application for Linux, Windows, macOS, and browsers.
- `rust/spa-control`: CLI and stdio MCP control of the running Rust application.
- `src/SpaInspectorReader`: C# library with a shared export schema.
- `src/SpaFileReader`: C# library retaining the older accessor APIs.

The readers implement the ordinary clear SPA loading path recovered from OMNIC 9.2 and existing readers. They preserve the complete input, every directory section, and undecoded object data. This is not a claim of complete OMNIC compatibility. Special runtime spectrum types, some metadata meanings, other container formats, and broader background processing remain unverified. See the [format reference](docs/spa-format.md).

## Run

The workspace pins Rust 1.99.0 through `rust-toolchain.toml`; rustup selects it automatically. The C# reader libraries and tests use the .NET 10 SDK selected by `global.json`.

```sh
# Desktop egui application
cargo run desktop
# Plotting and reader code are optimized in development builds too.

# Browser egui application, using Trunk 0.21.14
cargo install trunk --version 0.21.14 --locked
rustup target add wasm32-unknown-unknown
cargo run browser
# http://127.0.0.1:8081
```

The egui app opens multiple local files, selects spectra or interferograms, displays metadata/history/sections, and exports all series. Choose Overlay spectra to compare loaded spectra with matching X/Y units on one graph. Each trace retains its own coordinates and sampling. Individual and overlay views share the plot controls, axis-direction row, card styling, and legend layout. Hover a truncated legend name to see it in full.

For a spectral section, enter X min/max and apply the interval, use a wavenumber preset, or drag horizontally across the graph. Wheel zoom affects X; Y fits the visible samples. Double-click or Full range restores the complete interval. Free panning is disabled.

The egui workspace follows the desktop/browser theme, with Light and Dark overrides in Preferences. Plot readouts and trace visibility controls sit above the graph. Export results and errors appear as dismissible toasts. The compact file sidebar can be resized or collapsed; reopening it restores its width. Right-click or Escape cancels an active drag selection. Hover a wavenumber preset to preview its selection band before clicking; moving away restores the selected zoom. Metadata appears in a side inspector on wide windows and below the graph in a scrolling workspace on narrow windows.

Browser files stay in memory on the device. The reverse-X control changes only the plot view. Exports retain the reader's coordinate order and full precision.

The Cargo launcher works from the workspace root. Browser options pass through to `trunk serve`, for example `cargo run browser --port 8082`. The browser address stays bound to localhost by default. If the port is occupied, stop its server or choose another port. Use `cargo run -- --help` for launcher usage.

For automation, start `cargo run desktop --control-port 8091`, then use `cargo run control status`, `cargo run control open FILE...`, or `cargo run control mcp`. Browser control uses a local bridge and the same commands. File/series selection, graph navigation, overlays, raw inspection, exports, accessibility actions and screenshots are available through the API. See [CLI, MCP and browser control](docs/control.md) for setup and the live smoke check.

## Convert or embed

The Rust CLI creates a new output file and refuses to overwrite an existing one. Optional text encodings are `auto`, `utf-8`, `windows-1252`, and `latin1`. C# exposes reader libraries directly.

```sh
cargo run --locked -p spa-reader --bin spa-convert -- input.spa output.json
cargo run --locked -p spa-reader --bin spa-convert -- input.spa output.csv windows-1252
```

```rust
let document = spa_reader::parse(&bytes, &spa_reader::ReadOptions::default())?;
let csv = document.to_csv();
let json = document.to_json()?;
```

```csharp
var document = SpaInspectorReader.SpaParser.Parse(bytes);
var csv = document.ToCsv();
var json = document.ToJson();
```

CSV contains one row per point across every supported series, with kind, selectors, units, X, and Y. JSON schema version 1 includes normalized numerical series, raw metadata, embedded objects, every section, and the complete original file as base64. NaN and infinities use named strings in JSON. The raw bytes preserve their original bit patterns.

The legacy `SpaFile` accessors now read without modifying their input. Returned byte spans are copies. Float replacement accepts the reader's order; raw-byte replacement accepts the stored order. Replacements must have exactly the original length. `SpgFile` keeps its independent legacy group accessor; full SPG semantics are outside the SPA reader's contract.

## Build and verify

```sh
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
dotnet test SpaInspector.sln -c Release
python3 tools/check-parity.py

# Static browser builds
(cd rust/spa-inspector && trunk build --release)
```

The egui application enables AccessKit. Its `egui_kittest` harness runs the real application, queries controls by accessible names, injects file selection and export destinations, and exercises graph gestures. Run it with `cargo test --locked -p spa-inspector`. Browser egui remains a canvas; the control API exposes its accessibility tree and rendered screenshots for automation.

Serve the generated `rust/spa-inspector/dist` directory over HTTP. For a subdirectory deployment, pass Trunk's `--public-url /path/`. GitHub Pages publishes the egui app at the repository's root URL.

The parity checker invokes the C# reader's test-only export hook and compares it with the Rust CLI; it does not require a C# application.

Set `SPA_CORPUS_DIR` to an additional sample directory to exercise it in both test suites and the parity checker. The checker compares JSON structure, original bytes, all decoded float32 values, coordinates, embedded attributes, and CSV values. Warning wording is implementation-specific. Keep samples outside the repository unless their redistribution terms are established.

## Publication boundary

This repository distributes the SPA reader, viewer, and original-data exporters.
Stored processing metadata is decoded and preserved; numerical corrections and
reprocessing are not included in the app, libraries, browser builds, or packages.

Run `python3 tools/check-public-boundary.py` before publishing. It rejects
processing source, research, fixtures, benchmarks, and private dependencies in
the public tree. The check is also part of CI.

## Downloads and releases

The browser viewer is hosted at https://jetersen.github.io/SpaInspector/.
The Desktop packages workflow builds a Windows x64 executable and per-user MSI,
Linux x86_64 AppImage and Flatpak bundle, and Intel and Apple Silicon macOS DMGs.
Main-branch builds and manual runs provide downloadable workflow artifacts.
Publish a GitHub release with a three-part version tag such as `v0.1.0` to attach
the packages and SHA-256 checksums to that release.

The MSI installs under the current user's local application directory. Install
the Flatpak bundle with `flatpak install --user ./SpaInspector-*.flatpak`; its
runtime comes from Flathub. Flatpak opens files through the desktop portal.
Windows packages are unsigned. macOS apps use an ad-hoc signature and are not
Apple-notarized; distributing trusted signed installers requires signing credentials.
NuGet packages are built on release; publishing them requires `NUGET_API_KEY`.
