use crate::model::{Chunk, ChunkKind, FileMeta, IndexFile};
use crate::util::build_chunk_refs;
use serde::Serialize;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::io::{Cursor, Write};
use zip::write::FileOptions;

pub fn export_llm(index: &IndexFile) -> String {
    let mut out = String::new();
    let mut chunks = index.chunks.clone();
    chunks.sort_by(chunk_sort);

    let file_meta = build_file_meta_map(&index.files);
    let refs = if index.chunk_refs.is_empty() {
        build_chunk_refs(&chunks)
    } else {
        index.chunk_refs.clone()
    };

    out.push_str("# llm.md (pointer manifest)\n\n");
    out.push_str("Index ID: ");
    out.push_str(&index.index_id);
    out.push_str("\nFiles: ");
    out.push_str(&index.files.len().to_string());
    out.push_str("  Chunks: ");
    out.push_str(&chunks.len().to_string());
    out.push_str("\n\n");
    out.push_str("Chunk files live under `chunks/` and are named `{ref}.md`.\n");
    out.push_str("Prefer search to find refs, then open only the referenced chunk files.\n\n");

    if !index.warnings.is_empty() {
        out.push_str("## Warnings\n\n");
        out.push_str("Some files were skipped or truncated.\n\n");
        for warning in &index.warnings {
            out.push_str(&format!(
                "- {}: {}\n",
                markdown_code_span(&warning.path),
                sanitize_single_line(&warning.message)
            ));
        }
        out.push('\n');
    }

    out.push_str("## Files\n\n");

    let mut current_path = String::new();
    for chunk in &chunks {
        if chunk.path != current_path {
            current_path = chunk.path.clone();
            if let Some(meta) = file_meta.get(current_path.as_str()) {
                let kind_short = kind_short_label(meta.kind);
                out.push_str(&format!(
                    "### {} ({}, {} lines)\n",
                    &current_path, kind_short, meta.line_count
                ));
            } else {
                out.push_str(&format!("### {}\n", &current_path));
            }
        }

        let chunk_ref = refs.get(chunk.id.as_str()).map(String::as_str).unwrap_or(&chunk.short_id);
        out.push_str(&render_chunk_entry_outline(chunk, chunk_ref));
        out.push('\n');
    }

    out
}

pub fn export_chunks(index: &IndexFile) -> Vec<(String, String)> {
    let mut chunks = index.chunks.clone();
    chunks.sort_by(chunk_sort);
    let refs = if index.chunk_refs.is_empty() {
        build_chunk_refs(&chunks)
    } else {
        index.chunk_refs.clone()
    };
    chunks
        .into_iter()
        .enumerate()
        .map(|(idx, chunk)| {
            let mut body = String::new();
            let (content, compacted) = compact_for_export(&chunk);
            let chunk_ref = refs.get(chunk.id.as_str()).map(String::as_str).unwrap_or(&chunk.short_id);
            body.push_str(&chunk_front_matter(idx + 1, &chunk, compacted, chunk_ref));
            body.push_str("\n\n");
            body.push_str(&content);
            let name = format!("chunks/{}.md", chunk_ref);
            (name, body)
        })
        .collect()
}

pub fn export_zip(index: &IndexFile) -> Vec<u8> {
    let buffer = Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(buffer);
    let options = FileOptions::default();

    let llm = export_llm(index);
    writer.start_file("llm.md", options).ok();
    writer.write_all(llm.as_bytes()).ok();

    let index_json = serde_json::to_string(index).unwrap_or_default();
    writer.start_file("index.json", options).ok();
    writer.write_all(index_json.as_bytes()).ok();

    let manifest = export_manifest(index);
    writer.start_file("manifest.json", options).ok();
    writer.write_all(manifest.as_bytes()).ok();

    for (name, content) in export_chunks(index) {
        writer.start_file(name, options).ok();
        writer.write_all(content.as_bytes()).ok();
    }

    match writer.finish() {
        Ok(cursor) => cursor.into_inner(),
        Err(_) => Vec::new(),
    }
}

pub fn export_manifest_json(index: &IndexFile) -> String {
    export_manifest(index)
}

#[derive(Debug, Serialize)]
struct ManifestV2 {
    format_version: u32,
    index_id: String,
    files: Vec<FileMeta>,
    paths: Vec<String>,
    kinds: Vec<String>,
    chunk_columns: Vec<&'static str>,
    chunks: Vec<ManifestChunkRowV2>,
}

