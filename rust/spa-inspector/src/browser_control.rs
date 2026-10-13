//! Browser control surface and optional localhost WebSocket bridge.
use eframe::{
    egui,
    wasm_bindgen::{JsCast, JsValue, closure::Closure},
};
use serde_json::{Value, json};
use spa_control::{Command, Envelope, Reply};
use std::{cell::RefCell, sync::mpsc};

struct Pending {
    receiver: mpsc::Receiver<Reply>,
    resolve: js_sys::Function,
    reject: js_sys::Function,
}
thread_local! {static PENDING:RefCell<Vec<Pending>>=const{RefCell::new(Vec::new())};}

pub fn install(sender: mpsc::Sender<Envelope>, ctx: egui::Context) {
    let invoke = Closure::wrap(Box::new(move |request: JsValue| -> js_sys::Promise {
        js_sys::Promise::new(&mut |resolve, reject| {
            let command = js_sys::JSON::stringify(&request)
                .map_err(|_| "Command is not serializable".to_string())
                .and_then(|text| {
                    serde_json::from_str::<Command>(&String::from(text)).map_err(|e| e.to_string())
                });
            match command {
                Ok(command) => {
                    let (tx, rx) = mpsc::channel();
                    if sender.send(Envelope { command, reply: tx }).is_err() {
                        let _ = reject.call1(
                            &JsValue::UNDEFINED,
                            &JsValue::from_str("Inspector disconnected"),
                        );
                        return;
                    }
                    PENDING.with(|p| {
                        p.borrow_mut().push(Pending {
                            receiver: rx,
                            resolve,
                            reject,
                        })
                    });
                    ctx.request_repaint();
                }
                Err(error) => {
                    let _ = reject.call1(&JsValue::UNDEFINED, &JsValue::from_str(&error));
                }
            }
        })
    }) as Box<dyn FnMut(JsValue) -> js_sys::Promise>);
    let object = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&object, &JsValue::from_str("invoke"), invoke.as_ref());
    let _ = js_sys::Reflect::set(
        &js_sys::global(),
        &JsValue::from_str("spaInspector"),
        &object,
    );
    invoke.forget();
    connect_bridge();
}
pub fn flush() {
    PENDING.with(|pending| {
        pending
            .borrow_mut()
            .retain(|p| match p.receiver.try_recv() {
                Ok(Ok(value)) => {
                    let result = js_sys::JSON::parse(&value.to_string()).unwrap_or(JsValue::NULL);
                    let _ = p.resolve.call1(&JsValue::UNDEFINED, &result);
                    false
                }
                Ok(Err(error)) => {
                    let _ = p
                        .reject
                        .call1(&JsValue::UNDEFINED, &JsValue::from_str(&error));
                    false
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    let _ = p.reject.call1(
                        &JsValue::UNDEFINED,
                        &JsValue::from_str("Inspector disconnected"),
                    );
                    false
                }
                Err(mpsc::TryRecvError::Empty) => true,
            })
    });
}
fn connect_bridge() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let search = window.location().search().unwrap_or_default();
    let Some(port) = search
        .trim_start_matches('?')
        .split('&')
        .find_map(|p| p.strip_prefix("control=")?.parse::<u16>().ok())
        .filter(|p| *p != 0)
    else {
        return;
    };
    let Ok(socket) = web_sys::WebSocket::new(&format!("ws://127.0.0.1:{port}/bridge")) else {
        return;
    };
    let sender = socket.clone();
    let callback = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
        let Some(text) = event.data().as_string() else {
            return;
        };
        let Ok(request) = serde_json::from_str::<Value>(&text) else {
            return;
        };
        let id = request["id"].clone();
        let sender = sender.clone();
        let invoke = js_sys::Reflect::get(
            &js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("spaInspector"))
                .unwrap_or(JsValue::NULL),
            &JsValue::from_str("invoke"),
        )
        .ok()
        .and_then(|v| v.dyn_into::<js_sys::Function>().ok());
        let result = invoke
            .and_then(|f| {
                js_sys::JSON::parse(&request.to_string())
                    .ok()
                    .and_then(|v| f.call1(&JsValue::UNDEFINED, &v).ok())
            })
            .and_then(|v| v.dyn_into::<js_sys::Promise>().ok());
        if let Some(promise) = result {
            wasm_bindgen_futures::spawn_local(async move {
                let reply = match wasm_bindgen_futures::JsFuture::from(promise).await {
                    Ok(value) => match js_sys::JSON::stringify(&value)
                        .ok()
                        .and_then(|s| serde_json::from_str::<Value>(&String::from(s)).ok())
                    {
                        Some(value) => json!({"id":id,"ok":true,"result":value}),
                        None => json!({"id":id,"ok":false,"error":"Invalid browser result"}),
                    },
                    Err(error) => {
                        json!({"id":id,"ok":false,"error":error.as_string().unwrap_or_else(||"Browser command failed".into())})
                    }
                };
                let _ = sender.send_with_str(&reply.to_string());
            });
        }
    }) as Box<dyn FnMut(web_sys::MessageEvent)>);
    socket.set_onmessage(Some(callback.as_ref().unchecked_ref()));
    callback.forget();
    // The event handler retains its socket until the page closes.
    let _ = js_sys::Reflect::set(
        &js_sys::global(),
        &JsValue::from_str("spaInspectorBridge"),
        socket.as_ref(),
    );
}
