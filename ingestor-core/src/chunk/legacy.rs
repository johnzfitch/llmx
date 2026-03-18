use crate::model::{Chunk, ChunkKind, IngestOptions};
use crate::util::{estimate_tokens, sha256_hex, short_id, slugify};
use regex::Regex;
use std::collections::BTreeMap;
use std::sync::OnceLock;
#[cfg(feature = "treesitter")]
use tree_sitter::{Language, Node, Parser};

fn markdown_heading_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(#{1,6})\s+(.+)").expect("markdown heading regex"))
}

fn html_heading_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)<h([1-6])[^>]*>(.*?)</h[1-6]>").expect("html heading regex"))
}

fn html_tag_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"<[^>]+>").expect("html tag regex"))
}

#[derive(Debug, Clone)]
pub(crate) struct ChunkDraft {
    pub(crate) kind: ChunkKind,
    pub(crate) start_line: usize,
    pub(crate) end_line: usize,
    pub(crate) content: String,
    pub(crate) heading_path: Vec<String>,
    pub(crate) symbol: Option<String>,
    pub(crate) address: Option<String>,
    // Phase 7: structural enrichment (populated by tree-sitter)
    pub(crate) ast_kind: Option<crate::model::AstNodeKind>,
    pub(crate) qualified_name: Option<String>,
    pub(crate) signature: Option<String>,
    pub(crate) parent_symbol: Option<String>,
    pub(crate) imports: Vec<String>,
    pub(crate) exports: Vec<String>,
    pub(crate) calls: Vec<String>,
    pub(crate) type_refs: Vec<String>,
    pub(crate) doc_summary: Option<String>,
    pub(crate) symbol_id: Option<String>,
    pub(crate) symbol_tail: Option<String>,
    pub(crate) module_path: Option<String>,
    pub(crate) visibility: Option<crate::Visibility>,
}

impl ChunkDraft {
    /// Create a plain draft with no structural enrichment (non-code content).
    pub(crate) fn plain(kind: ChunkKind, start_line: usize, end_line: usize, content: String, heading_path: Vec<String>, symbol: Option<String>, address: Option<String>) -> Self {
        Self {
            kind, start_line, end_line, content, heading_path, symbol, address,
            ast_kind: None, qualified_name: None, signature: None, parent_symbol: None,
            imports: Vec::new(), exports: Vec::new(), calls: Vec::new(), type_refs: Vec::new(),
            doc_summary: None, symbol_id: None, symbol_tail: None, module_path: None, visibility: None,
        }
    }
}

pub(crate) fn chunk_file(path: &str, text: &str, kind: ChunkKind, options: &IngestOptions) -> Vec<Chunk> {
    let drafts = match kind {
        ChunkKind::Markdown => chunk_markdown(text, options),
        ChunkKind::Json => chunk_json(text, options),
        ChunkKind::JavaScript => chunk_javascript(path, text, options),
        ChunkKind::Html => chunk_html(text, options),
        ChunkKind::Image => chunk_image(path),
        ChunkKind::Text | ChunkKind::Unknown => chunk_text(text, options),
    };
    finalize_chunks(path, drafts)
}

pub(crate) fn finalize_chunks(path: &str, drafts: Vec<ChunkDraft>) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut hash_counts: BTreeMap<String, usize> = BTreeMap::new();
    for (index, draft) in drafts.into_iter().enumerate() {
        let content_hash = sha256_hex(draft.content.as_bytes());
        let count = hash_counts.entry(content_hash.clone()).or_insert(0);
        let occurrence = *count;
        *count += 1;
        let id_seed = format!("{}\n{}\n{}", path, content_hash, occurrence);
        let id = sha256_hex(id_seed.as_bytes());
        let token_estimate = estimate_tokens(&draft.content);
        let content = draft.content;
        let short_id = short_id(&id, 12);
        let slug = make_slug(
            path,
            draft.kind,
            &draft.heading_path,
            &draft.symbol,
            &draft.address,
            draft.start_line,
            draft.end_line,
        );
        chunks.push(Chunk {
            id,
            short_id,
            slug,
            path: path.to_string(),
            root_path: String::new(),
            relative_path: path.to_string(),
            kind: draft.kind,
            language: None,
            chunk_index: index,
            start_line: draft.start_line,
            end_line: draft.end_line,
            content,
            content_hash,
            token_estimate,
            heading_path: draft.heading_path,
            symbol: draft.symbol,
            address: draft.address,
            asset_path: None,
            is_generated: false,
            quality_score: None,
            resolution_tier: crate::ResolutionTier::TextOnly,
            // Phase 7: structural metadata from tree-sitter enrichment
            ast_kind: draft.ast_kind,
            qualified_name: draft.qualified_name,
            symbol_id: draft.symbol_id,
            symbol_tail: draft.symbol_tail,
            signature: draft.signature,
            module_path: draft.module_path,
            parent_symbol: draft.parent_symbol,
            visibility: draft.visibility,
            imports: draft.imports,
            exports: draft.exports,
            calls: draft.calls,
            type_refs: draft.type_refs,
            doc_summary: draft.doc_summary,
        });
    }
    chunks
}

fn chunk_image(path: &str) -> Vec<ChunkDraft> {
    let name = path
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .trim()
        .to_string();
    vec![ChunkDraft::plain(
        ChunkKind::Image, 1, 1,
        format!("Image: {name}\nSource: {path}"),
        Vec::new(), None, None,
    )]
}

