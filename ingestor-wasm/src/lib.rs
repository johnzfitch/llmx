#![recursion_limit = "256"]

use ingestor_core::{
    export_catalog_llm_md, export_chunks, export_chunks_compact, export_llm, export_llm_pointer,
    export_manifest_json, export_manifest_llm_tsv, export_manifest_min_json, ingest_files, search, update_index,
    update_index_selective, FileInput, IngestOptions,
    IndexFile, SearchFilters,
};
use serde_wasm_bindgen::{from_value, to_value};
use std::collections::BTreeMap;
use std::io::{Cursor, Write};
use wasm_bindgen::prelude::*;
use zip::write::FileOptions;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn wasm_start() {
    std::panic::set_hook(Box::new(|info| {
        web_sys::console::error_1(&JsValue::from_str(&info.to_string()));
    }));
}

// Phase 6: Burn-based embeddings for WebGPU
mod bert;
mod model_loader;
mod embeddings_burn;
#[cfg(test)]
mod quantization_test;
#[cfg(test)]
mod backend_tests;
pub use embeddings_burn::Embedder;

#[wasm_bindgen]
pub struct Ingestor {
    index: IndexFile,
    assets: BTreeMap<String, Vec<u8>>,
}

#[wasm_bindgen]
impl Ingestor {
    #[wasm_bindgen(js_name = ingest)]
    pub fn ingest(files: JsValue, options: JsValue) -> Result<Ingestor, JsValue> {
        let files: Vec<FileInput> = from_value(files).map_err(to_js_error)?;
        let options = parse_options(options)?;
        let assets = collect_assets(&files);
        let index = ingest_files(files, options);
        Ok(Ingestor { index, assets })
    }

    #[wasm_bindgen(js_name = fromIndexJson)]
    pub fn from_index_json(json: String) -> Result<Ingestor, JsValue> {
        let index: IndexFile = serde_json::from_str(&json).map_err(to_js_error)?;
        Ok(Ingestor {
            index,
            assets: BTreeMap::new(),
        })
    }

    #[wasm_bindgen(js_name = update)]
    pub fn update(&mut self, files: JsValue, options: JsValue) -> Result<(), JsValue> {
        let files: Vec<FileInput> = from_value(files).map_err(to_js_error)?;
        let options = parse_options(options)?;
        merge_assets(&mut self.assets, &files);
        self.index = update_index(self.index.clone(), files, options);
        Ok(())
    }

    #[wasm_bindgen(js_name = updateSelective)]
    pub fn update_selective(&mut self, files: JsValue, keep_paths: JsValue, options: JsValue) -> Result<(), JsValue> {
        let files: Vec<FileInput> = from_value(files).map_err(to_js_error)?;
        let keep_paths: Vec<String> = from_value(keep_paths).map_err(to_js_error)?;
        let options = parse_options(options)?;
        merge_assets(&mut self.assets, &files);
        self.index = update_index_selective(self.index.clone(), files, keep_paths, options);
        Ok(())
    }

    #[wasm_bindgen(js_name = search)]
    pub fn search(&self, query: String, filters: JsValue, limit: usize) -> Result<JsValue, JsValue> {
        let filters: SearchFilters = if filters.is_null() || filters.is_undefined() {
            SearchFilters::default()
        } else {
            from_value(filters).map_err(to_js_error)?
        };
        let results = search(&self.index, &query, filters, limit);
        to_value(&results).map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = getChunk)]
    pub fn get_chunk(&self, chunk_id: String) -> Result<JsValue, JsValue> {
        let chunk = self.index.chunks.iter().find(|c| c.id == chunk_id);
        to_value(&chunk).map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = exportLlm)]
    pub fn export_llm(&self) -> String {
        export_llm(&self.index)
    }

    #[wasm_bindgen(js_name = exportLlmPointer)]
    pub fn export_llm_pointer(&self) -> String {
        export_llm_pointer(&self.index)
    }

    #[wasm_bindgen(js_name = exportZip)]
    pub fn export_zip(&self) -> Vec<u8> {
        export_zip_with_assets(&self.index, &self.assets)
    }

    #[wasm_bindgen(js_name = exportZipCompact)]
    pub fn export_zip_compact(&self) -> Vec<u8> {
        export_zip_compact_with_assets(&self.index, &self.assets)
    }

    #[wasm_bindgen(js_name = exportManifestMinJson)]
    pub fn export_manifest_min_json(&self) -> String {
        export_manifest_min_json(&self.index)
    }

    #[wasm_bindgen(js_name = exportManifestLlmTsv)]
    pub fn export_manifest_llm_tsv(&self) -> String {
        export_manifest_llm_tsv(&self.index)
    }

    #[wasm_bindgen(js_name = exportCatalogLlmMd)]
    pub fn export_catalog_llm_md(&self) -> String {
        export_catalog_llm_md(&self.index)
    }

    #[wasm_bindgen(js_name = exportIndexJson)]
    pub fn export_index_json(&self) -> String {
        // Serializes index including embeddings if set via setEmbeddings()
        // This enables embedding persistence for faster reload
        serde_json::to_string(&self.index).unwrap_or_default()
    }

    /// Set embeddings for semantic search
    /// embeddings_js: Float32Array flattened as [chunk0_dim0, chunk0_dim1, ..., chunk1_dim0, ...]
    /// model_id: Embedding model identifier for cache validation
    /// dimension: Embedding dimension (e.g., 384 for arctic-embed-s)
    #[wasm_bindgen(js_name = setEmbeddings)]
    pub fn set_embeddings(
        &mut self,
        embeddings_js: js_sys::Float32Array,
        model_id: String,
        dimension: usize,
    ) -> Result<(), JsValue> {
        let flat: Vec<f32> = embeddings_js.to_vec();
        let chunk_count = self.index.chunks.len();

        // Validate dimensions
        if flat.len() != chunk_count * dimension {
            return Err(JsValue::from_str(&format!(
                "Embeddings length mismatch: expected {} ({}×{}), got {}",
                chunk_count * dimension,
                chunk_count,
                dimension,
                flat.len()
            )));
        }

        // Convert flat array to Vec<Vec<f32>>
        let mut embeddings = Vec::with_capacity(chunk_count);
        for i in 0..chunk_count {
            let start = i * dimension;
            let end = start + dimension;
            embeddings.push(flat[start..end].to_vec());
        }

        self.index.embeddings = Some(embeddings);
        self.index.embedding_model = Some(model_id);

        Ok(())
    }

    #[wasm_bindgen(js_name = indexId)]
    pub fn index_id(&self) -> String {
        self.index.index_id.clone()
    }

    #[wasm_bindgen(js_name = listOutline)]
    pub fn list_outline(&self, path: String) -> Result<JsValue, JsValue> {
        let outline = ingestor_core::list_outline(&self.index.chunks, &path);
        to_value(&outline).map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = listSymbols)]
    pub fn list_symbols(&self, path: String) -> Result<JsValue, JsValue> {
        let symbols = ingestor_core::list_symbols(&self.index.chunks, &path);
        to_value(&symbols).map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = warnings)]
    pub fn warnings(&self) -> Result<JsValue, JsValue> {
        to_value(&self.index.warnings).map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = stats)]
    pub fn stats(&self) -> Result<JsValue, JsValue> {
        to_value(&self.index.stats).map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = files)]
    pub fn files(&self) -> Result<JsValue, JsValue> {
        to_value(&self.index.files).map_err(to_js_error)
    }
}

