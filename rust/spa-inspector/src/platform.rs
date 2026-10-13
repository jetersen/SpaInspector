use spa_reader::{Document, ReadOptions};
use std::sync::{Arc, mpsc::Sender};

pub const MAX_FILE_BYTES: usize = 64 * 1024 * 1024;
// Bound browser memory and UI work without imposing a desktop file-count cap.
pub const MAX_OPEN_FILES: Option<usize> = if cfg!(target_arch = "wasm32") {
    Some(32)
} else {
    None
};
pub const MAX_TOTAL_BYTES: usize = 128 * 1024 * 1024;

pub fn remaining_file_slots(open_files: usize) -> usize {
    MAX_OPEN_FILES.map_or(usize::MAX, |limit| limit.saturating_sub(open_files))
}

pub struct InputFile {
    pub name: String,
    pub data: Result<Vec<u8>, String>,
}

pub enum Event {
    Imported(Vec<InputFile>),
    Exported(Result<String, String>),
    Parsed {
        index: usize,
        result: Result<Box<Document>, String>,
    },
}

pub fn pick_files(sender: Sender<Event>, ctx: eframe::egui::Context, limit: usize, budget: usize) {
    let task = async move {
        let mut inputs = Vec::new();
        let mut remaining = budget;
        if let Some(files) = rfd::AsyncFileDialog::new()
            .set_title("Open SPA files")
            .add_filter("SPA spectra", &["spa", "SPA"])
            .pick_files()
            .await
        {
            for file in files.into_iter().take(limit) {
                let name = file.file_name();
                let data = read_file(&file, remaining).await;
                if let Ok(bytes) = &data {
                    remaining -= bytes.len();
                }
                inputs.push(InputFile { name, data });
            }
        }
        let _ = sender.send(Event::Imported(inputs));
        ctx.request_repaint();
    };
    spawn(task);
}

#[cfg(not(target_arch = "wasm32"))]
async fn read_file(file: &rfd::FileHandle, budget: usize) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let reader = std::fs::File::open(file.path()).map_err(|error| error.to_string())?;
    let size = reader.metadata().map_err(|error| error.to_string())?.len();
    if size > MAX_FILE_BYTES as u64 {
        return Err("File exceeds the 64 MiB limit".into());
    }
    if size > budget as u64 {
        return Err("Open files exceed the 128 MiB total limit".into());
    }
    let mut data = Vec::new();
    reader
        .take(budget.min(MAX_FILE_BYTES) as u64 + 1)
        .read_to_end(&mut data)
        .map_err(|error| error.to_string())?;
    if data.len() > budget {
        return Err("Open files exceed the 128 MiB total limit".into());
    }
    if data.len() > MAX_FILE_BYTES {
        return Err("File exceeds the 64 MiB limit".into());
    }
    Ok(data)
}

#[cfg(target_arch = "wasm32")]
async fn read_file(file: &rfd::FileHandle, budget: usize) -> Result<Vec<u8>, String> {
    if file.inner().size() > MAX_FILE_BYTES as f64 {
        return Err("File exceeds the 64 MiB limit".into());
    }
    if file.inner().size() > budget as f64 {
        return Err("Open files exceed the 128 MiB total limit".into());
    }
    let buffer = wasm_bindgen_futures::JsFuture::from(file.inner().array_buffer())
        .await
        .map_err(|error| format!("Could not read file: {error:?}"))?;
    Ok(js_sys::Uint8Array::new(&buffer).to_vec())
}

pub fn save_file(name: String, bytes: Vec<u8>, sender: Sender<Event>, ctx: eframe::egui::Context) {
    spawn(async move {
        let result = save(name, bytes).await;
        let _ = sender.send(Event::Exported(result));
        ctx.request_repaint();
    });
}

#[cfg(not(target_arch = "wasm32"))]
async fn save(name: String, bytes: Vec<u8>) -> Result<String, String> {
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_file_name(&name)
        .save_file()
        .await
    else {
        return Ok("Export cancelled".into());
    };
    file.write(&bytes)
        .await
        .map_err(|error| error.to_string())?;
    Ok(format!("Saved {}", file.file_name()))
}

#[cfg(target_arch = "wasm32")]
async fn save(name: String, bytes: Vec<u8>) -> Result<String, String> {
    use eframe::wasm_bindgen::JsCast;
    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or("Browser document unavailable")?;
    let array = js_sys::Array::new();
    array.push(&js_sys::Uint8Array::from(bytes.as_slice()));
    let blob = web_sys::Blob::new_with_u8_array_sequence(&array)
        .map_err(|error| format!("Cannot create download: {error:?}"))?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)
        .map_err(|error| format!("Cannot create download URL: {error:?}"))?;
    let result = (|| {
        let anchor = document
            .create_element("a")
            .map_err(|error| format!("Cannot create download link: {error:?}"))?
            .dyn_into::<web_sys::HtmlAnchorElement>()
            .map_err(|_| "Cannot create download link".to_string())?;
        anchor.set_href(&url);
        anchor.set_download(&name);
        let body = document.body().ok_or("Browser document body unavailable")?;
        body.append_child(&anchor)
            .map_err(|error| format!("Cannot attach download link: {error:?}"))?;
        anchor.click();
        anchor.remove();
        Ok(format!("Downloaded {name}"))
    })();
    let _ = web_sys::Url::revoke_object_url(&url);
    result
}

#[cfg(not(target_arch = "wasm32"))]
fn spawn(future: impl std::future::Future<Output = ()> + Send + 'static) {
    std::thread::spawn(move || pollster::block_on(future));
}

#[cfg(target_arch = "wasm32")]
fn spawn(future: impl std::future::Future<Output = ()> + 'static) {
    wasm_bindgen_futures::spawn_local(future);
}

pub fn parse_files(
    jobs: Vec<(usize, Arc<Vec<u8>>)>,
    options: ReadOptions,
    sender: Sender<Event>,
    ctx: eframe::egui::Context,
) {
    spawn(async move {
        for (index, bytes) in jobs {
            let result = spa_reader::parse(&bytes, &options)
                .map(Box::new)
                .map_err(|error| error.to_string());
            let _ = sender.send(Event::Parsed { index, result });
            ctx.request_repaint();
        }
    });
}
