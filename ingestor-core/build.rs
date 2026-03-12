use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

const MODEL_DIR: &str = "models";
const MODEL_F32_FILE: &str = "models/mdbr-leaf-ir-f32.bin";
const MODEL_Q8_FILE: &str = "models/mdbr-leaf-ir-q8.bin";
const LEGACY_MODEL_Q8_FILE: &str = "models/mdbr-leaf-ir.bin";
const TOKENIZER_FILE: &str = "models/tokenizer.json";
const BASE_SAFETENSORS_FILE: &str = "models/mdbr-leaf-ir.safetensors";
const DENSE_SAFETENSORS_FILE: &str = "models/mdbr-leaf-ir-dense.safetensors";
const MODEL_SRC_FILE: &str = "src/bert.rs";
const FALLBACK_MODEL_DIR: &str = "../ingestor-wasm/models";
const FALLBACK_MODEL_Q8_FILE: &str = "../ingestor-wasm/models/mdbr-leaf-ir.bin";
const FALLBACK_BASE_SAFETENSORS_FILE: &str = "../ingestor-wasm/models/mdbr-leaf-ir.safetensors";
const FALLBACK_DENSE_SAFETENSORS_FILE: &str = "../ingestor-wasm/models/mdbr-leaf-ir-dense.safetensors";
const FALLBACK_TOKENIZER_FILE: &str = "../ingestor-wasm/models/tokenizer.json";

#[allow(dead_code)]
mod model {
    include!("src/bert.rs");
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", MODEL_SRC_FILE);
    println!("cargo:rerun-if-changed={}", BASE_SAFETENSORS_FILE);
    println!("cargo:rerun-if-changed={}", DENSE_SAFETENSORS_FILE);
    println!("cargo:rerun-if-changed={}", LEGACY_MODEL_Q8_FILE);
    println!("cargo:rerun-if-changed={}", FALLBACK_MODEL_Q8_FILE);
    println!("cargo:rerun-if-changed={}", FALLBACK_BASE_SAFETENSORS_FILE);
    println!("cargo:rerun-if-changed={}", FALLBACK_DENSE_SAFETENSORS_FILE);
    println!("cargo:rerun-if-changed={}", TOKENIZER_FILE);
    println!("cargo:rerun-if-changed={}", FALLBACK_TOKENIZER_FILE);

    // Only enforce model/tokenizer presence when an embedding backend is enabled.
    // Build scripts run for all configurations, so unconditional panics break
    // builds that don't use semantic embeddings (e.g. --no-default-features).
    let embeddings_enabled = env::var_os("CARGO_FEATURE_NDARRAY_BACKEND").is_some()
        || env::var_os("CARGO_FEATURE_WGPU_BACKEND").is_some();

    if embeddings_enabled {
        fs::create_dir_all(MODEL_DIR).expect("Failed to create models directory");

        let tokenizer_source =
            resolve_source_path(&[TOKENIZER_FILE, FALLBACK_TOKENIZER_FILE]);

        if tokenizer_source != Path::new(TOKENIZER_FILE) {
            fs::copy(&tokenizer_source, TOKENIZER_FILE).unwrap_or_else(|err| {
                panic!(
                    "Failed to copy tokenizer from {} to {}: {err}",
                    tokenizer_source.display(),
                    TOKENIZER_FILE
                )
            });
        }

        if should_rebuild_f32_artifact(Path::new(MODEL_F32_FILE), Path::new(MODEL_SRC_FILE)) {
            let base_safetensors = resolve_source_path(&[
                BASE_SAFETENSORS_FILE,
                FALLBACK_BASE_SAFETENSORS_FILE,
            ]);
            let dense_safetensors = resolve_source_path(&[
                DENSE_SAFETENSORS_FILE,
                FALLBACK_DENSE_SAFETENSORS_FILE,
            ]);
            build_native_f32_artifact(
                &base_safetensors,
                &dense_safetensors,
                Path::new(MODEL_F32_FILE),
            )
            .unwrap_or_else(|err| panic!("Failed to build native f32 embedding artifact: {err}"));
        }

        let q8_source = resolve_source_path(&[LEGACY_MODEL_Q8_FILE, FALLBACK_MODEL_Q8_FILE]);
        if should_copy_file(&q8_source, Path::new(MODEL_Q8_FILE)) {
            fs::copy(&q8_source, MODEL_Q8_FILE).unwrap_or_else(|err| {
                panic!(
                    "Failed to copy q8 model from {} to {}: {err}",
                    q8_source.display(),
                    MODEL_Q8_FILE
                )
            });
        }

        if !Path::new(MODEL_F32_FILE).exists() {
            panic!(
                "Missing model file: {}\n\
                 Expected build.rs to create native embedding artifacts.",
                MODEL_F32_FILE
            );
        }

        if !Path::new(MODEL_Q8_FILE).exists() {
            panic!(
                "Missing model file: {}\n\
                 Expected build.rs to create native embedding artifacts.",
                MODEL_Q8_FILE
            );
        }

        if !Path::new(TOKENIZER_FILE).exists() {
            panic!(
                "Missing tokenizer file: {}\n\
                 Copy from ingestor-wasm/models/ or download from release artifacts.",
                TOKENIZER_FILE
            );
        }

        emit_model_metadata("F32", "f32", Path::new(MODEL_F32_FILE));
        emit_model_metadata("Q8", "q8", Path::new(MODEL_Q8_FILE));

        println!(
            "cargo:warning=Using native models: {} and {}",
            MODEL_F32_FILE, MODEL_Q8_FILE
        );
    }
}