fn parse_options(value: JsValue) -> Result<IngestOptions, JsValue> {
    if value.is_null() || value.is_undefined() {
        Ok(IngestOptions::default())
    } else {
        from_value(value).map_err(to_js_error)
    }
}

fn to_js_error<E: std::fmt::Display>(error: E) -> JsValue {
    JsValue::from_str(&error.to_string())
}

fn export_zip_with_assets(index: &IndexFile, assets: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let buffer = Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(buffer);
    let options = FileOptions::default();

    let llm = export_llm_pointer(index);
    writer.start_file("llm.md", options).ok();
    writer.write_all(llm.as_bytes()).ok();

    let catalog = export_catalog_llm_md(index);
    writer.start_file("catalog.llm.md", options).ok();
    writer.write_all(catalog.as_bytes()).ok();

    let outline = export_llm(index);
    writer.start_file("outline.md", options).ok();
    writer.write_all(outline.as_bytes()).ok();

    let index_json = serde_json::to_string(index).unwrap_or_default();
    writer.start_file("index.json", options).ok();
    writer.write_all(index_json.as_bytes()).ok();

    let manifest = export_manifest_json(index);
    writer.start_file("manifest.json", options).ok();
    writer.write_all(manifest.as_bytes()).ok();

    let manifest_min = export_manifest_min_json(index);
    writer.start_file("manifest.min.json", options).ok();
    writer.write_all(manifest_min.as_bytes()).ok();

    let manifest_llm_tsv = export_manifest_llm_tsv(index);
    writer.start_file("manifest.llm.tsv", options).ok();
    writer.write_all(manifest_llm_tsv.as_bytes()).ok();

    for (name, content) in export_chunks(index) {
        writer.start_file(name, options).ok();
        writer.write_all(content.as_bytes()).ok();
    }

    for (path, bytes) in assets {
        writer.start_file(path, options).ok();
        writer.write_all(bytes).ok();
    }

    match writer.finish() {
        Ok(cursor) => cursor.into_inner(),
        Err(_) => Vec::new(),
    }
}

fn export_zip_compact_with_assets(index: &IndexFile, assets: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let buffer = Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(buffer);
    let options = FileOptions::default();

    let llm = export_llm_pointer(index);
    writer.start_file("llm.md", options).ok();
    writer.write_all(llm.as_bytes()).ok();

    let manifest_llm_tsv = export_manifest_llm_tsv(index);
    writer.start_file("manifest.llm.tsv", options).ok();
    writer.write_all(manifest_llm_tsv.as_bytes()).ok();

    for (name, content) in export_chunks_compact(index) {
        writer.start_file(name, options).ok();
        writer.write_all(content.as_bytes()).ok();
    }

    for (path, bytes) in assets {
        writer.start_file(path, options).ok();
        writer.write_all(bytes).ok();
    }

    match writer.finish() {
        Ok(cursor) => cursor.into_inner(),
        Err(_) => Vec::new(),
    }
}

fn collect_assets(files: &[FileInput]) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    for file in files {
        if file.path.to_ascii_lowercase().ends_with(".png")
            || file.path.to_ascii_lowercase().ends_with(".jpg")
            || file.path.to_ascii_lowercase().ends_with(".jpeg")
            || file.path.to_ascii_lowercase().ends_with(".webp")
            || file.path.to_ascii_lowercase().ends_with(".gif")
            || file.path.to_ascii_lowercase().ends_with(".bmp")
        {
            out.insert(format!("images/{}", sanitize_zip_path(&file.path)), file.data.clone());
        }
    }
    out
}

fn merge_assets(out: &mut BTreeMap<String, Vec<u8>>, files: &[FileInput]) {
    for (path, bytes) in collect_assets(files) {
        out.insert(path, bytes);
    }
}

fn sanitize_zip_path(input: &str) -> String {
    let replaced = input.replace('\\', "/");
    let mut parts = Vec::new();
    for part in replaced.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            continue;
        }
        parts.push(part);
    }
    parts.join("/")
}
