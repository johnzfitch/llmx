use std::env;
use std::path::Path;

const MODEL_BIN_FILE: &str = "models/arctic-embed-s.bin";
const TOKENIZER_FILE: &str = "models/tokenizer.json";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    // Only enforce model/tokenizer presence when an embedding backend is enabled.
    // Build scripts run for all configurations, so unconditional panics break
    // builds that don't use semantic embeddings (e.g. --no-default-features).
    let embeddings_enabled = env::var_os("CARGO_FEATURE_NDARRAY_BACKEND").is_some()
        || env::var_os("CARGO_FEATURE_WGPU_BACKEND").is_some();

    if embeddings_enabled {
        println!("cargo:rerun-if-changed={}", MODEL_BIN_FILE);
        println!("cargo:rerun-if-changed={}", TOKENIZER_FILE);

        if !Path::new(MODEL_BIN_FILE).exists() {
            panic!(
                "Missing model file: {}\n\
                 Copy from ingestor-wasm/models/ or download from release artifacts.",
                MODEL_BIN_FILE
            );
        }

        if !Path::new(TOKENIZER_FILE).exists() {
            panic!(
                "Missing tokenizer file: {}\n\
                 Copy from ingestor-wasm/models/ or download from release artifacts.",
                TOKENIZER_FILE
            );
        }

        println!("cargo:warning=Using pre-built model: {}", MODEL_BIN_FILE);
    }
}
