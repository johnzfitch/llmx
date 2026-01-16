use crate::model::{Chunk, ChunkKind, IngestOptions};
use crate::util::{estimate_tokens, sha256_hex, short_id, slugify};
use regex::Regex;
use std::collections::BTreeMap;
#[cfg(feature = "treesitter")]
use tree_sitter::{Language, Node, Parser};

#[derive(Debug, Clone)]
struct ChunkDraft {
    kind: ChunkKind,
    start_line: usize,
    end_line: usize,
    content: String,
    heading_path: Vec<String>,
    symbol: Option<String>,
    address: Option<String>,
}

pub fn chunk_file(path: &str, text: &str, kind: ChunkKind, options: &IngestOptions) -> Vec<Chunk> {
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

fn finalize_chunks(path: &str, drafts: Vec<ChunkDraft>) -> Vec<Chunk> {
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
            kind: draft.kind,
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
    vec![ChunkDraft {
        kind: ChunkKind::Image,
        start_line: 1,
        end_line: 1,
        content: format!("Image: {name}\nSource: {path}"),
        heading_path: Vec::new(),
        symbol: None,
        address: None,
    }]
}

fn chunk_markdown(text: &str, options: &IngestOptions) -> Vec<ChunkDraft> {
    let mut drafts = Vec::new();
    let mut buf: Vec<String> = Vec::new();
    let mut heading_stack: Vec<String> = Vec::new();
    let mut current_heading = heading_stack.clone();
    let mut start_line = 1;
    let mut in_fence = false;
    let heading_re = Regex::new(r"^(#{1,6})\s+(.+)").unwrap();

    for (idx, line) in text.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
        }

        if !in_fence {
            if let Some(caps) = heading_re.captures(line) {
                flush_chunk(&mut drafts, &mut buf, start_line, line_no - 1, ChunkKind::Markdown, &current_heading, None, None);
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
            flush_chunk(&mut drafts, &mut buf, start_line, line_no, ChunkKind::Markdown, &current_heading, None, None);
            start_line = line_no + 1;
        }
    }

    flush_chunk(&mut drafts, &mut buf, start_line, line_count(text), ChunkKind::Markdown, &current_heading, None, None);
    drafts
}

fn chunk_text(text: &str, options: &IngestOptions) -> Vec<ChunkDraft> {
    let lines: Vec<&str> = text.lines().collect();
    let mut drafts = Vec::new();
    let mut buf: Vec<String> = Vec::new();
    let mut start_line = 1;
    for (idx, line) in lines.iter().enumerate() {
        let line_no = idx + 1;
        if line.trim().is_empty() && !buf.is_empty() {
            if buffer_len(&buf) >= options.chunk_target_chars {
                flush_chunk(&mut drafts, &mut buf, start_line, line_no, ChunkKind::Text, &[], None, None);
                start_line = line_no + 1;
            } else {
                buf.push(line.to_string());
            }
            continue;
        }
        buf.push((*line).to_string());
        if buffer_len(&buf) >= options.chunk_max_chars {
            flush_chunk(&mut drafts, &mut buf, start_line, line_no, ChunkKind::Text, &[], None, None);
            start_line = line_no + 1;
        }
    }
    flush_chunk(&mut drafts, &mut buf, start_line, line_count(text), ChunkKind::Text, &[], None, None);
    drafts
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
    let heading_re = Regex::new(r"(?i)<h([1-6])[^>]*>(.*?)</h[1-6]>").unwrap();
    let tag_re = Regex::new(r"<[^>]+>").unwrap();
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
            flush_chunk(&mut drafts, &mut buf, start_line, line_no - 1, ChunkKind::Html, &current_heading, None, None);
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
            flush_chunk(&mut drafts, &mut buf, start_line, line_no, ChunkKind::Html, &current_heading, None, None);
            start_line = line_no + 1;
        }
    }

    flush_chunk(&mut drafts, &mut buf, start_line, line_count(text), ChunkKind::Html, &current_heading, None, None);
    drafts
}

fn chunk_json(text: &str, options: &IngestOptions) -> Vec<ChunkDraft> {
    let mut drafts = Vec::new();
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
                let content = serde_json::to_string_pretty(&value).unwrap_or_default();
                drafts.push(ChunkDraft {
                    kind: ChunkKind::Json,
                    start_line: 1,
                    end_line: line_count,
                    content,
                    heading_path: vec![key.clone()],
                    symbol: Some(key),
                    address: Some(address),
                });
            }
        }
        serde_json::Value::Array(list) => {
            let mut start = 0usize;
            while start < list.len() {
                let end = (start + 50).min(list.len());
                let slice = &list[start..end];
                let address = format!("$[{}:{}]", start, end);
                let content = serde_json::to_string_pretty(&slice).unwrap_or_default();
                drafts.push(ChunkDraft {
                    kind: ChunkKind::Json,
                    start_line: 1,
                    end_line: line_count,
                    content,
                    heading_path: Vec::new(),
                    symbol: None,
                    address: Some(address),
                });
                start = end;
            }
        }
        _ => {
            drafts.push(ChunkDraft {
                kind: ChunkKind::Json,
                start_line: 1,
                end_line: line_count,
                content: serde_json::to_string_pretty(&value).unwrap_or_default(),
                heading_path: Vec::new(),
                symbol: None,
                address: Some("$".to_string()),
            });
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

    let mut cursor = tree.root_node().walk();
    let mut drafts = Vec::new();
    for child in tree.root_node().children(&mut cursor) {
        if !is_js_symbol_node(child) {
            continue;
        }
        if let Some(draft) = draft_from_node(text, child, ChunkKind::JavaScript) {
            drafts.push(draft);
        }
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

#[cfg(feature = "treesitter")]
fn draft_from_node(text: &str, node: Node, kind: ChunkKind) -> Option<ChunkDraft> {
    let start = node.start_byte();
    let end = node.end_byte();
    let slice = text.get(start..end)?;
    let start_line = node.start_position().row + 1;
    let end_line = node.end_position().row + 1;
    let symbol = node
        .child_by_field_name("name")
        .and_then(|n| text.get(n.start_byte()..n.end_byte()))
        .map(|s| s.to_string());

    Some(ChunkDraft {
        kind,
        start_line,
        end_line,
        content: slice.trim().to_string(),
        heading_path: Vec::new(),
        symbol,
        address: None,
    })
}

#[cfg(feature = "treesitter")]
fn is_js_symbol_node(node: Node) -> bool {
    matches!(
        node.kind(),
        "function_declaration" | "class_declaration" | "method_definition"
    )
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

fn flush_chunk(
    drafts: &mut Vec<ChunkDraft>,
    buf: &mut Vec<String>,
    start_line: usize,
    end_line: usize,
    kind: ChunkKind,
    heading_path: &[String],
    symbol: Option<String>,
    address: Option<String>,
) {
    if buf.is_empty() || end_line < start_line {
        return;
    }
    let content = buf.join("\n");
    drafts.push(ChunkDraft {
        kind,
        start_line,
        end_line,
        content: content.trim().to_string(),
        heading_path: heading_path.to_vec(),
        symbol,
        address,
    });
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
    } else if let Some(address) = address {
        Some(address.as_str())
    } else {
        None
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
    let mut out = if input.len() <= max_len {
        input.to_string()
    } else {
        input.chars().take(max_len).collect()
    };
    while out.ends_with('-') {
        out.pop();
    }
    while out.starts_with('-') {
        out.remove(0);
    }
    out
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
