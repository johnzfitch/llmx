use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

const BASE_SAFETENSORS_URL: &str =
    "https://huggingface.co/MongoDB/mdbr-leaf-ir/resolve/main/model.safetensors";
const DENSE_SAFETENSORS_URL: &str =
    "https://huggingface.co/MongoDB/mdbr-leaf-ir/resolve/main/2_Dense/model.safetensors";
const MODEL_DIR: &str = "models";
const BASE_SAFETENSORS_FILE: &str = "models/mdbr-leaf-ir.safetensors";
const DENSE_SAFETENSORS_FILE: &str = "models/mdbr-leaf-ir-dense.safetensors";
const MODEL_BIN_FILE: &str = "models/mdbr-leaf-ir.bin";
const MODEL_SRC_FILE: &str = "src/bert.rs";
const MAX_MODEL_BYTES: u64 = 100 * 1024 * 1024;

// Model format version - must match model_loader.rs expectations
// Format: BinFileRecorder<FullPrecisionSettings> + INT8 Q8S quantization
#[allow(dead_code)]
const MODEL_FORMAT_VERSION: u8 = 1;

#[allow(dead_code)]
mod model {
    include!("src/bert.rs");
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", MODEL_SRC_FILE);
    println!("cargo:rerun-if-changed={}", BASE_SAFETENSORS_FILE);
    println!("cargo:rerun-if-changed={}", DENSE_SAFETENSORS_FILE);
    println!("cargo:rerun-if-env-changed=LLMX_EMBEDDING_MODEL_URL");

    if !Path::new(MODEL_DIR).exists() {
        fs::create_dir(MODEL_DIR).expect("Failed to create models directory");
    }

    if !Path::new(BASE_SAFETENSORS_FILE).exists() {
        println!("cargo:warning=Downloading mdbr-leaf-ir base safetensors...");
        download_file(BASE_SAFETENSORS_URL, BASE_SAFETENSORS_FILE)
            .expect("Failed to download base safetensors model");
        println!("cargo:warning=Base safetensors model downloaded");
    } else {
        println!("cargo:warning=Using cached base safetensors model");
    }

    if !Path::new(DENSE_SAFETENSORS_FILE).exists() {
        println!("cargo:warning=Downloading mdbr-leaf-ir dense safetensors...");
        download_file(DENSE_SAFETENSORS_URL, DENSE_SAFETENSORS_FILE)
            .expect("Failed to download dense safetensors model");
        println!("cargo:warning=Dense safetensors model downloaded");
    } else {
        println!("cargo:warning=Using cached dense safetensors model");
    }

    if should_convert(BASE_SAFETENSORS_FILE, DENSE_SAFETENSORS_FILE, MODEL_BIN_FILE, MODEL_SRC_FILE) {
        println!("cargo:warning=Converting safetensors to Burn binary (INT8 Q8S)...");
        convert_safetensors_to_bin(BASE_SAFETENSORS_FILE, DENSE_SAFETENSORS_FILE, MODEL_BIN_FILE)
            .expect("Failed to convert safetensors to burn binary");
        println!("cargo:warning=Burn binary written to models/mdbr-leaf-ir.bin");
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

fn should_convert(
    base_safetensors_path: &str,
    dense_safetensors_path: &str,
    bin_path: &str,
    model_src_path: &str,
) -> bool {
    if !Path::new(bin_path).exists() {
        return true;
    }

    let base_safetensors_meta = match fs::metadata(base_safetensors_path) {
        Ok(meta) => meta,
        Err(_) => return true,
    };
    let dense_safetensors_meta = match fs::metadata(dense_safetensors_path) {
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
        base_safetensors_meta.modified(),
        dense_safetensors_meta.modified(),
        model_src_meta.modified(),
        bin_meta.modified(),
    ) {
        (Ok(base_time), Ok(dense_time), Ok(src_time), Ok(bin_time)) => {
            base_time > bin_time || dense_time > bin_time || src_time > bin_time
        }
        _ => true,
    }
}

fn convert_safetensors_to_bin(
    base_safetensors_path: &str,
    dense_safetensors_path: &str,
    bin_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use burn::module::{Module, Quantizer};
    use burn::record::{BinFileRecorder, FullPrecisionSettings, Recorder};
    use burn::tensor::backend::Backend as BurnBackend;
    use burn::tensor::quantization::{
        Calibration, QTensorPrimitive, QuantLevel, QuantParam, QuantValue,
    };
    use burn_ndarray::{NdArray, NdArrayDevice};
    use burn_store::{ModuleSnapshot, PyTorchToBurnAdapter, SafetensorsStore};

    type Backend = NdArray<f32>;
    let device = NdArrayDevice::default();

    // Map HuggingFace PyTorch weight keys to Burn module structure
    // These patterns are critical - if they don't match, model loading will fail
    let mut store = SafetensorsStore::from_file(PathBuf::from(base_safetensors_path))
        .with_from_adapter(PyTorchToBurnAdapter)
        .with_key_remapping("^bert\\.(.*)$", "$1")                              // Remove bert. prefix
        .with_key_remapping("^model\\.(.*)$", "$1")                             // Remove model. prefix
        .with_key_remapping("attention\\.self\\.(.*)$", "attention.self_attn.$1")  // Rename self -> self_attn
        .with_key_remapping("^LayerNorm\\.(.*)$", "layer_norm.$1")              // Pascal -> snake case
        .with_key_remapping("\\.LayerNorm\\.", ".layer_norm.")                  // Pascal -> snake case (nested)
        .allow_partial(true);

    println!("cargo:warning=Loading safetensors with key remapping for Burn module structure");

    let mut model = model::BertModel::<Backend>::new(&device);
    model.load_from(&mut store)?;

    let mut dense_store = SafetensorsStore::from_file(PathBuf::from(dense_safetensors_path))
        .with_from_adapter(PyTorchToBurnAdapter)
        .with_key_remapping("^linear\\.(.*)$", "dense.$1")
        .allow_partial(true);
    model.load_from(&mut dense_store)?;

    println!("cargo:warning=Safetensors loaded successfully, all keys matched");

    // Apply INT8 Q8S quantization (tensor-level, signed 8-bit)
    let scheme = <<Backend as BurnBackend>::QuantizedTensorPrimitive as QTensorPrimitive>::default_scheme()
        .with_value(QuantValue::Q8S)
        .with_level(QuantLevel::Tensor)
        .with_param(QuantParam::F32);
    let mut quantizer = Quantizer {
        calibration: Calibration::MinMax,
        scheme,
    };
    let quantized_model = model.quantize_weights(&mut quantizer);

    // CRITICAL: Must match model_loader.rs recorder settings
    // Uses FullPrecisionSettings - quantization is transparent during serialization
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