#[derive(Debug, Serialize)]
struct ManifestChunkRowV2(
    String,         // ref
    String,         // id
    String,         // slug
    usize,          // path_i
    usize,          // kind_i
    usize,          // start_line
    usize,          // end_line
    usize,          // token_estimate
    String,         // content_sha256
    Vec<String>,    // heading_path
    Option<String>, // symbol
    Option<String>, // address
    Option<String>, // asset_path
);

fn export_manifest(index: &IndexFile) -> String {
    let mut chunks = index.chunks.clone();
    chunks.sort_by(chunk_sort);
    let refs = if index.chunk_refs.is_empty() {
        build_chunk_refs(&chunks)
    } else {
        index.chunk_refs.clone()
    };

    let mut paths: Vec<String> = Vec::new();
    let mut path_ids: BTreeMap<String, usize> = BTreeMap::new();
    let mut kinds: Vec<String> = Vec::new();
    let mut kind_ids: BTreeMap<String, usize> = BTreeMap::new();

    let mut rows = Vec::new();
    for chunk in chunks {
        let chunk_ref = refs
            .get(chunk.id.as_str())
            .cloned()
            .unwrap_or_else(|| chunk.short_id.clone());

        let path_i = match path_ids.get(chunk.path.as_str()) {
            Some(id) => *id,
            None => {
                let id = paths.len();
                paths.push(chunk.path.clone());
                path_ids.insert(chunk.path.clone(), id);
                id
            }
        };

        let kind_label = kind_label(chunk.kind).to_string();
        let kind_i = match kind_ids.get(kind_label.as_str()) {
            Some(id) => *id,
            None => {
                let id = kinds.len();
                kinds.push(kind_label.clone());
                kind_ids.insert(kind_label, id);
                id
            }
        };

        rows.push(ManifestChunkRowV2(
            chunk_ref,
            chunk.id,
            chunk.slug,
            path_i,
            kind_i,
            chunk.start_line,
            chunk.end_line,
            chunk.token_estimate,
            chunk.content_hash,
            chunk.heading_path,
            chunk.symbol,
            chunk.address,
            chunk.asset_path,
        ));
    }

    let manifest = ManifestV2 {
        format_version: 2,
        index_id: index.index_id.clone(),
        files: index.files.clone(),
        paths,
        kinds,
        chunk_columns: vec![
            "ref",
            "id",
            "slug",
            "path_i",
            "kind_i",
            "start_line",
            "end_line",
            "token_estimate",
            "content_sha256",
            "heading_path",
            "symbol",
            "address",
            "asset_path",
        ],
        chunks: rows,
    };

    serde_json::to_string(&manifest).unwrap_or_default()
}

fn chunk_front_matter(index: usize, chunk: &Chunk, compacted: bool, chunk_ref: &str) -> String {
    let compact_flag = if compacted { "true" } else { "false" };
    let heading_json = serde_json::to_string(&chunk.heading_path).unwrap_or_else(|_| "[]".to_string());
    let symbol_json = serde_json::to_string(&chunk.symbol).unwrap_or_else(|_| "null".to_string());
    let address_json = serde_json::to_string(&chunk.address).unwrap_or_else(|_| "null".to_string());
    let asset_json = serde_json::to_string(&chunk.asset_path).unwrap_or_else(|_| "null".to_string());
    format!(
        "---\nchunk_index: {}\nref: {}\nid: {}\nslug: {}\npath: {}\nkind: {}\nlines: [{}, {}]\ntoken_estimate: {}\ncontent_sha256: {}\ncompacted: {}\nheading_path: {}\nsymbol: {}\naddress: {}\nasset_path: {}\n---",
        index,
        yaml_string(chunk_ref),
        yaml_string(&chunk.id),
        yaml_string(&chunk.slug),
        yaml_string(&chunk.path),
        yaml_string(kind_label(chunk.kind)),
        chunk.start_line,
        chunk.end_line,
        chunk.token_estimate,
        yaml_string(&chunk.content_hash),
        compact_flag,
        heading_json,
        symbol_json,
        address_json
        ,
        asset_json
    )
}

