/// Phase 6: Build script to fetch and convert ONNX model to Burn format
///
/// This script:
/// 1. Downloads bge-small-en-v1.5 ONNX model from HuggingFace (if not cached)
/// 2. Converts ONNX to Burn Rust code at compile time
/// 3. Generates model code in src/model/ directory
///
/// Model is fetched once and cached locally in models/ directory (gitignored).

use std::fs;
use std::path::Path;

const MODEL_URL: &str = "https://huggingface.co/BAAI/bge-small-en-v1.5/resolve/main/onnx/model.onnx";
const MODEL_DIR: &str = "models";
const MODEL_FILE: &str = "models/bge-small-en-v1.5.onnx";
const MODEL_FILE_OPSET13: &str = "models/bge-small-en-v1.5-opset13.onnx";
const TOKENIZER_URL: &str = "https://huggingface.co/BAAI/bge-small-en-v1.5/resolve/main/tokenizer.json";
const TOKENIZER_FILE: &str = "models/tokenizer.json";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", MODEL_FILE);

    // Create models directory if it doesn't exist
    if !Path::new(MODEL_DIR).exists() {
        fs::create_dir(MODEL_DIR).expect("Failed to create models directory");
    }

    // Download ONNX model if not cached
    if !Path::new(MODEL_FILE).exists() {
        println!("cargo:warning=Downloading bge-small-en-v1.5 ONNX model (~33MB)...");
        download_file(MODEL_URL, MODEL_FILE)
            .expect("Failed to download ONNX model");
        println!("cargo:warning=Model downloaded successfully");
    } else {
        println!("cargo:warning=Using cached ONNX model");
    }

    // Download tokenizer if not cached
    if !Path::new(TOKENIZER_FILE).exists() {
        println!("cargo:warning=Downloading tokenizer...");
        download_file(TOKENIZER_URL, TOKENIZER_FILE)
            .expect("Failed to download tokenizer");
        println!("cargo:warning=Tokenizer downloaded successfully");
    } else {
        println!("cargo:warning=Using cached tokenizer");
    }

    // Convert model to opset 13 if needed
    if !Path::new(MODEL_FILE_OPSET13).exists() {
        if Path::new(MODEL_FILE).exists() {
            println!("cargo:warning=Converting ONNX model from opset 11 to opset 13...");
            println!("cargo:warning=Run: python -c \"import onnx; from onnx import version_converter; m = onnx.load('{}'); c = version_converter.convert_version(m, 13); onnx.save(c, '{}')\"", MODEL_FILE, MODEL_FILE_OPSET13);
            panic!("Please convert ONNX model to opset 13 (see warning above)");
        } else {
            panic!("Model file {} not found. Please download it first.", MODEL_FILE);
        }
    }

    // Convert ONNX to Burn model
    // Note: This generates Rust code at compile time using the opset 13 model
    println!("cargo:warning=Converting ONNX (opset 13) to Burn model (this may take a minute)...");

    use burn_import::onnx::ModelGen;

    ModelGen::new()
        .input(MODEL_FILE_OPSET13)
        .out_dir("src/model/")
        .run_from_script();

    println!("cargo:warning=Model conversion complete");
}

fn download_file(url: &str, dest: &str) -> Result<(), Box<dyn std::error::Error>> {
    let response = reqwest::blocking::get(url)?;

    if !response.status().is_success() {
        return Err(format!("Failed to download: HTTP {}", response.status()).into());
    }

    let bytes = response.bytes()?;
    fs::write(dest, bytes)?;

    Ok(())
}
