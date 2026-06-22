use std::env;
use std::fs;
use std::io::Read;
use std::path::Path;

const MODEL_F32_FILE: &str = "models/mdbr-leaf-ir-f32.bin";
const MODEL_Q8_FILE: &str = "models/mdbr-leaf-ir-q8.bin";
const TOKENIZER_FILE: &str = "models/tokenizer.json";

// The native embedding artifacts (f32 + q8 Burn binaries) and tokenizer are
// committed under `models/`. build.rs verifies their presence and derives the
// model id/sha256 used by the runtime integrity check. There is no download or
// safetensors conversion step: the committed bins are the source of truth.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", MODEL_F32_FILE);
    println!("cargo:rerun-if-changed={}", MODEL_Q8_FILE);
    println!("cargo:rerun-if-changed={}", TOKENIZER_FILE);

    // Only enforce model/tokenizer presence when an embedding backend is enabled.
    // Build scripts run for all configurations, so unconditional panics break
    // builds that don't use semantic embeddings (e.g. --no-default-features).
    let embeddings_enabled = env::var_os("CARGO_FEATURE_NDARRAY_BACKEND").is_some()
        || env::var_os("CARGO_FEATURE_WGPU_BACKEND").is_some();

    if !embeddings_enabled {
        return;
    }

    for required in [MODEL_F32_FILE, MODEL_Q8_FILE, TOKENIZER_FILE] {
        if !Path::new(required).exists() {
            panic!(
                "Missing committed model artifact: {required}\n\
                 The native embedding model is committed under ingestor-core/models/. \
                 Restore it (e.g. `git checkout -- {required}`) or build without an \
                 embedding backend (--no-default-features)."
            );
        }
    }

    emit_model_metadata("F32", "f32", Path::new(MODEL_F32_FILE));
    emit_model_metadata("Q8", "q8", Path::new(MODEL_Q8_FILE));
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