fn chunk_markdown(text: &str, options: &IngestOptions) -> Vec<ChunkDraft> {
    let mut drafts = Vec::new();
    let mut buf: Vec<String> = Vec::new();
    let mut heading_stack: Vec<String> = Vec::new();
    let mut current_heading = heading_stack.clone();
    let mut start_line = 1;
    let mut in_fence = false;
    let heading_re = markdown_heading_re();

    for (idx, line) in text.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
        }

        if !in_fence {
            if let Some(caps) = heading_re.captures(line) {
                flush_chunk(&mut drafts, &mut buf, &current_heading, ChunkFlushParams {
                    start_line,
                    end_line: line_no - 1,
                    kind: ChunkKind::Markdown,
                    symbol: None,
                    address: None,
                });
                let level = caps.get(1).unwrap().as_str().len();
                let title = caps.get(2).unwrap().as_str().trim().to_string();
                while heading_stack.len() >= level {
                    heading_stack.pop();
                }
                heading_stack.push(title);
                current_heading = heading_stack.clone();
                start_line = line_no;
            }
        }

        buf.push(line.to_string());
        if !in_fence && buffer_len(&buf) >= options.chunk_max_chars {
            flush_chunk(&mut drafts, &mut buf, &current_heading, ChunkFlushParams {
                start_line,
                end_line: line_no,
                kind: ChunkKind::Markdown,
                symbol: None,
                address: None,
            });
            start_line = line_no + 1;
        }
    }

    flush_chunk(&mut drafts, &mut buf, &current_heading, ChunkFlushParams {
        start_line,
        end_line: line_count(text),
        kind: ChunkKind::Markdown,
        symbol: None,
        address: None,
    });
    drafts
}

fn chunk_text(text: &str, options: &IngestOptions) -> Vec<ChunkDraft> {
    let mut drafts = Vec::new();
    let mut buf: Vec<String> = Vec::new();
    let mut start_line = 1;
    for (idx, line) in text.lines().enumerate() {
        let line_no = idx + 1;

        if line.len() > options.chunk_max_chars {
            flush_chunk(&mut drafts, &mut buf, &[], ChunkFlushParams {
                start_line,
                end_line: line_no.saturating_sub(1),
                kind: ChunkKind::Text,
                symbol: None,
                address: None,
            });

            for slice in split_string_by_chars(line, options.chunk_max_chars) {
                drafts.push(ChunkDraft::plain(
                    ChunkKind::Text, line_no, line_no,
                    slice.trim().to_string(),
                    Vec::new(), None, None,
                ));
            }

            start_line = line_no + 1;
            continue;
        }

        if line.trim().is_empty() && !buf.is_empty() {
            if buffer_len(&buf) >= options.chunk_target_chars {
                flush_chunk(&mut drafts, &mut buf, &[], ChunkFlushParams {
                    start_line,
                    end_line: line_no,
                    kind: ChunkKind::Text,
                    symbol: None,
                    address: None,
                });
                start_line = line_no + 1;
            } else {
                buf.push(line.to_string());
            }
            continue;
        }
        buf.push(line.to_string());
        if buffer_len(&buf) >= options.chunk_max_chars {
            flush_chunk(&mut drafts, &mut buf, &[], ChunkFlushParams {
                start_line,
                end_line: line_no,
                kind: ChunkKind::Text,
                symbol: None,
                address: None,
            });
            start_line = line_no + 1;
        }
    }
    flush_chunk(&mut drafts, &mut buf, &[], ChunkFlushParams {
        start_line,
        end_line: line_count(text),
        kind: ChunkKind::Text,
        symbol: None,
        address: None,
    });
    drafts
}

fn split_string_by_chars(input: &str, max_chars: usize) -> Vec<String> {
    if max_chars == 0 {
        return vec![String::new()];
    }
    // Fast path: if bytes <= max_chars then chars <= max_chars (each char is >= 1 byte).
    if input.len() <= max_chars {
        return vec![input.to_string()];
    }

    let mut out = Vec::new();
    let mut start = 0usize;
    let mut count = 0usize;
    for (idx, ch) in input.char_indices() {
        if count == max_chars {
            out.push(input[start..idx].to_string());
            start = idx;
            count = 0;
        }
        count += 1;
        if ch == '\n' {
            // Should never occur because we split by `lines()`, but keep this defensive.
            out.push(input[start..idx].to_string());
            start = idx + ch.len_utf8();
            count = 0;
        }
    }

    if start < input.len() {
        out.push(input[start..].to_string());
    }
    out
}

fn decode_html_entity(entity: &str) -> Option<char> {
    match entity {
        "lt;" => Some('<'),
        "gt;" => Some('>'),
        "amp;" => Some('&'),
        "quot;" => Some('"'),
        "apos;" => Some('\''),
        "nbsp;" => Some(' '),
        "#34;" => Some('"'),
        "#39;" => Some('\''),
        _ => {
            if let Some(rest) = entity.strip_prefix("#x") {
                let hex = rest.strip_suffix(';')?;
                let value = u32::from_str_radix(hex, 16).ok()?;
                char::from_u32(value)
            } else if let Some(rest) = entity.strip_prefix("#X") {
                let hex = rest.strip_suffix(';')?;
                let value = u32::from_str_radix(hex, 16).ok()?;
                char::from_u32(value)
            } else if let Some(rest) = entity.strip_prefix('#') {
                let dec = rest.strip_suffix(';')?;
                let value = dec.parse::<u32>().ok()?;
                char::from_u32(value)
            } else {
                None
            }
        }
    }
}

fn decode_html_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '&' {
            out.push(ch);
            continue;
        }
        let mut entity = String::new();
        while let Some(&next) = chars.peek() {
            entity.push(next);
            chars.next();
            if next == ';' {
                break;
            }
            if entity.len() > 16 {
                break;
            }
        }
        if let Some(decoded) = decode_html_entity(&entity) {
            out.push(decoded);
        } else {
            out.push('&');
            out.push_str(&entity);
        }
    }
    out
}

