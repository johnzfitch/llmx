mod chunk;
mod export;
mod index;
mod model;
mod util;

#[cfg(feature = "mcp")]
pub mod mcp;

#[cfg(feature = "embeddings")]
pub mod embeddings;

pub use crate::export::{export_chunks, export_llm, export_manifest_json, export_zip};
pub use crate::index::{build_inverted_index, compute_stats, list_outline, list_symbols, search_index};
#[cfg(feature = "embeddings")]
pub use crate::index::{hybrid_search, vector_search};
pub use crate::model::*;
use crate::util::{build_chunk_refs, detect_kind, sha256_hex};
use std::collections::BTreeMap;

pub fn ingest_files(mut files: Vec<FileInput>, options: IngestOptions) -> IndexFile {
    files.sort_by(|a, b| a.path.cmp(&b.path));

    let mut warnings = Vec::new();
    let mut total_bytes = 0usize;
    let mut file_metas = Vec::new();
    let mut chunks = Vec::new();

    for file in files {
        let path = file.path;
        let data = file.data;
        let mtime_ms = file.mtime_ms;
        let fingerprint_sha256 = file.fingerprint_sha256;
        if total_bytes + data.len() > options.max_total_bytes {
            warnings.push(IngestWarning {
                path: path.clone(),
                code: "max_total_bytes".to_string(),
                message: "Total size limit exceeded; file skipped.".to_string(),
            });
            continue;
        }
        if data.len() > options.max_file_bytes {
            warnings.push(IngestWarning {
                path: path.clone(),
                code: "max_file_bytes".to_string(),
                message: "File size limit exceeded; file skipped.".to_string(),
            });
            continue;
        }
        total_bytes += data.len();
        let kind = detect_kind(&path);
        let file_hash = sha256_hex(&data);
        let bytes_len = data.len();

        let (line_count, mut file_chunks) = if kind == ChunkKind::Image {
            let mut chunks = chunk::chunk_file(&path, "", kind, &options);
            for chunk in &mut chunks {
                chunk.asset_path = Some(format!("images/{}", sanitize_zip_path(&path)));
            }
            (1usize, chunks)
        } else {
            let text = match String::from_utf8(data) {
                Ok(text) => text,
                Err(_) => {
                    warnings.push(IngestWarning {
                        path: path.clone(),
                        code: "utf8".to_string(),
                        message: "File is not valid UTF-8; file skipped.".to_string(),
                    });
                    continue;
                }
            };
            let line_count = text.lines().count().max(1);
            (line_count, chunk::chunk_file(&path, &text, kind, &options))
        };

        if file_chunks.len() > options.max_chunks_per_file {
            warnings.push(IngestWarning {
                path: path.clone(),
                code: "max_chunks_per_file".to_string(),
                message: "Chunk limit exceeded; file truncated.".to_string(),
            });
            file_chunks.truncate(options.max_chunks_per_file);
        }
        chunks.extend(file_chunks);
        file_metas.push(FileMeta {
            path,
            kind,
            bytes: bytes_len,
            sha256: file_hash,
            line_count,
            mtime_ms,
            fingerprint_sha256,
        });
    }

    chunks.sort_by(|a, b| match a.path.cmp(&b.path) {
        std::cmp::Ordering::Equal => a.start_line.cmp(&b.start_line),
        other => other,
    });

    let chunk_refs = build_chunk_refs(&chunks);
    let inverted_index = build_inverted_index(&chunks);
    let stats = compute_stats(&file_metas, &chunks);
    let index_id = compute_index_id(&file_metas);

    IndexFile {
        version: 1,
        index_id,
        files: file_metas,
        chunks,
        chunk_refs,
        inverted_index,
        stats,
        warnings,
        embeddings: None,
        embedding_model: None,
    }
}

