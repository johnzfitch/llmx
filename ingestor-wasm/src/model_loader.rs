use burn::module::Module;
use burn::record::{BinBytesRecorder, FullPrecisionSettings, Recorder, RecorderError};
use burn::tensor::backend::Backend;
#[cfg(feature = "ndarray-backend")]
use burn_ndarray::NdArrayDevice;
#[cfg(feature = "wgpu-backend")]
use burn_wgpu::{Wgpu, WgpuDevice};
pub use crate::bert::Model;
use js_sys::{Date, Uint8Array};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::HashMap;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Event, IdbDatabase, IdbOpenDbRequest, IdbRequest, IdbTransactionMode, Request, RequestInit,
    RequestMode, Response,
};

pub const MODEL_ID: &str = "arctic-embed-s-q8-a80e2e953bcd";
const MODEL_CACHE_KEY: &str = "arctic-embed-s-q8-a80e2e953bcd.bin";
const MODEL_SHA256: &str = "a80e2e953bcd6a2cfe102043d84adfead9f21b4c2f89fa70527eebf4c2cf0821";
const MODEL_URL: &str = match option_env!("LLMX_EMBEDDING_MODEL_URL") {
    Some(value) => value,
    None => "",
};
const MAX_MODEL_BYTES: usize = 80 * 1024 * 1024;
const MIN_FETCH_INTERVAL_MS: f64 = 5_000.0;
const MAX_FETCH_RETRIES: u32 = 3;
const ALLOWED_MODEL_ORIGINS: [&str; 2] = ["https://cdn.jsdelivr.net/", "https://huggingface.co/"];

const DB_NAME: &str = "llmx-model-cache";
const STORE_NAME: &str = "models";
const DB_VERSION: u32 = 1;

thread_local! {
    static LAST_FETCH_MS: RefCell<HashMap<String, f64>> = RefCell::new(HashMap::new());
}

#[cfg(feature = "wgpu-backend")]
pub async fn load_model(device: &WgpuDevice) -> Result<Model<Wgpu>, JsValue> {
    let bytes = fetch_model_bytes().await?;
    model_from_bytes(&bytes, device).map_err(|err| js_error("Failed to load model", err))
}

#[cfg(feature = "ndarray-backend")]
pub async fn load_model_cpu(
    device: &NdArrayDevice,
) -> Result<Model<burn_ndarray::NdArray>, JsValue> {
    let bytes = fetch_model_bytes().await?;
    model_from_bytes(&bytes, device).map_err(|err| js_error("Failed to load model", err))
}

fn model_from_bytes<B: Backend>(bytes: &[u8], device: &B::Device) -> Result<Model<B>, RecorderError> {
    let recorder = BinBytesRecorder::<FullPrecisionSettings, Vec<u8>>::default();
    let record: <Model<B> as Module<B>>::Record = recorder.load(bytes.to_vec(), device)?;
    Ok(Model::new(device).load_record(record))
}

async fn fetch_model_bytes() -> Result<Vec<u8>, JsValue> {
    fetch_with_cache(
        MODEL_URL,
        MODEL_CACHE_KEY,
        MODEL_SHA256,
        &ALLOWED_MODEL_ORIGINS,
        MAX_MODEL_BYTES,
    )
    .await
}

pub(crate) async fn fetch_with_cache(
    url: &str,
    cache_key: &str,
    expected_sha256: &str,
    allowed_origins: &[&str],
    max_bytes: usize,
) -> Result<Vec<u8>, JsValue> {
    if let Some(bytes) = load_cached_bytes(cache_key).await? {
        if verify_sha256(&bytes, expected_sha256) {
            return Ok(bytes);
        }
        let _ = delete_cached_bytes(cache_key).await;
    }

    if url.is_empty() {
        return Err(JsValue::from_str("Resource URL not configured"));
    }

    validate_url_origin(url, allowed_origins)?;
    enforce_rate_limit(cache_key)?;

    let bytes = fetch_with_retry(url, expected_sha256, max_bytes, allowed_origins).await?;
    store_cached_bytes(cache_key, &bytes).await?;
    Ok(bytes)
}