fn collapse_whitespace(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_space = false;
    for ch in input.chars() {
        let is_ws = ch.is_whitespace() || ch == '\u{00a0}';
        if is_ws {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out.trim().to_string()
}

fn normalize_html_text(input: &str) -> String {
    collapse_whitespace(&decode_html_entities(input))
}

fn should_skip_html_line(line: &str) -> bool {
    if line.is_empty() {
        return true;
    }
    match line {
        "Prev" | "Next" | "Show more" => return true,
        _ => {}
    }
    if line.len() <= 3 && line.as_bytes().iter().all(|b| b.is_ascii_digit()) {
        return true;
    }
    false
}

fn chunk_html(text: &str, options: &IngestOptions) -> Vec<ChunkDraft> {
    let mut drafts = Vec::new();
    let mut buf: Vec<String> = Vec::new();
    let mut heading_stack: Vec<String> = Vec::new();
    let mut current_heading = heading_stack.clone();
    let mut start_line = 1;
    let heading_re = html_heading_re();
    let tag_re = html_tag_re();
    let mut in_script = false;
    let mut in_style = false;

    for (idx, raw_line) in text.lines().enumerate() {
        let line_no = idx + 1;
        let mut line = raw_line.to_string();
        let lower = line.to_ascii_lowercase();
        if lower.contains("<script") {
            in_script = true;
        }
        if lower.contains("</script>") {
            in_script = false;
            continue;
        }
        if lower.contains("<style") {
            in_style = true;
        }
        if lower.contains("</style>") {
            in_style = false;
            continue;
        }
        if in_script || in_style {
            continue;
        }

        if let Some(caps) = heading_re.captures(&line) {
            flush_chunk(&mut drafts, &mut buf, &current_heading, ChunkFlushParams {
                start_line,
                end_line: line_no - 1,
                kind: ChunkKind::Html,
                symbol: None,
                address: None,
            });
            let level: usize = caps.get(1).unwrap().as_str().parse().unwrap_or(1);
            let raw_title = tag_re.replace_all(caps.get(2).unwrap().as_str(), "");
            let title = normalize_html_text(raw_title.as_ref());
            while heading_stack.len() >= level {
                heading_stack.pop();
            }
            heading_stack.push(title);
            current_heading = heading_stack.clone();
            start_line = line_no;
        }

        line = tag_re.replace_all(&line, " ").to_string();
        let normalized = normalize_html_text(&line);
        if !should_skip_html_line(&normalized) {
            buf.push(normalized);
        }

        if buffer_len(&buf) >= options.chunk_max_chars {
            flush_chunk(&mut drafts, &mut buf, &current_heading, ChunkFlushParams {
                start_line,
                end_line: line_no,
                kind: ChunkKind::Html,
                symbol: None,
                address: None,
            });
            start_line = line_no + 1;
        }
    }

    flush_chunk(&mut drafts, &mut buf, &current_heading, ChunkFlushParams {
        start_line,
        end_line: line_count(text),
        kind: ChunkKind::Html,
        symbol: None,
        address: None,
    });
    drafts
}

fn chunk_json(text: &str, options: &IngestOptions) -> Vec<ChunkDraft> {
    let mut drafts = Vec::new();
    const MAX_JSON_PARSE_BYTES: usize = 512 * 1024;
    if text.len() > MAX_JSON_PARSE_BYTES {
        return chunk_text(text, options)
            .into_iter()
            .map(|draft| ChunkDraft { kind: ChunkKind::Json, ..draft })
            .collect();
    }

    let value: serde_json::Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(_) => {
            return chunk_text(text, options)
                .into_iter()
                .map(|draft| ChunkDraft { kind: ChunkKind::Json, ..draft })
                .collect();
        }
    };
    let line_count = line_count(text);
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                let address = format!("$.{}", key);
                let content = serde_json::to_string(&value).unwrap_or_default();
                if content.len() <= options.chunk_max_chars {
                    drafts.push(ChunkDraft::plain(
                        ChunkKind::Json, 1, line_count, content,
                        vec![key.clone()], Some(key), Some(address),
                    ));
                } else {
                    for (idx, slice) in split_string_by_chars(&content, options.chunk_max_chars).into_iter().enumerate() {
                        drafts.push(ChunkDraft::plain(
                            ChunkKind::Json, 1, line_count, slice,
                            vec![key.clone()], Some(key.clone()), Some(format!("{address}#{}", idx + 1)),
                        ));
                    }
                }
            }
        }
        serde_json::Value::Array(list) => {
            let mut start = 0usize;
            while start < list.len() {
                let mut end = (start + 50).min(list.len());
                while end > start + 1 {
                    let slice = &list[start..end];
                    let content = serde_json::to_string(&slice).unwrap_or_default();
                    if content.len() <= options.chunk_max_chars {
                        break;
                    }
                    end = start + ((end - start) / 2).max(1);
                }

                let slice = &list[start..end];
                let address = format!("$[{}:{}]", start, end);
                let content = serde_json::to_string(&slice).unwrap_or_default();
                if content.len() <= options.chunk_max_chars {
                    drafts.push(ChunkDraft::plain(
                        ChunkKind::Json, 1, line_count, content,
                        Vec::new(), None, Some(address),
                    ));
                } else {
                    for (idx, slice) in split_string_by_chars(&content, options.chunk_max_chars).into_iter().enumerate() {
                        drafts.push(ChunkDraft::plain(
                            ChunkKind::Json, 1, line_count, slice,
                            Vec::new(), None, Some(format!("{address}#{}", idx + 1)),
                        ));
                    }
                }
                start = end;
            }
        }
        _ => {
            let content = serde_json::to_string(&value).unwrap_or_default();
            if content.len() <= options.chunk_max_chars {
                drafts.push(ChunkDraft::plain(
                    ChunkKind::Json, 1, line_count, content,
                    Vec::new(), None, Some("$".to_string()),
                ));
            } else {
                for (idx, slice) in split_string_by_chars(&content, options.chunk_max_chars).into_iter().enumerate() {
                    drafts.push(ChunkDraft::plain(
                        ChunkKind::Json, 1, line_count, slice,
                        Vec::new(), None, Some(format!("$#{}", idx + 1)),
                    ));
                }
            }
        }
    }
    drafts
}

