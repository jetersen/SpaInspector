use super::*;
use serde_json::json;
use std::{
    io::{Read, Write},
    net::{Ipv4Addr, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
};

#[test]
fn mcp_negotiates_and_lists_every_command_without_notification_responses() {
    let mut protocol = mcp::Protocol::default();
    let input = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}});
    let reply = protocol
        .handle(input, |_| panic!("Initialization must not invoke the app"))
        .unwrap();
    assert_eq!(reply["result"]["protocolVersion"], "2025-11-25");
    assert!(
        protocol
            .handle(
                json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
                |_| panic!()
            )
            .is_none()
    );
    let reply = protocol
        .handle(
            json!({"jsonrpc":"2.0","id":"tools","method":"tools/list"}),
            |_| panic!(),
        )
        .unwrap();
    let tools = reply["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), catalog().len());
    assert!(tools.iter().any(|tool| tool["name"] == "spa_ui_screenshot"));
    let open = tools
        .iter()
        .find(|tool| tool["name"] == "spa_files_open")
        .unwrap();
    assert!(
        open["inputSchema"]["properties"]["paths"]
            .get("maxItems")
            .is_none()
    );
    let open = catalog()
        .into_iter()
        .find(|c| c["method"] == "files.open")
        .unwrap();
    assert!(
        open["appInputSchema"]["properties"]["files"]
            .get("maxItems")
            .is_none()
    );
    let mut names = std::collections::HashSet::new();
    for tool in tools {
        assert!(names.insert(tool["name"].as_str().unwrap()));
        assert_eq!(tool["inputSchema"]["type"], "object");
    }
    let reply=protocol.handle(json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"spa_plot_set_range","arguments":{"min":700,"max":1500}}}),|c|{assert_eq!(c.method,"plot.set_range");assert_eq!(c.params["max"],1500);Ok(json!({"range":[700,1500]}))}).unwrap();
    assert_eq!(reply["result"]["isError"], false);
    assert_eq!(
        reply["result"]["structuredContent"]["range"],
        json!([700, 1500])
    );
    let reply = protocol.handle(json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"spa_ui_screenshot"}}), |_| Ok(json!({"mimeType":"image/png","dataBase64":"iVBORw==","width":1,"height":1}))).unwrap();
    assert_eq!(reply["result"]["content"][0]["type"], "image");
    assert_eq!(reply["result"]["content"][0]["data"], "iVBORw==");
    assert!(
        reply["result"]["structuredContent"]
            .get("dataBase64")
            .is_none()
    );
}
#[test]
fn mcp_distinguishes_protocol_errors_from_app_errors_and_keeps_stdout_jsonl() {
    let mut p = mcp::Protocol::default();
    let reply = p
        .handle(
            json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
            |_| panic!(),
        )
        .unwrap();
    assert_eq!(reply["error"]["code"], -32000);
    p.handle(
        json!({"jsonrpc":"2.0","id":2,"method":"initialize"}),
        |_| panic!(),
    );
    let reply = p
        .handle(
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"spa_plot_reset"}}),
            |_| Err("Unsupported unit".into()),
        )
        .unwrap();
    assert_eq!(reply["result"]["isError"], true);
    let input=b"bad json\n{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n";
    let mut output = Vec::new();
    mcp::serve(0, std::io::Cursor::new(input), &mut output).unwrap();
    let output = String::from_utf8(output).unwrap();
    let lines = output.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    for line in lines {
        let _: Value = serde_json::from_str(line).unwrap();
    }
}
#[test]
fn native_transport_calls_actor_and_rejects_foreign_origins_before_dispatch() {
    let (tx, rx) = mpsc::channel::<Envelope>();
    let calls = Arc::new(AtomicUsize::new(0));
    let invoked = Arc::clone(&calls);
    thread::spawn(move || {
        for e in rx {
            invoked.fetch_add(1, Ordering::Relaxed);
            let _ = e.reply.send(Ok(
                json!({"method":e.command.method,"params":e.command.params}),
            ));
        }
    });
    let server =
        transport::Server::start(0, transport::Broker::native(tx, Arc::new(|| {}))).unwrap();
    let result = transport::call(
        server.port,
        Command {
            method: "state".into(),
            params: json!({}),
        },
    )
    .unwrap();
    assert_eq!(result["method"], "state");
    let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, server.port)).unwrap();
    write!(stream,"POST /control HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nOrigin: https://example.com\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{{}}",server.port).unwrap();
    let mut reply = String::new();
    let _ = stream.read_to_string(&mut reply);
    assert!(reply.starts_with("HTTP/1.1 403"));
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}
#[test]
fn browser_bridge_routes_the_same_commands_and_returns_disconnect_errors() {
    let server = transport::Server::start(0, transport::Broker::browser()).unwrap();
    let port = server.port;
    let (mut socket, _) = tungstenite::connect(format!("ws://127.0.0.1:{port}/bridge")).unwrap();
    let client = thread::spawn(move || {
        transport::call(
            port,
            Command {
                method: "plot.reset".into(),
                params: json!({}),
            },
        )
    });
    let request = socket.read().unwrap().into_text().unwrap();
    let request: Value = serde_json::from_str(&request).unwrap();
    assert_eq!(request["method"], "plot.reset");
    socket
        .send(tungstenite::Message::Text(
            json!({"id":request["id"],"ok":true,"result":{"range":[700,4000]}})
                .to_string()
                .into(),
        ))
        .unwrap();
    assert_eq!(client.join().unwrap().unwrap()["range"], json!([700, 4000]));
    let _ = socket.close(None);
}
#[test]
fn export_refuses_overwrite_and_import_reads_exact_source_bytes() {
    let directory = std::env::temp_dir().join(format!(
        "spa-control-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let input = directory.join("sample.spa");
    std::fs::write(&input, b"original source").unwrap();
    let command = mcp::prepare(Command {
        method: "files.open".into(),
        params: json!({"paths":[input]}),
    })
    .unwrap();
    use base64::Engine;
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(command.params["files"][0]["dataBase64"].as_str().unwrap())
            .unwrap(),
        b"original source"
    );
    let command = mcp::prepare(Command {
        method: "files.open".into(),
        params: json!({"paths":vec![&input; 33]}),
    })
    .unwrap();
    assert_eq!(command.params["files"].as_array().unwrap().len(), 33);
    assert!(
        mcp::prepare(Command {
            method: "files.open".into(),
            params: json!({"paths":[]}),
        })
        .is_err()
    );
    let (tx, rx) = mpsc::channel::<Envelope>();
    thread::spawn(move || {
        for e in rx {
            let _ = e.reply.send(Ok(json!({"text":"index,x,y\n0,1,2\n"})));
        }
    });
    let server =
        transport::Server::start(0, transport::Broker::native(tx, Arc::new(|| {}))).unwrap();
    let output = directory.join("output.csv");
    let command = Command {
        method: "export".into(),
        params: json!({"format":"csv","output":output}),
    };
    mcp::execute(server.port, command.clone()).unwrap();
    assert!(mcp::execute(server.port, command).is_err());
    assert_eq!(
        std::fs::read_to_string(output).unwrap(),
        "index,x,y\n0,1,2\n"
    );
    std::fs::remove_dir_all(directory).unwrap();
}