fn chunk_sort(a: &Chunk, b: &Chunk) -> Ordering {
    match a.path.cmp(&b.path) {
        Ordering::Equal => a.start_line.cmp(&b.start_line),
        other => other,
    }
}

fn compact_for_export(chunk: &Chunk) -> (String, bool) {
    if chunk.kind != ChunkKind::Text {
        return (chunk.content.clone(), false);
    }
    compact_repeated_lines(&chunk.content, 3)
}

fn compact_repeated_lines(text: &str, min_repeat: usize) -> (String, bool) {
    let mut out: Vec<String> = Vec::new();
    let mut current: Option<String> = None;
    let mut count = 0usize;
    let mut compacted = false;

    for line in text.lines() {
        match &current {
            Some(prev) if prev == line => {
                count += 1;
            }
            Some(prev) => {
                push_run(&mut out, prev, count, min_repeat, &mut compacted);
                current = Some(line.to_string());
                count = 1;
            }
            None => {
                current = Some(line.to_string());
                count = 1;
            }
        }
    }
    if let Some(prev) = current {
        push_run(&mut out, &prev, count, min_repeat, &mut compacted);
    }

    (out.join("\n"), compacted)
}

fn push_run(out: &mut Vec<String>, line: &str, count: usize, min_repeat: usize, compacted: &mut bool) {
    if count >= min_repeat {
        out.push(line.to_string());
        out.push(format!(
            "... (previous line repeated {} more times)",
            count.saturating_sub(1)
        ));
        *compacted = true;
    } else {
        for _ in 0..count {
            out.push(line.to_string());
        }
    }
}

fn yaml_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn kind_label(kind: ChunkKind) -> &'static str {
    match kind {
        ChunkKind::Markdown => "markdown",
        ChunkKind::Json => "json",
        ChunkKind::JavaScript => "java_script",
        ChunkKind::Html => "html",
        ChunkKind::Text => "text",
        ChunkKind::Image => "image",
        ChunkKind::Unknown => "unknown",
    }
}

fn kind_short_label(kind: ChunkKind) -> &'static str {
    match kind {
        ChunkKind::Markdown => "md",
        ChunkKind::Json => "json",
        ChunkKind::JavaScript => "js",
        ChunkKind::Html => "html",
        ChunkKind::Text => "txt",
        ChunkKind::Image => "img",
        ChunkKind::Unknown => "?",
    }
}

fn build_file_meta_map(files: &[FileMeta]) -> BTreeMap<&str, &FileMeta> {
    let mut map = BTreeMap::new();
    for meta in files {
        map.insert(meta.path.as_str(), meta);
    }
    map
}

fn render_chunk_entry_outline(chunk: &Chunk, chunk_ref: &str) -> String {
    let lines = format!("{}-{}", chunk.start_line, chunk.end_line);

    // Build semantic label based on content type
    let semantic = match (chunk.kind, &chunk.symbol, chunk.heading_path.is_empty()) {
        // Code with symbol: show as function/class
        (ChunkKind::JavaScript, Some(sym), _) => format!("`{}()`", sym),
        // Markdown with headings: show breadcrumb path
        (ChunkKind::Markdown, _, false) => {
            chunk.heading_path.iter().rev().take(3).rev()
                .cloned().collect::<Vec<_>>().join(" > ")
        }
        // Fallback to slug
        _ => chunk.slug.clone(),
    };

    format!("- {} ({}) {}", chunk_ref, lines, semantic)
}

fn sanitize_single_line(input: &str) -> String {
    input
        .chars()
        .map(|ch| if ch == '\n' || ch == '\r' || ch.is_control() { ' ' } else { ch })
        .collect()
}

fn markdown_code_span(input: &str) -> String {
    let cleaned = sanitize_single_line(input);
    let fence_len = max_backtick_run(&cleaned) + 1;
    let fence = "`".repeat(fence_len);
    if cleaned.starts_with(' ') || cleaned.ends_with(' ') {
        format!("{fence} {cleaned} {fence}")
    } else {
        format!("{fence}{cleaned}{fence}")
    }
}

fn max_backtick_run(input: &str) -> usize {
    let mut max_run = 0usize;
    let mut current = 0usize;
    for ch in input.chars() {
        if ch == '`' {
            current += 1;
            max_run = max_run.max(current);
        } else {
            current = 0;
        }
    }
    max_run
}