fn chunk_javascript(path: &str, text: &str, options: &IngestOptions) -> Vec<ChunkDraft> {
    let _ = path;
    #[cfg(not(feature = "treesitter"))]
    {
        return chunk_text(text, options)
            .into_iter()
            .map(|draft| ChunkDraft { kind: ChunkKind::JavaScript, ..draft })
            .collect();
    }
    #[cfg(feature = "treesitter")]
    {
    let language = select_js_language(path);
    let mut parser = Parser::new();
    if parser.set_language(language).is_err() {
        return chunk_text(text, options)
            .into_iter()
            .map(|draft| ChunkDraft { kind: ChunkKind::JavaScript, ..draft })
            .collect();
    }

    let tree = match parser.parse(text, None) {
        Some(tree) => tree,
        None => {
            return chunk_text(text, options)
                .into_iter()
                .map(|draft| ChunkDraft { kind: ChunkKind::JavaScript, ..draft })
                .collect();
        }
    };

    let root = tree.root_node();

    // Collect file-level imports (not chunked separately, but tracked for cross-ref)
    let file_imports = collect_file_imports(root, text);

    let mut drafts = Vec::new();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if let Some(draft) = enriched_draft_from_node(text, child, None, &file_imports) {
            drafts.push(draft);
        }
        collect_class_member_drafts(child, text, &file_imports, &mut drafts);
    }

    if drafts.is_empty() {
        chunk_text(text, options)
            .into_iter()
            .map(|draft| ChunkDraft { kind: ChunkKind::JavaScript, ..draft })
            .collect()
    } else {
        drafts
    }
    }
}

/// Classify a tree-sitter node kind string into our AstNodeKind enum.
#[cfg(feature = "treesitter")]
fn classify_node_kind(node: Node, text: &str) -> Option<crate::model::AstNodeKind> {
    use crate::model::AstNodeKind;
    match node.kind() {
        "function_declaration" | "function" | "generator_function_declaration" => {
            // Check for test patterns: function name starts with test/it/describe
            let name = node_name_text(node, text).unwrap_or_default();
            if is_test_name(&name) { Some(AstNodeKind::Test) } else { Some(AstNodeKind::Function) }
        }
        "arrow_function" => Some(AstNodeKind::Function),
        "class_declaration" | "class" | "abstract_class_declaration" => Some(AstNodeKind::Class),
        "method_definition" => Some(AstNodeKind::Method),
        "interface_declaration" => Some(AstNodeKind::Interface),
        "type_alias_declaration" => Some(AstNodeKind::Type),
        "enum_declaration" => Some(AstNodeKind::Enum),
        "import_statement" => Some(AstNodeKind::Import),
        "export_statement" => {
            // Unwrap: classify the inner declaration if present
            let mut child_cursor = node.walk();
            for child in node.children(&mut child_cursor) {
                if let Some(inner) = classify_node_kind(child, text) {
                    return Some(inner);
                }
            }
            Some(AstNodeKind::Export)
        }
        "lexical_declaration" | "variable_declaration" => {
            // Check if the value is an arrow function → Function, else Variable/Constant
            if has_arrow_function_value(node) {
                let name = lexical_name(node, text).unwrap_or_default();
                if is_test_name(&name) { Some(AstNodeKind::Test) } else { Some(AstNodeKind::Function) }
            } else if node.kind() == "lexical_declaration" {
                // const → Constant, let/var → Variable
                let first_text = node.child(0)
                    .and_then(|c| text.get(c.start_byte()..c.end_byte()))
                    .unwrap_or("");
                if first_text == "const" { Some(AstNodeKind::Constant) } else { Some(AstNodeKind::Variable) }
            } else {
                Some(AstNodeKind::Variable)
            }
        }
        _ => None,
    }
}

/// Check if a node should produce a chunk (top-level or class member).
#[cfg(feature = "treesitter")]
fn is_chunkable_node(node: Node) -> bool {
    matches!(
        node.kind(),
        "function_declaration" | "class_declaration" | "method_definition"
        | "generator_function_declaration" | "abstract_class_declaration"
        | "interface_declaration" | "type_alias_declaration" | "enum_declaration"
        | "export_statement" | "lexical_declaration" | "variable_declaration"
    )
}

/// Build an enriched ChunkDraft from a tree-sitter node with full structural metadata.
#[cfg(feature = "treesitter")]
fn enriched_draft_from_node(
    text: &str,
    node: Node,
    parent_name: Option<&str>,
    file_imports: &[String],
) -> Option<ChunkDraft> {
    use crate::model::AstNodeKind;

    // For export statements, try to unwrap to the inner declaration
    let (effective_node, is_exported) = if node.kind() == "export_statement" {
        let inner = find_inner_declaration(node);
        match inner {
            Some(inner) => (inner, true),
            None => (node, true), // bare export
        }
    } else {
        (node, false)
    };

    let ast_kind = classify_node_kind(effective_node, text)?;

    // Skip import statements — they don't get their own chunk, just tracked at file level
    if ast_kind == AstNodeKind::Import {
        return None;
    }

    let start = node.start_byte(); // use outer node (includes export keyword)
    let end = node.end_byte();
    let slice = text.get(start..end)?;
    let start_line = node.start_position().row + 1;
    let end_line = node.end_position().row + 1;

    // Extract symbol name
    let symbol = extract_symbol_name(effective_node, text);

    // Build qualified name
    let qualified_name = match (&symbol, parent_name) {
        (Some(sym), Some(parent)) => Some(format!("{}.{}", parent, sym)),
        (Some(sym), None) => Some(sym.clone()),
        _ => None,
    };

    // Extract signature (first meaningful line for functions, full declaration for types)
    let signature = extract_signature(effective_node, text);

    // Extract calls within this node
    let calls = extract_calls(effective_node, text);

    // Extract type references (TS type annotations, generic params)
    let type_refs = extract_type_refs(effective_node, text);

    // Extract doc comment from preceding sibling
    let doc_summary = extract_doc_comment(node, text);

    // Exports list
    let exports = if is_exported {
        symbol.iter().cloned().collect()
    } else {
        Vec::new()
    };

    // Filter file-level imports to identifiers actually referenced in this AST node.
    let referenced_identifiers = extract_identifier_refs(effective_node, text);
    let imports: Vec<String> = file_imports
        .iter()
        .filter(|imp| referenced_identifiers.contains(imp.as_str()))
        .cloned()
        .collect();

    // For classes, recurse into members
    if matches!(ast_kind, AstNodeKind::Class | AstNodeKind::Interface) {
        // We still emit the class as a single chunk (not splitting members)
        // but we do collect method-level calls/type_refs into the class chunk
    }

    Some(ChunkDraft {
        kind: ChunkKind::JavaScript,
        start_line,
        end_line,
        content: slice.trim().to_string(),
        heading_path: Vec::new(),
        symbol,
        address: None,
        ast_kind: Some(ast_kind),
        qualified_name,
        signature,
        parent_symbol: parent_name.map(|s| s.to_string()),
        imports,
        exports,
        calls,
        type_refs,
        doc_summary,
        symbol_id: None,
        symbol_tail: None,
        module_path: None,
        visibility: None,
    })
}

