# Rust app control

The desktop and browser applications share a command API. `spa-control` exposes it as a CLI and a stdio MCP server. Commands operate on the running app, including its selected file and graph. The conversion-only CLI remains available as `spa-convert`.

## Desktop

Start a desktop instance with its optional localhost endpoint:

```sh
cargo run desktop --control-port 8091
```

In another terminal:

```sh
cargo run control status
cargo run control open tests/TestFiles/test1.spa tests/TestFiles/test2.spa
cargo run control call view.set '{"mode":"overlay","includedFiles":[0,1]}'
cargo run control call plot.set_range '{"min":700,"max":1500}'
cargo run control call plot.preset '{"section":"4000-2500","preview":true}'
cargo run control call plot.preset '{"clearPreview":true}'
cargo run control call ui.screenshot '{"output":"spectrum.png"}'
cargo run control call export '{"format":"json","output":"spectrum.json"}'
```

Shell quoting in these examples is for Bash. The compiled `spa-control` executable accepts the same arguments on all desktop platforms. Use `--port PORT` or `--endpoint http://127.0.0.1:PORT/control` to target another instance. `--control-port 0` selects a free desktop port and prints the selected endpoint to stderr. Desktop control is disabled unless requested.

The endpoint uses `POST /control` with JSON `{ "method": "state", "params": {} }`. Responses contain `{ "ok": true, "result": ... }` or `{ "ok": false, "error": "..." }`. This is the internal app transport, not an HTTP MCP server.

## Browser

Start the app and a local bridge in separate terminals:

```sh
cargo run browser
cargo run control --port 8092 bridge
```

Open `http://127.0.0.1:8081/?control=8092`. CLI and MCP clients then use port 8092 to control that tab. Each bridge accepts one connected tab. Start another bridge on a different port for another tab. Reload the page after restarting a bridge.

The bridge listens on `127.0.0.1` and accepts browser origins served over HTTP from `127.0.0.1` or `localhost`. Imports arrive as bytes through the bridge; they are parsed in the browser. Local files and export paths belong to the CLI/MCP process's machine. The optional endpoint accepts localhost clients without authentication and is intended for local automation. It does not enable remote control.

The browser also exposes a Promise API without the bridge:

```js
const api = window.spaInspector;
const capabilities = await api.invoke({method: "capabilities", params: {}});
await api.invoke({method: "files.open", params: {
  files: [{name: "sample.spa", dataBase64: "..."}]
}});
const state = await api.invoke({method: "state", params: {}});
const exported = await api.invoke({method: "export", params: {format: "csv"}});
// exported.text contains the CSV; this API does not write local paths.
const screenshot = await api.invoke({method: "ui.screenshot", params: {}});
// screenshot.dataBase64 contains PNG bytes.
```

`capabilities.commands` supplies `inputSchema` for CLI/MCP and `appInputSchema` for direct HTTP/JavaScript calls. Imports and UI actions return `queuedForFrame`; direct clients should poll `state` until `busy` is false and `frame` is greater than that value. After importing, wait another frame for graph caches to initialize. CLI/MCP performs this wait automatically. UI node IDs are decimal strings, preserving their full precision in JavaScript.

## MCP

Build the control executable:

```sh
cargo build --release -p spa-control
```

Add a stdio server to an MCP client's configuration, using an absolute path to the executable and the running app's control port:

```json
{
  "mcpServers": {
    "spa-inspector": {
      "command": "/absolute/path/SpaInspector/target/release/spa-control",
      "args": ["--port", "8091", "mcp"]
    }
  }
}
```

On Windows use the corresponding `spa-control.exe` path. The server implements initialization, ping, tool discovery and tool calls using newline-delimited JSON-RPC on stdin/stdout. Diagnostics go to stderr. Starting MCP does not start the app. Tools use names such as `spa_files_open`, `spa_plot_set_range` and `spa_ui_screenshot`. A screenshot without `output` returns MCP image content; with `output` it saves a PNG and returns its dimensions/path.

## Commands and inspection

Run `cargo run control capabilities` for the complete schemas.

| Commands | Behavior |
| --- | --- |
| `state`, `capabilities` | Active files/series, view, themes, encoding, graph bounds and command schemas. |
| `files.open`, `files.select`, `files.close`, `files.clear` | Import files, select a series or release loaded files. Browser: maximum 32 open files. Desktop: no file-count limit. Both: 64 MiB per file and 128 MiB across open files. |
| `view.set` | Individual/overlay view, included files, matching X/Y unit group and reversed X direction. |
| `plot.set_range`, `plot.zoom`, `plot.reset` | Exact X interval, zoom factor/center or full range. Y fits visible traces. |
| `plot.preset` | Apply or preview `4000-2500`, `2500-1500`, `1500-700`; clear a preview with `clearPreview`. Preview preserves the selected interval. |
| `plot.trace`, `plot.sample` | Trace visibility or nearest sample from a plotted trace, from the stored data. Trace indices come from `state.plot.traces`. |
| `inspect` | Document, series, directory section, embedded object. Request `includeData` for numerical arrays and `includePayload` for original bytes. |
| `export` | Original CSV/JSON. CLI/MCP requires `output` and refuses overwrite unless `overwrite` is true. |
| `preferences` | Text encoding and `system`, `light` or `dark` appearance. Encoding changes reparse retained input bytes. |
| `ui.elements`, `ui.perform`, `ui.screenshot` | Rendered accessibility tree, actual widget/graph input and PNG capture. |

Export results and errors appear as temporary, dismissible toasts. The latest operation result remains available in `state.status` after its toast expires or is dismissed.

`state.plot` includes the data domain, selected range, rendered range, hover preview, fitted Y range, visible point count, trace visibility and graph bounds. `inspect` returns decoded metadata and preserved undecoded sections/objects; it does not assign meanings to unverified fields.

For widget automation, query `ui.elements`, find a node by its label, then pass its string `id` to `ui.perform`. Nodes include role, current value, bounds, children, disabled state and supported actions. Requery after opening menus/collapsible sections or changing files; the tree contains currently rendered elements. For example:

```sh
cargo run control elements
cargo run control call ui.perform '{"id":"ID_FROM_ELEMENTS","action":"setValue","value":"1000"}'
cargo run control call ui.perform '{"id":"APPLY_INTERVAL_ID","action":"click"}'
```

Graph gestures use `pointerDown`, `pointerMove`, `pointerUp` and `wheel` with coordinates in egui points, from `state.plot.graphBounds`. `ui.elements.pixelsPerPoint` converts these coordinates to screenshot pixels. The selection remains visible while a pointer is held down, allowing screenshot inspection before release. Preset buttons support `hover` through their node bounds. Keyboard actions accept `key`, `ctrl` and `shift`.

## Verification

Use a dedicated test instance. The smoke check changes files, plot settings and preferences, writes artifacts outside the repository, then closes the test files:

```sh
cargo build -p spa-control
python3 tools/check-control.py --port 8091
# Use a connected browser bridge's port for the same browser check.
python3 tools/check-control.py --port 8092
```

The check discovers and exercises every MCP command. It verifies original byte
preservation, exports/overwrite protection, overlays, precise zoom, preset and
drag bands, accessibility editing, screenshot image content, and themes. Unit
tests separately exercise protocol errors, localhost-origin rejection,
transports and the application harness.