pub fn update_index(prev: IndexFile, files: Vec<FileInput>, options: IngestOptions) -> IndexFile {
    let mut prev_map: BTreeMap<String, (FileMeta, Vec<Chunk>)> = BTreeMap::new();
    let mut chunk_map: BTreeMap<String, Vec<Chunk>> = BTreeMap::new();
    for chunk in prev.chunks {
        chunk_map.entry(chunk.path.clone()).or_default().push(chunk);
    }
    for file in prev.files {
        if let Some(chunks) = chunk_map.remove(&file.path) {
            prev_map.insert(file.path.clone(), (file, chunks));
        }
    }

    let mut new_files = files;
    new_files.sort_by(|a, b| a.path.cmp(&b.path));

    let mut warnings = Vec::new();
    let mut file_metas = Vec::new();
    let mut chunks = Vec::new();
    let mut total_bytes = 0usize;

    for file in new_files {
        let path = file.path;
        let data = file.data;
        let mtime_ms = file.mtime_ms;
        let fingerprint_sha256 = file.fingerprint_sha256;
        if total_bytes + data.len() > options.max_total_bytes {
            warnings.push(IngestWarning {
                path: path.clone(),
                code: "max_total_bytes".to_string(),
                message: "Total size limit exceeded; file skipped.".to_string(),
            });
            continue;
        }
        if data.len() > options.max_file_bytes {
            warnings.push(IngestWarning {
                path: path.clone(),
                code: "max_file_bytes".to_string(),
                message: "File size limit exceeded; file skipped.".to_string(),
            });
            continue;
        }
        total_bytes += data.len();
        let kind = detect_kind(&path);
        let file_hash = sha256_hex(&data);
        let bytes_len = data.len();
        if let Some((meta, existing_chunks)) = prev_map.get(&path) {
            if meta.sha256 == file_hash {
                file_metas.push(meta.clone());
                chunks.extend(existing_chunks.clone());
                continue;
            }
        }

        let (line_count, mut file_chunks) = if kind == ChunkKind::Image {
            let mut chunks = chunk::chunk_file(&path, "", kind, &options);
            for chunk in &mut chunks {
                chunk.asset_path = Some(format!("images/{}", sanitize_zip_path(&path)));
            }
            (1usize, chunks)
        } else {
            let text = match String::from_utf8(data) {
                Ok(text) => text,
                Err(_) => {
                    warnings.push(IngestWarning {
                        path: path.clone(),
                        code: "utf8".to_string(),
                        message: "File is not valid UTF-8; file skipped.".to_string(),
                    });
                    continue;
                }
            };
            let line_count = text.lines().count().max(1);
            (line_count, chunk::chunk_file(&path, &text, kind, &options))
        };

        if file_chunks.len() > options.max_chunks_per_file {
            warnings.push(IngestWarning {
                path: path.clone(),
                code: "max_chunks_per_file".to_string(),
                message: "Chunk limit exceeded; file truncated.".to_string(),
            });
            file_chunks.truncate(options.max_chunks_per_file);
        }
        chunks.extend(file_chunks);
        file_metas.push(FileMeta {
            path,
            kind,
            bytes: bytes_len,
            sha256: file_hash,
            line_count,
            mtime_ms,
            fingerprint_sha256,
        });
    }

    chunks.sort_by(|a, b| match a.path.cmp(&b.path) {
        std::cmp::Ordering::Equal => a.start_line.cmp(&b.start_line),
        other => other,
    });

    let chunk_refs = build_chunk_refs(&chunks);
    let inverted_index = build_inverted_index(&chunks);
    let stats = compute_stats(&file_metas, &chunks);
    let index_id = compute_index_id(&file_metas);

    IndexFile {
        version: 1,
        index_id,
        files: file_metas,
        chunks,
        chunk_refs,
        inverted_index,
        stats,
        warnings,
        embeddings: None,
        embedding_model: None,
    }
}