/// Collect method chunks from class bodies so methods become first-class searchable units.
#[cfg(feature = "treesitter")]
fn collect_class_member_drafts(
    node: Node,
    text: &str,
    file_imports: &[String],
    drafts: &mut Vec<ChunkDraft>,
) {
    let effective_node = if node.kind() == "export_statement" {
        find_inner_declaration(node).unwrap_or(node)
    } else {
        node
    };

    if !matches!(
        effective_node.kind(),
        "class_declaration" | "abstract_class_declaration"
    ) {
        return;
    }

    let Some(parent_name) = extract_symbol_name(effective_node, text) else {
        return;
    };
    let Some(body) = effective_node.child_by_field_name("body") else {
        return;
    };

    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() != "method_definition" {
            continue;
        }
        if let Some(draft) = enriched_draft_from_node(text, child, Some(parent_name.as_str()), file_imports) {
            drafts.push(draft);
        }
    }
}

// ── Tree-sitter extraction helpers ──────────────────────────────────────────

/// Get the text of a node's "name" field.
#[cfg(feature = "treesitter")]
fn node_name_text<'a>(node: Node<'a>, text: &'a str) -> Option<String> {
    node.child_by_field_name("name")
        .and_then(|n| text.get(n.start_byte()..n.end_byte()))
        .map(|s| s.to_string())
}

/// Extract the symbol name from various node structures.
#[cfg(feature = "treesitter")]
fn extract_symbol_name(node: Node, text: &str) -> Option<String> {
    // Direct name field (function_declaration, class_declaration, etc.)
    if let Some(name) = node_name_text(node, text) {
        return Some(name);
    }
    // For lexical_declaration: const foo = ...
    if let Some(name) = lexical_name(node, text) {
        return Some(name);
    }
    // For variable_declaration: var foo = ...
    if matches!(node.kind(), "variable_declaration") {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "variable_declarator" {
                return node_name_text(child, text);
            }
        }
    }
    None
}

/// Get variable name from a lexical_declaration (const/let).
#[cfg(feature = "treesitter")]
fn lexical_name(node: Node, text: &str) -> Option<String> {
    if !matches!(node.kind(), "lexical_declaration" | "variable_declaration") {
        return None;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "variable_declarator" {
            return node_name_text(child, text);
        }
    }
    None
}

/// Check if a lexical_declaration's value is an arrow function.
#[cfg(feature = "treesitter")]
fn has_arrow_function_value(node: Node) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "variable_declarator" {
            if let Some(value) = child.child_by_field_name("value") {
                return matches!(value.kind(), "arrow_function" | "function");
            }
        }
    }
    false
}

/// Find the inner declaration inside an export_statement.
#[cfg(feature = "treesitter")]
fn find_inner_declaration(export_node: Node) -> Option<Node> {
    let mut cursor = export_node.walk();
    for child in export_node.children(&mut cursor) {
        if is_chunkable_node(child) && child.kind() != "export_statement" {
            return Some(child);
        }
        // Also check for direct function/class/interface etc.
        if matches!(child.kind(),
            "function_declaration" | "class_declaration" | "interface_declaration"
            | "type_alias_declaration" | "enum_declaration" | "lexical_declaration"
            | "variable_declaration" | "abstract_class_declaration"
        ) {
            return Some(child);
        }
    }
    None
}

/// Extract function/method signature (params + return type annotation).
#[cfg(feature = "treesitter")]
fn extract_signature(node: Node, text: &str) -> Option<String> {
    match node.kind() {
        "function_declaration" | "generator_function_declaration" | "method_definition" => {
            // name(params): return_type
            let name = node_name_text(node, text).unwrap_or_else(|| "anonymous".to_string());
            let params = node.child_by_field_name("parameters")
                .and_then(|n| text.get(n.start_byte()..n.end_byte()))
                .unwrap_or("()");
            let return_type = node.child_by_field_name("return_type")
                .and_then(|n| text.get(n.start_byte()..n.end_byte()))
                .map(|rt| rt.trim_start_matches(':').trim());
            match return_type {
                Some(rt) => Some(format!("{}{}: {}", name, params, rt)),
                None => Some(format!("{}{}", name, params)),
            }
        }
        "lexical_declaration" | "variable_declaration" => {
            // For arrow functions: name = (params) => ...
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "variable_declarator" {
                    let name = node_name_text(child, text)?;
                    if let Some(value) = child.child_by_field_name("value") {
                        if matches!(value.kind(), "arrow_function" | "function") {
                            let params = value.child_by_field_name("parameters")
                                .and_then(|n| text.get(n.start_byte()..n.end_byte()))
                                .unwrap_or("()");
                            let return_type = value.child_by_field_name("return_type")
                                .and_then(|n| text.get(n.start_byte()..n.end_byte()))
                                .map(|rt| rt.trim_start_matches(':').trim());
                            return match return_type {
                                Some(rt) => Some(format!("{}{}: {}", name, params, rt)),
                                None => Some(format!("{}{}", name, params)),
                            };
                        }
                    }
                }
            }
            None
        }
        "interface_declaration" | "type_alias_declaration" => {
            // First line of the declaration
            let slice = text.get(node.start_byte()..node.end_byte())?;
            let first_line = slice.lines().next()?;
            Some(first_line.trim().to_string())
        }
        _ => None,
    }
}