fn validate_url_origin(url: &str, allowed_origins: &[&str]) -> Result<(), JsValue> {
    if is_same_origin_relative_url(url) {
        return Ok(());
    }
    if allowed_origins.iter().any(|origin| url.starts_with(origin)) {
        Ok(())
    } else {
        Err(JsValue::from_str("Invalid resource origin"))
    }
}

fn is_same_origin_relative_url(url: &str) -> bool {
    if url.starts_with("//") {
        return false;
    }
    url.starts_with('/') || url.starts_with("./")
}

fn validate_final_fetch_url(
    final_url: &str,
    requested_url: &str,
    allowed_origins: &[&str],
) -> Result<(), JsValue> {
    if !is_same_origin_relative_url(requested_url) {
        return validate_url_origin(final_url, allowed_origins);
    }

    let window = web_sys::window().ok_or_else(|| JsValue::from_str("No window object"))?;
    let origin = window.location().origin()?;
    if final_url.starts_with(&origin) {
        Ok(())
    } else {
        Err(JsValue::from_str("Redirected off-origin"))
    }
}

fn enforce_rate_limit(key: &str) -> Result<(), JsValue> {
    let now_ms = Date::now();
    let mut blocked = false;
    LAST_FETCH_MS.with(|map| {
        let mut map = map.borrow_mut();
        if let Some(last) = map.get(key) {
            if now_ms - *last < MIN_FETCH_INTERVAL_MS {
                blocked = true;
            } else {
                map.insert(key.to_string(), now_ms);
            }
        } else {
            map.insert(key.to_string(), now_ms);
        }
    });

    if blocked {
        Err(JsValue::from_str("Rate limit: please wait before retrying"))
    } else {
        Ok(())
    }
}

async fn fetch_with_retry(
    url: &str,
    expected_sha256: &str,
    max_bytes: usize,
    allowed_origins: &[&str],
) -> Result<Vec<u8>, JsValue> {
    let mut attempt: u32 = 0;
    loop {
        match try_fetch(url, expected_sha256, max_bytes, allowed_origins).await {
            Ok(bytes) => return Ok(bytes),
            Err(err) if attempt < MAX_FETCH_RETRIES => {
                attempt += 1;
                let delay_ms = 500 * (2u32.pow(attempt));
                sleep_ms(delay_ms as i32).await?;
            }
            Err(err) => return Err(err),
        }
    }
}

async fn try_fetch(
    url: &str,
    expected_sha256: &str,
    max_bytes: usize,
    allowed_origins: &[&str],
) -> Result<Vec<u8>, JsValue> {
    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);

    let request = Request::new_with_str_and_init(url, &opts)?;
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("No window object"))?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;

    if !resp.ok() {
        return Err(JsValue::from_str("Failed to fetch resource"));
    }

    let final_url = resp.url();
    if !final_url.is_empty() {
        validate_final_fetch_url(&final_url, url, allowed_origins)?;
    }

    if let Ok(Some(content_type)) = resp.headers().get("content-type") {
        let content_type = content_type.to_ascii_lowercase();
        if content_type.contains("text/html") {
            return Err(JsValue::from_str("Invalid content type"));
        }
    }

    let buffer = JsFuture::from(resp.array_buffer()?).await?;
    let bytes = Uint8Array::new(&buffer).to_vec();
    if bytes.len() > max_bytes {
        return Err(JsValue::from_str("Resource exceeds size limit"));
    }
    if !verify_sha256(&bytes, expected_sha256) {
        return Err(JsValue::from_str("Integrity check failed"));
    }

    Ok(bytes)
}

async fn sleep_ms(ms: i32) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("No window object"))?;
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let resolve = resolve.clone();
        let callback = Closure::wrap(Box::new(move || {
            let _ = resolve.call0(&JsValue::NULL);
        }) as Box<dyn FnMut()>);

        if window
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                callback.as_ref().unchecked_ref(),
                ms,
            )
            .is_err()
        {
            let _ = reject.call1(&JsValue::NULL, &JsValue::from_str("Failed to schedule retry"));
        }
        callback.forget();
    });
    JsFuture::from(promise).await.map(|_| ())
}