fn resolve_source_path(candidates: &[&str]) -> PathBuf {
    candidates
        .iter()
        .map(PathBuf::from)
        .find(|path| path.exists())
        .unwrap_or_else(|| {
            panic!(
                "Missing required source file. Looked in: {}",
                candidates.join(", ")
            )
        })
}

fn should_copy_file(source_path: &Path, target_path: &Path) -> bool {
    if !target_path.exists() {
        return true;
    }

    match (sha256_hex(source_path), sha256_hex(target_path)) {
        (Ok(source_hash), Ok(target_hash)) => source_hash != target_hash,
        _ => true,
    }
}

fn should_rebuild_f32_artifact(
    f32_path: &Path,
    model_src_path: &Path,
) -> bool {
    if !f32_path.exists() {
        return true;
    }

    let src_meta = match fs::metadata(model_src_path) {
        Ok(meta) => meta,
        Err(_) => return true,
    };
    let f32_meta = match fs::metadata(f32_path) {
        Ok(meta) => meta,
        Err(_) => return true,
    };

    match (
        src_meta.modified(),
        f32_meta.modified(),
    ) {
        (Ok(src_time), Ok(f32_time)) => src_time > f32_time,
        _ => true,
    }
}

fn build_native_f32_artifact(
    base_safetensors_path: &Path,
    dense_safetensors_path: &Path,
    f32_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "cargo:warning=Building native mdbr-leaf-ir f32 artifact from {}",
        fallback_source_dir(base_safetensors_path)
    );

    let model = load_model(base_safetensors_path, dense_safetensors_path)?;
    write_f32_model(model, f32_path)?;

    Ok(())
}

fn fallback_source_dir(path: &Path) -> String {
    path.parent()
        .unwrap_or_else(|| Path::new(FALLBACK_MODEL_DIR))
        .display()
        .to_string()
}

fn load_model(
    base_safetensors_path: &Path,
    dense_safetensors_path: &Path,
) -> Result<model::BertModel<burn_ndarray::NdArray<f32>>, Box<dyn std::error::Error>> {
    use burn_ndarray::{NdArray, NdArrayDevice};
    use burn_store::{ModuleSnapshot, PyTorchToBurnAdapter, SafetensorsStore};

    type Backend = NdArray<f32>;
    let device = NdArrayDevice::default();

    let mut store = SafetensorsStore::from_file(base_safetensors_path.to_path_buf())
        .with_from_adapter(PyTorchToBurnAdapter)
        .with_key_remapping("^bert\\.(.*)$", "$1")
        .with_key_remapping("^model\\.(.*)$", "$1")
        .with_key_remapping("attention\\.self\\.(.*)$", "attention.self_attn.$1")
        .with_key_remapping("^LayerNorm\\.(.*)$", "layer_norm.$1")
        .with_key_remapping("\\.LayerNorm\\.", ".layer_norm.")
        .allow_partial(true);

    let mut model = model::BertModel::<Backend>::new(&device);
    model.load_from(&mut store)?;

    let mut dense_store = SafetensorsStore::from_file(dense_safetensors_path.to_path_buf())
        .with_from_adapter(PyTorchToBurnAdapter)
        .with_key_remapping("^linear\\.(.*)$", "dense.$1")
        .allow_partial(true);
    model.load_from(&mut dense_store)?;

    Ok(model)
}

fn write_f32_model(
    model: model::BertModel<burn_ndarray::NdArray<f32>>,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    use burn::module::Module;
    use burn::record::{BinFileRecorder, FullPrecisionSettings, Recorder};

    BinFileRecorder::<FullPrecisionSettings>::default().record(model.into_record(), path.to_path_buf())?;
    Ok(())
}

fn emit_model_metadata(env_suffix: &str, model_kind: &str, path: &Path) {
    let sha256 = sha256_hex(path).unwrap_or_else(|err| {
        panic!("Failed to hash {}: {err}", path.display());
    });
    let short_hash = &sha256[..12];
    println!(
        "cargo:rustc-env=LLMX_MODEL_ID_{}=mdbr-leaf-ir-{}-{}",
        env_suffix, model_kind, short_hash
    );
    println!("cargo:rustc-env=LLMX_MODEL_SHA256_{}={}", env_suffix, sha256);
}

fn sha256_hex(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    use sha2::{Digest, Sha256};

    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];

    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}