/// Walk a node tree and collect all call_expression callee names.
#[cfg(feature = "treesitter")]
fn extract_calls(node: Node, text: &str) -> Vec<String> {
    let mut calls = Vec::new();
    let mut seen = std::collections::HashSet::new();
    collect_calls_recursive(node, text, &mut calls, &mut seen);
    calls
}

#[cfg(feature = "treesitter")]
fn collect_calls_recursive(
    node: Node,
    text: &str,
    calls: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    if node.kind() == "call_expression" {
        if let Some(callee) = node.child_by_field_name("function") {
            let callee_text = text.get(callee.start_byte()..callee.end_byte())
                .unwrap_or("")
                .trim();
            // Normalize: take the last segment for member expressions
            let name = callee_text.rsplit('.').next().unwrap_or(callee_text);
            if !name.is_empty() && name.len() < 100 && seen.insert(name.to_string()) {
                calls.push(name.to_string());
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_calls_recursive(child, text, calls, seen);
    }
}

/// Collect identifier references from a node for exact import attribution.
#[cfg(feature = "treesitter")]
fn extract_identifier_refs(node: Node, text: &str) -> std::collections::HashSet<String> {
    let mut refs = std::collections::HashSet::new();
    collect_identifier_refs_recursive(node, text, &mut refs);
    refs
}

#[cfg(feature = "treesitter")]
fn collect_identifier_refs_recursive(
    node: Node,
    text: &str,
    refs: &mut std::collections::HashSet<String>,
) {
    if node.kind() == "identifier" {
        if let Some(name) = text.get(node.start_byte()..node.end_byte()) {
            if !name.is_empty() {
                refs.insert(name.to_string());
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_identifier_refs_recursive(child, text, refs);
    }
}

/// Walk a node tree and collect TypeScript type references.
#[cfg(feature = "treesitter")]
fn extract_type_refs(node: Node, text: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut seen = std::collections::HashSet::new();
    collect_type_refs_recursive(node, text, &mut refs, &mut seen);
    refs
}

#[cfg(feature = "treesitter")]
fn collect_type_refs_recursive(
    node: Node,
    text: &str,
    refs: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    // type_identifier is used in TS for type references: `foo: MyType`
    // generic_type wraps parameterized types: `Promise<Foo>`
    if matches!(node.kind(), "type_identifier" | "predefined_type") {
        let type_text = text.get(node.start_byte()..node.end_byte())
            .unwrap_or("")
            .trim();
        // Skip built-in primitives
        if !matches!(type_text, "string" | "number" | "boolean" | "void" | "null"
            | "undefined" | "any" | "never" | "unknown" | "object" | "symbol" | "bigint")
            && !type_text.is_empty()
            && type_text.len() < 100
            && seen.insert(type_text.to_string())
        {
            refs.push(type_text.to_string());
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_type_refs_recursive(child, text, refs, seen);
    }
}

/// Extract doc comment from the node's preceding sibling (JSDoc or line comments).
#[cfg(feature = "treesitter")]
fn extract_doc_comment(node: Node, text: &str) -> Option<String> {
    let prev = node.prev_sibling()?;
    if !matches!(prev.kind(), "comment") {
        return None;
    }
    let comment_text = text.get(prev.start_byte()..prev.end_byte())?;

    // JSDoc: /** ... */
    if comment_text.starts_with("/**") {
        let inner = comment_text
            .trim_start_matches("/**")
            .trim_end_matches("*/")
            .lines()
            .map(|l| l.trim().trim_start_matches('*').trim())
            .filter(|l| !l.is_empty() && !l.starts_with('@'))
            .collect::<Vec<_>>()
            .join(" ");
        let first_sentence = inner.split(". ").next().unwrap_or(&inner);
        if first_sentence.is_empty() {
            None
        } else {
            Some(truncate_doc(first_sentence, 200))
        }
    }
    // Single line: // ...
    else if comment_text.starts_with("//") {
        let content = comment_text.trim_start_matches("//").trim();
        if content.is_empty() { None } else { Some(truncate_doc(content, 200)) }
    } else {
        None
    }
}

/// Collect all import specifiers at file scope.
#[cfg(feature = "treesitter")]
fn collect_file_imports(root: Node, text: &str) -> Vec<String> {
    let mut imports = Vec::new();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "import_statement" {
            collect_import_names(child, text, &mut imports);
        }
        // export { x } from '...' also imports
        if child.kind() == "export_statement" {
            let mut inner_cursor = child.walk();
            for inner in child.children(&mut inner_cursor) {
                if inner.kind() == "import_statement" {
                    collect_import_names(inner, text, &mut imports);
                }
            }
        }
    }
    imports
}

/// Extract imported names from an import statement node.
#[cfg(feature = "treesitter")]
fn collect_import_names(node: Node, text: &str, imports: &mut Vec<String>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "import_clause" => {
                let mut clause_cursor = child.walk();
                for cc in child.children(&mut clause_cursor) {
                    match cc.kind() {
                        "identifier" => {
                            if let Some(name) = text.get(cc.start_byte()..cc.end_byte()) {
                                imports.push(name.to_string());
                            }
                        }
                        "named_imports" => {
                            let mut named_cursor = cc.walk();
                            for spec in cc.children(&mut named_cursor) {
                                if spec.kind() == "import_specifier" {
                                    // Use alias if present, otherwise the name
                                    let alias = spec.child_by_field_name("alias")
                                        .or_else(|| spec.child_by_field_name("name"));
                                    if let Some(name_node) = alias {
                                        if let Some(name) = text.get(name_node.start_byte()..name_node.end_byte()) {
                                            imports.push(name.to_string());
                                        }
                                    }
                                }
                            }
                        }
                        "namespace_import" => {
                            // import * as Foo
                            if let Some(name_node) = cc.child_by_field_name("name") {
                                if let Some(name) = text.get(name_node.start_byte()..name_node.end_byte()) {
                                    imports.push(name.to_string());
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

/// Check if a function name looks like a test.
#[cfg(feature = "treesitter")]
fn is_test_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("test")
        || lower.starts_with("it_")
        || lower == "it"
        || lower == "describe"
        || lower.starts_with("describe_")
        || lower.starts_with("spec_")
}

/// Truncate a doc string to max chars on a word boundary.
#[cfg(feature = "treesitter")]
fn truncate_doc(s: &str, max: usize) -> String {
    if s.len() <= max { return s.to_string(); }
    let truncated: String = s.chars().take(max).collect();
    // Find last space for clean break
    match truncated.rfind(' ') {
        Some(pos) if pos > max / 2 => format!("{}...", &truncated[..pos]),
        _ => format!("{}...", truncated),
    }
}

#[cfg(feature = "treesitter")]
fn select_js_language(path: &str) -> Language {
    if path.ends_with(".ts") {
        tree_sitter_typescript::language_typescript()
    } else if path.ends_with(".tsx") {
        tree_sitter_typescript::language_tsx()
    } else {
        tree_sitter_javascript::language()
    }
}

struct ChunkFlushParams {
    start_line: usize,
    end_line: usize,
    kind: ChunkKind,
    symbol: Option<String>,
    address: Option<String>,
}

fn flush_chunk(
    drafts: &mut Vec<ChunkDraft>,
    buf: &mut Vec<String>,
    heading_path: &[String],
    params: ChunkFlushParams,
) {
    if buf.is_empty() || params.end_line < params.start_line {
        return;
    }
    let content = buf.join("\n");
    drafts.push(ChunkDraft::plain(
        params.kind, params.start_line, params.end_line,
        content.trim().to_string(),
        heading_path.to_vec(), params.symbol, params.address,
    ));
    buf.clear();
}

fn buffer_len(buf: &[String]) -> usize {
    buf.iter().map(|line| line.len() + 1).sum()
}

fn line_count(text: &str) -> usize {
    text.lines().count().max(1)
}

fn make_slug(
    path: &str,
    kind: ChunkKind,
    heading_path: &[String],
    symbol: &Option<String>,
    address: &Option<String>,
    start_line: usize,
    end_line: usize,
) -> String {
    let base_name = path.rsplit('/').next().unwrap_or(path);
    let base_stem = strip_extension(base_name);
    let base_limit = if kind == ChunkKind::Image { 72 } else { 28 };
    let base_slug = truncate_slug(&slugify(base_stem), base_limit);

    let raw_context = if let Some(last) = heading_path.last() {
        Some(last.as_str())
    } else if let Some(symbol) = symbol {
        Some(symbol.as_str())
    } else {
        address.as_ref().map(|address| address.as_str())
    };

    let context_slug = raw_context
        .map(slugify)
        .map(|ctx| strip_redundant_prefix(&ctx, &base_slug))
        .map(|ctx| truncate_slug(&ctx, 44))
        .filter(|ctx| !ctx.is_empty() && ctx != "chunk" && *ctx != base_slug);

    let mut slug = match context_slug {
        Some(ctx) => format!("{}--{}", base_slug, ctx),
        None => base_slug,
    };
    if kind == ChunkKind::Text {
        slug = format!("{}-l{}-{}", slug, start_line, end_line);
    }
    truncate_slug(&slug, 96)
}

fn strip_extension(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((stem, _ext)) if !stem.is_empty() => stem,
        _ => name,
    }
}

fn truncate_slug(input: &str, max_len: usize) -> String {
    // Truncate to max_len first
    let truncated: String = if input.len() <= max_len {
        input.to_string()
    } else {
        input.chars().take(max_len).collect()
    };

    // Trim dashes from both ends in one pass (O(n) instead of O(n^2))
    truncated.trim_matches('-').to_string()
}

fn strip_redundant_prefix(context: &str, base: &str) -> String {
    let mut ctx = context.to_string();
    let mut changed = true;
    while changed {
        changed = false;
        if let Some(rest) = ctx.strip_prefix(base) {
            let rest = rest.trim_start_matches('-');
            ctx = rest.to_string();
            changed = true;
        }
    }
    ctx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AstNodeKind, IngestOptions};

    #[test]
    fn test_js_function_enrichment() {
        let code = r#"
import { Router } from 'express';
import jwt from 'jsonwebtoken';

/** Verify a JWT token and return the decoded claims. */
function verifyToken(token, secret) {
    const decoded = jwt.verify(token, secret);
    console.log('verified');
    return decoded;
}
"#;
        let chunks = chunk_file("auth.js", code, ChunkKind::JavaScript, &IngestOptions::default());
        assert!(!chunks.is_empty(), "Should produce at least one chunk");

        let func_chunk = chunks.iter().find(|c| c.symbol.as_deref() == Some("verifyToken"))
            .expect("Should find verifyToken chunk");

        assert_eq!(func_chunk.ast_kind, Some(AstNodeKind::Function));
        assert!(func_chunk.signature.as_ref().unwrap().contains("verifyToken"));
        assert!(func_chunk.signature.as_ref().unwrap().contains("token"));
        assert!(func_chunk.doc_summary.as_ref().unwrap().contains("Verify a JWT token"));

        // Calls: should find verify, log
        assert!(func_chunk.calls.iter().any(|c| c == "verify"), "Should detect jwt.verify call: {:?}", func_chunk.calls);
        assert!(func_chunk.calls.iter().any(|c| c == "log"), "Should detect console.log call: {:?}", func_chunk.calls);
    }

    #[test]
    fn test_ts_arrow_function_enrichment() {
        let code = r#"
import { Request, Response } from 'express';

export const handleRequest = (req: Request, res: Response): void => {
    const data = parseBody(req);
    sendResponse(res, data);
};
"#;
        let chunks = chunk_file("handler.ts", code, ChunkKind::JavaScript, &IngestOptions::default());
        assert!(!chunks.is_empty(), "Should produce at least one chunk");

        let arrow_chunk = chunks.iter().find(|c| c.symbol.as_deref() == Some("handleRequest"))
            .expect("Should find handleRequest chunk");

        assert_eq!(arrow_chunk.ast_kind, Some(AstNodeKind::Function));
        assert!(!arrow_chunk.exports.is_empty(), "Should be marked as export");
        assert!(arrow_chunk.exports.contains(&"handleRequest".to_string()));

        // Calls
        assert!(arrow_chunk.calls.iter().any(|c| c == "parseBody"), "Should detect parseBody call: {:?}", arrow_chunk.calls);
        assert!(arrow_chunk.calls.iter().any(|c| c == "sendResponse"), "Should detect sendResponse call: {:?}", arrow_chunk.calls);
    }

    #[test]
    fn test_ts_interface_enrichment() {
        let code = r#"
export interface UserProfile {
    id: string;
    name: string;
    email: string;
    role: UserRole;
    createdAt: Date;
}
"#;
        let chunks = chunk_file("types.ts", code, ChunkKind::JavaScript, &IngestOptions::default());
        let iface = chunks.iter().find(|c| c.symbol.as_deref() == Some("UserProfile"))
            .expect("Should find UserProfile chunk");

        assert_eq!(iface.ast_kind, Some(AstNodeKind::Interface));
        assert!(!iface.exports.is_empty());

        // Type refs: should find UserRole, Date but not string
        assert!(iface.type_refs.iter().any(|t| t == "UserRole"), "Should find UserRole type ref: {:?}", iface.type_refs);
        assert!(iface.type_refs.iter().any(|t| t == "Date"), "Should find Date type ref: {:?}", iface.type_refs);
        assert!(!iface.type_refs.iter().any(|t| t == "string"), "Should not include primitive 'string': {:?}", iface.type_refs);
    }

    #[test]
    fn test_class_enrichment() {
        let code = r#"
/** User authentication service. */
export class AuthService {
    constructor(private db: Database) {}

    async login(email: string, password: string): Promise<Token> {
        const user = await this.db.findUser(email);
        const valid = await bcrypt.compare(password, user.hash);
        if (!valid) throw new Error('Invalid credentials');
        return generateToken(user);
    }
}
"#;
        let chunks = chunk_file("auth.ts", code, ChunkKind::JavaScript, &IngestOptions::default());
        let class_chunk = chunks.iter().find(|c| c.symbol.as_deref() == Some("AuthService"))
            .expect("Should find AuthService chunk");

        assert_eq!(class_chunk.ast_kind, Some(AstNodeKind::Class));
        assert!(class_chunk.doc_summary.as_ref().unwrap().contains("authentication service"));

        // Type refs from TS annotations
        assert!(class_chunk.type_refs.iter().any(|t| t == "Database"), "Should find Database type ref: {:?}", class_chunk.type_refs);
        assert!(class_chunk.type_refs.iter().any(|t| t == "Token" || t == "Promise"), "Should find Token or Promise type ref: {:?}", class_chunk.type_refs);
    }

    #[test]
    fn test_class_method_chunks_have_parent_qualified_names() {
        let code = r#"
export class AuthService {
    async login(email: string, password: string): Promise<Token> {
        return generateToken(email + password);
    }
}
"#;
        let chunks = chunk_file("auth.ts", code, ChunkKind::JavaScript, &IngestOptions::default());
        let method_chunk = chunks
            .iter()
            .find(|c| c.symbol.as_deref() == Some("login"))
            .expect("Should find class method chunk");

        assert_eq!(method_chunk.ast_kind, Some(AstNodeKind::Method));
        assert_eq!(method_chunk.parent_symbol.as_deref(), Some("AuthService"));
        assert_eq!(method_chunk.qualified_name.as_deref(), Some("AuthService.login"));
        assert!(
            method_chunk.calls.iter().any(|call| call == "generateToken"),
            "Should retain method call extraction: {:?}",
            method_chunk.calls
        );
    }

    #[test]
    fn test_import_attribution_uses_identifiers_not_substrings() {
        let code = r#"
import { it, parseBody } from "./helpers";

export const handleRequest = (req: Request) => {
    const title = "split";
    return parseBody(req.body);
};
"#;
        let chunks = chunk_file("handler.ts", code, ChunkKind::JavaScript, &IngestOptions::default());
        let handler_chunk = chunks
            .iter()
            .find(|c| c.symbol.as_deref() == Some("handleRequest"))
            .expect("Should find handleRequest chunk");

        assert!(
            handler_chunk.imports.iter().any(|name| name == "parseBody"),
            "Should attribute exact imported identifier: {:?}",
            handler_chunk.imports
        );
        assert!(
            !handler_chunk.imports.iter().any(|name| name == "it"),
            "Should not attribute substring-only import matches: {:?}",
            handler_chunk.imports
        );
    }

    #[test]
    fn test_plain_text_no_enrichment() {
        let text = "This is a plain text document.\nIt has no code structure.\n";
        let chunks = chunk_file("readme.txt", text, ChunkKind::Text, &IngestOptions::default());
        assert!(!chunks.is_empty());
        assert!(chunks[0].ast_kind.is_none());
        assert!(chunks[0].imports.is_empty());
        assert!(chunks[0].calls.is_empty());
    }

    #[test]
    fn test_markdown_no_enrichment() {
        let text = "# Hello\n\nSome markdown content.\n";
        let chunks = chunk_file("doc.md", text, ChunkKind::Markdown, &IngestOptions::default());
        assert!(!chunks.is_empty());
        assert!(chunks[0].ast_kind.is_none());
    }

    #[test]
    fn test_backward_compat_no_treesitter() {
        // Verify that JSON chunking still works correctly with new ChunkDraft fields
        let json = r#"{"name": "test", "value": 42}"#;
        let chunks = chunk_file("data.json", json, ChunkKind::Json, &IngestOptions::default());
        assert!(!chunks.is_empty());
        assert!(chunks[0].ast_kind.is_none());
        assert!(chunks[0].imports.is_empty());
    }
}