fn verify_sha256(bytes: &[u8], expected_hex: &str) -> bool {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let computed = hex_encode(&digest);
    computed.eq_ignore_ascii_case(expected_hex)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

async fn open_db() -> Result<IdbDatabase, JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("No window object"))?;
    let indexed_db = window
        .indexed_db()?
        .ok_or_else(|| JsValue::from_str("IndexedDB unavailable"))?;
    let request = indexed_db.open_with_u32(DB_NAME, DB_VERSION)?;
    let store_name = STORE_NAME.to_string();

    let upgrade = Closure::wrap(Box::new(move |event: web_sys::IdbVersionChangeEvent| {
        let target = match event.target() {
            Some(target) => target,
            None => return,
        };
        let request: IdbOpenDbRequest = match target.dyn_into() {
            Ok(request) => request,
            Err(_) => return,
        };
        let db: IdbDatabase = match request.result().and_then(|value| value.dyn_into()) {
            Ok(db) => db,
            Err(_) => return,
        };
        if !db.object_store_names().contains(&store_name) {
            let _ = db.create_object_store(&store_name);
        }
    }) as Box<dyn FnMut(web_sys::IdbVersionChangeEvent)>);

    request.set_onupgradeneeded(Some(upgrade.as_ref().unchecked_ref()));
    upgrade.forget();

    let request: IdbRequest = request.unchecked_into();
    let result = idb_request(request).await?;
    result.dyn_into()
}

async fn load_cached_bytes(key: &str) -> Result<Option<Vec<u8>>, JsValue> {
    let db = open_db().await?;
    let tx = db.transaction_with_str_and_mode(STORE_NAME, IdbTransactionMode::Readonly)?;
    let store = tx.object_store(STORE_NAME)?;
    let request = store.get(&JsValue::from_str(key))?;
    let value = idb_request(request).await?;
    if value.is_undefined() || value.is_null() {
        return Ok(None);
    }
    Ok(Some(Uint8Array::new(&value).to_vec()))
}

async fn store_cached_bytes(key: &str, bytes: &[u8]) -> Result<(), JsValue> {
    let db = open_db().await?;
    let tx = db.transaction_with_str_and_mode(STORE_NAME, IdbTransactionMode::Readwrite)?;
    let store = tx.object_store(STORE_NAME)?;
    let value = Uint8Array::from(bytes);
    let request = store.put_with_key(&value.into(), &JsValue::from_str(key))?;
    let _ = idb_request(request).await?;
    Ok(())
}

async fn delete_cached_bytes(key: &str) -> Result<(), JsValue> {
    let db = open_db().await?;
    let tx = db.transaction_with_str_and_mode(STORE_NAME, IdbTransactionMode::Readwrite)?;
    let store = tx.object_store(STORE_NAME)?;
    let request = store.delete(&JsValue::from_str(key))?;
    let _ = idb_request(request).await?;
    Ok(())
}

async fn idb_request(request: IdbRequest) -> Result<JsValue, JsValue> {
    let request_for_success = request.clone();

    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let request_for_success = request_for_success.clone();
        let success = Closure::wrap(Box::new(move |_event: Event| {
            let result = request_for_success.result().unwrap_or(JsValue::UNDEFINED);
            let _ = resolve.call1(&JsValue::NULL, &result);
        }) as Box<dyn FnMut(Event)>);

        let error = Closure::wrap(Box::new(move |_event: Event| {
            let err = JsValue::from_str("IndexedDB request failed");
            let _ = reject.call1(&JsValue::NULL, &err);
        }) as Box<dyn FnMut(Event)>);

        request.set_onsuccess(Some(success.as_ref().unchecked_ref()));
        request.set_onerror(Some(error.as_ref().unchecked_ref()));
        success.forget();
        error.forget();
    });

    JsFuture::from(promise).await
}

fn js_error(context: &str, detail: impl std::fmt::Debug) -> JsValue {
    web_sys::console::error_1(&JsValue::from_str(&format!("{context}: {detail:?}")));
    JsValue::from_str(context)
}
