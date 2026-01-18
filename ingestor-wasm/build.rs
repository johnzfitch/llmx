use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

const SAFETENSORS_URL: &str =
    "https://huggingface.co/Snowflake/snowflake-arctic-embed-s/resolve/main/model.safetensors";
const MODEL_DIR: &str = "models";
const SAFETENSORS_FILE: &str = "models/arctic-embed-s.safetensors";
const MODEL_BIN_FILE: &str = "models/arctic-embed-s.bin";
const MODEL_SRC_FILE: &str = "src/bert.rs";
const MAX_MODEL_BYTES: u64 = 100 * 1024 * 1024;

#[allow(dead_code)]
mod model {
    include!("src/bert.rs");
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", MODEL_SRC_FILE);
    println!("cargo:rerun-if-changed={}", SAFETENSORS_FILE);
    println!("cargo:rerun-if-env-changed=LLMX_EMBEDDING_MODEL_URL");

    if !Path::new(MODEL_DIR).exists() {
        fs::create_dir(MODEL_DIR).expect("Failed to create models directory");
    }

    if !Path::new(SAFETENSORS_FILE).exists() {
        println!("cargo:warning=Downloading arctic-embed-s safetensors (~33MB)...");
        download_file(SAFETENSORS_URL, SAFETENSORS_FILE)
            .expect("Failed to download safetensors model");
        println!("cargo:warning=Safetensors model downloaded");
    } else {
        println!("cargo:warning=Using cached safetensors model");
    }

    if should_convert(SAFETENSORS_FILE, MODEL_BIN_FILE, MODEL_SRC_FILE) {
        println!("cargo:warning=Converting safetensors to Burn binary (INT8 Q8S)...");
        convert_safetensors_to_bin(SAFETENSORS_FILE, MODEL_BIN_FILE)
            .expect("Failed to convert safetensors to burn binary");
        println!("cargo:warning=Burn binary written to models/arctic-embed-s.bin");
    } else {
        println!("cargo:warning=Using cached Burn binary");
    }

    if let Ok(model_url) = env::var("LLMX_EMBEDDING_MODEL_URL") {
        if !model_url.is_empty() {
            if model_url.contains('?') {
                println!(
                    "cargo:warning=LLMX_EMBEDDING_MODEL_URL contains query parameters; ensure it is public and non-sensitive"
                );
            }
            println!("cargo:rustc-env=LLMX_EMBEDDING_MODEL_URL={}", model_url);
        }
    } else {
        let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
        if target_arch == "wasm32" {
            println!("cargo:warning=LLMX_EMBEDDING_MODEL_URL not set; runtime model fetch will fail");
        }
    }
}

fn should_convert(safetensors_path: &str, bin_path: &str, model_src_path: &str) -> bool {
    if !Path::new(bin_path).exists() {
        return true;
    }

    let safetensors_meta = match fs::metadata(safetensors_path) {
        Ok(meta) => meta,
        Err(_) => return true,
    };
    let model_src_meta = match fs::metadata(model_src_path) {
        Ok(meta) => meta,
        Err(_) => return true,
    };
    let bin_meta = match fs::metadata(bin_path) {
        Ok(meta) => meta,
        Err(_) => return true,
    };

    match (
        safetensors_meta.modified(),
        model_src_meta.modified(),
        bin_meta.modified(),
    ) {
        (Ok(safe_time), Ok(src_time), Ok(bin_time)) => safe_time > bin_time || src_time > bin_time,
        _ => true,
    }
}

fn convert_safetensors_to_bin(
    safetensors_path: &str,
    bin_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use burn::module::{Module, Quantizer};
    use burn::record::{BinFileRecorder, FullPrecisionSettings, Recorder};
    use burn::tensor::backend::Backend as BurnBackend;
    use burn::tensor::quantization::{
        Calibration, QTensorPrimitive, QuantLevel, QuantParam, QuantValue,
    };
    use burn_import::safetensors::{AdapterType, LoadArgs, SafetensorsFileRecorder};
    use burn_ndarray::{NdArray, NdArrayDevice};

    type Backend = NdArray<f32>;
    let device = NdArrayDevice::default();

    let load_args = LoadArgs::new(PathBuf::from(safetensors_path))
        .with_adapter_type(AdapterType::PyTorch)
        .with_key_remap("^bert\\.(.*)$", "$1")
        .with_key_remap("^model\\.(.*)$", "$1")
        .with_key_remap("attention\\.self\\.(.*)$", "attention.self_attn.$1")
        .with_key_remap("^LayerNorm\\.(.*)$", "layer_norm.$1")
        .with_key_remap("\\.LayerNorm\\.", ".layer_norm.");

    let record: <model::BertModel<Backend> as Module<Backend>>::Record =
        SafetensorsFileRecorder::<FullPrecisionSettings>::default()
            .load(load_args, &device)?;

    let model = model::BertModel::<Backend>::new(&device).load_record(record);
    let scheme = <<Backend as BurnBackend>::QuantizedTensorPrimitive as QTensorPrimitive>::default_scheme()
        .with_value(QuantValue::Q8S)
        .with_level(QuantLevel::Tensor)
        .with_param(QuantParam::F32);
    let mut quantizer = Quantizer {
        calibration: Calibration::MinMax,
        scheme,
    };
    let quantized_model = model.quantize_weights(&mut quantizer);

    BinFileRecorder::<FullPrecisionSettings>::default()
        .record(quantized_model.into_record(), PathBuf::from(bin_path))?;

    Ok(())
}

fn download_file(url: &str, dest: &str) -> Result<(), Box<dyn std::error::Error>> {
    let response = reqwest::blocking::get(url)?;

    if !response.status().is_success() {
        return Err(format!("Failed to download: HTTP {}", response.status()).into());
    }

    if let Some(len) = response.content_length() {
        if len > MAX_MODEL_BYTES {
            return Err(format!(
                "Model too large: {} MB (max {} MB)",
                len / 1024 / 1024,
                MAX_MODEL_BYTES / 1024 / 1024
            )
            .into());
        }
    }

    let mut file = fs::File::create(dest)?;
    let mut limited = response.take(MAX_MODEL_BYTES + 1);
    let written = std::io::copy(&mut limited, &mut file)?;
    if written > MAX_MODEL_BYTES {
        let _ = fs::remove_file(dest);
        return Err("Model download exceeded size limit".into());
    }

    Ok(())
}