pub fn update_index_selective(
    prev: IndexFile,
    files: Vec<FileInput>,
    keep_paths: Vec<String>,
    options: IngestOptions,
) -> IndexFile {
    let mut prev_map: BTreeMap<String, (FileMeta, Vec<Chunk>)> = BTreeMap::new();
    let mut chunk_map: BTreeMap<String, Vec<Chunk>> = BTreeMap::new();
    for chunk in prev.chunks {
        chunk_map.entry(chunk.path.clone()).or_default().push(chunk);
    }
    for file in prev.files {
        if let Some(chunks) = chunk_map.remove(&file.path) {
            prev_map.insert(file.path.clone(), (file, chunks));
        }
    }

    let mut warnings = Vec::new();
    let mut file_metas = Vec::new();
    let mut chunks = Vec::new();

    let mut keep_paths_sorted = keep_paths;
    keep_paths_sorted.sort();
    keep_paths_sorted.dedup();

    for path in keep_paths_sorted {
        if let Some((meta, existing_chunks)) = prev_map.get(&path) {
            file_metas.push(meta.clone());
            chunks.extend(existing_chunks.clone());
        }
    }

    let mut new_files = files;
    new_files.sort_by(|a, b| a.path.cmp(&b.path));

    let mut total_bytes = 0usize;
    for file in new_files {
        let path = file.path;
        let data = file.data;
        let mtime_ms = file.mtime_ms;
        let fingerprint_sha256 = file.fingerprint_sha256;
        if total_bytes + data.len() > options.max_total_bytes {
            warnings.push(IngestWarning {
                path: path.clone(),
                code: "max_total_bytes".to_string(),
                message: "Total size limit exceeded; file skipped.".to_string(),
            });
            continue;
        }
        if data.len() > options.max_file_bytes {
            warnings.push(IngestWarning {
                path: path.clone(),
                code: "max_file_bytes".to_string(),
                message: "File size limit exceeded; file skipped.".to_string(),
            });
            continue;
        }
        total_bytes += data.len();

        let kind = detect_kind(&path);
        let file_hash = sha256_hex(&data);
        let bytes_len = data.len();

        if let Some((meta, existing_chunks)) = prev_map.get(&path) {
            if meta.sha256 == file_hash {
                file_metas.push(meta.clone());
                chunks.extend(existing_chunks.clone());
                continue;
            }
        }

        let (line_count, mut file_chunks) = if kind == ChunkKind::Image {
            let mut chunks = chunk::chunk_file(&path, "", kind, &options);
            for chunk in &mut chunks {
                chunk.asset_path = Some(format!("images/{}", sanitize_zip_path(&path)));
            }
            (1usize, chunks)
        } else {
            let text = match String::from_utf8(data) {
                Ok(text) => text,
                Err(_) => {
                    warnings.push(IngestWarning {
                        path: path.clone(),
                        code: "utf8".to_string(),
                        message: "File is not valid UTF-8; file skipped.".to_string(),
                    });
                    continue;
                }
            };
            let line_count = text.lines().count().max(1);
            (line_count, chunk::chunk_file(&path, &text, kind, &options))
        };

        if file_chunks.len() > options.max_chunks_per_file {
            warnings.push(IngestWarning {
                path: path.clone(),
                code: "max_chunks_per_file".to_string(),
                message: "Chunk limit exceeded; file truncated.".to_string(),
            });
            file_chunks.truncate(options.max_chunks_per_file);
        }
        chunks.extend(file_chunks);
        file_metas.push(FileMeta {
            path,
            kind,
            bytes: bytes_len,
            sha256: file_hash,
            line_count,
            mtime_ms,
            fingerprint_sha256,
        });
    }

    file_metas.sort_by(|a, b| a.path.cmp(&b.path));
    chunks.sort_by(|a, b| match a.path.cmp(&b.path) {
        std::cmp::Ordering::Equal => a.start_line.cmp(&b.start_line),
        other => other,
    });

    let chunk_refs = build_chunk_refs(&chunks);
    let inverted_index = build_inverted_index(&chunks);
    let stats = compute_stats(&file_metas, &chunks);
    let index_id = compute_index_id(&file_metas);

    IndexFile {
        version: 1,
        index_id,
        files: file_metas,
        chunks,
        chunk_refs,
        inverted_index,
        stats,
        warnings,
        embeddings: None,
        embedding_model: None,
    }
}

pub fn get_chunk(index: &IndexFile, chunk_id: &str) -> Option<Chunk> {
    index.chunks.iter().find(|c| c.id == chunk_id).cloned()
}

pub fn search(index: &IndexFile, query: &str, filters: SearchFilters, limit: usize) -> Vec<SearchResult> {
    search_index(
        &index.chunks,
        &index.inverted_index,
        &index.chunk_refs,
        query,
        &filters,
        limit,
    )
}

fn compute_index_id(files: &[FileMeta]) -> String {
    let mut seed = String::new();
    for file in files {
        seed.push_str(&file.path);
        seed.push('\n');
        seed.push_str(&file.sha256);
        seed.push('\n');
    }
    sha256_hex(seed.as_bytes())
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
